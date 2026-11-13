//! Managing panel users. Admins only. Passwords and their hashes never reach an audit row.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use serde_json::{json, Value};
use teamhub_db::UserChanges;

use super::text_field;
use crate::audit::local_success;
use crate::auth::{hash_password, validate_password, AdminUser};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

const VALID_ROLES: [&str; 2] = ["admin", "viewer"];

fn invalid_role() -> ApiError {
    ApiError::new(400, format!("Invalid role. Must be one of: {}", VALID_ROLES.join(", ")))
}

async fn list(State(state): State<AppState>, _admin: AdminUser) -> ApiResult<Json<Value>> {
    let users: Vec<Value> = state
        .db
        .users()
        .await?
        .into_iter()
        .map(|user| {
            json!({
                "id": user.id,
                "username": user.username,
                "displayName": user.display_name,
                "role": user.role,
                "enabled": user.enabled,
                "createdAt": user.created_at.to_iso(),
                "lastLoginAt": user.last_login_at.map(|at| at.to_iso()),
            })
        })
        .collect();
    Ok(Json(Value::Array(users)))
}

async fn create(State(state): State<AppState>, AdminUser(admin): AdminUser, Json(body): Json<Value>) -> ApiResult<(StatusCode, Json<Value>)> {
    let (Some(username), Some(password), Some(display_name)) = (text_field(&body, "username"), text_field(&body, "password"), text_field(&body, "displayName")) else {
        return Err(ApiError::new(400, "Username, password, and display name required"));
    };
    if let Some(problem) = validate_password(password) {
        return Err(ApiError::new(400, problem));
    }
    let role = text_field(&body, "role").unwrap_or("viewer");
    if !VALID_ROLES.contains(&role) {
        return Err(invalid_role());
    }
    let password_hash = hash_password(password.to_owned()).await?;
    let created = state
        .db
        .create_user_audited(username, &password_hash, display_name, role, |id| local_success(&admin, "user.create", "user", Some(id.to_string())))
        .await;
    match created {
        Ok(id) => Ok((StatusCode::CREATED, Json(json!({"id": id, "username": username})))),
        Err(error) if error.is_unique_violation() => Err(ApiError::new(409, "This username is already taken")),
        Err(error) => Err(error.into()),
    }
}

fn parse_user_id(raw: &str) -> ApiResult<i64> {
    raw.parse().map_err(|_| ApiError::new(400, "Invalid user ID"))
}

async fn update(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(user_id): Path<String>, Json(body): Json<Value>) -> ApiResult<StatusCode> {
    let id = parse_user_id(&user_id)?;
    let mut changes = UserChanges { display_name: body["displayName"].as_str().map(str::to_owned), ..UserChanges::default() };
    if !body["role"].is_null() {
        let role = body["role"].as_str().filter(|role| VALID_ROLES.contains(role)).ok_or_else(invalid_role)?;
        changes.role = Some(role.to_owned());
    }
    changes.enabled = body["enabled"].as_bool();
    if let Some(password) = text_field(&body, "password") {
        if let Some(problem) = validate_password(password) {
            return Err(ApiError::new(400, problem));
        }
        changes.password_hash = Some(hash_password(password.to_owned()).await?);
    }
    let audit = local_success(&admin, "user.update", "user", Some(id.to_string()));
    if !state.db.update_user_audited(id, &changes, &audit).await? {
        return Err(ApiError::new(404, "User not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn remove(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(user_id): Path<String>) -> ApiResult<StatusCode> {
    let id = parse_user_id(&user_id)?;
    if id == admin.id {
        return Err(ApiError::new(400, "Cannot delete your own account"));
    }
    let audit = local_success(&admin, "user.delete", "user", Some(id.to_string()));
    if !state.db.delete_user_audited(id, &audit).await? {
        return Err(ApiError::new(404, "User not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list).post(create)).route("/{user_id}", put(update).delete(remove))
}
