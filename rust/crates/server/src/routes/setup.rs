//! The first start: is an admin needed, and creating that admin.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};

use super::text_field;
use crate::auth::{hash_password, validate_password};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

async fn status(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    Ok(Json(json!({"needsSetup": state.db.user_count().await? == 0})))
}

async fn init(State(state): State<AppState>, Json(body): Json<Value>) -> ApiResult<(StatusCode, Json<Value>)> {
    // Only while no user exists.
    if state.db.user_count().await? > 0 {
        return Err(ApiError::new(403, "Setup already completed"));
    }
    let (Some(username), Some(password)) = (text_field(&body, "username"), text_field(&body, "password")) else {
        return Err(ApiError::new(400, "Username and password are required"));
    };
    if let Some(problem) = validate_password(password) {
        return Err(ApiError::new(400, problem));
    }
    let password_hash = hash_password(password.to_owned()).await?;
    let display_name = text_field(&body, "displayName").unwrap_or(username);
    state.db.create_user(username, &password_hash, display_name, "admin").await?;
    Ok((StatusCode::CREATED, Json(json!({"success": true, "message": "Admin account created. You can now log in."}))))
}

pub fn router() -> Router<AppState> {
    Router::new().route("/status", get(status)).route("/init", post(init))
}
