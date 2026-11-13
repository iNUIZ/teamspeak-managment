//! Signing in, keeping a session and changing a password.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde_json::{json, Value};
use teamhub_db::Timestamp;

use super::text_field;
use crate::auth::{hash_password, sign_access_token, validate_password, verify_password, AuthUser};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

const REFRESH_LIFETIME_DAYS: i64 = 7;
const FAMILY_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-";

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).expect("the system has random numbers");
    bytes
}

fn new_refresh_token() -> String {
    random_bytes::<64>().iter().map(|byte| format!("{byte:02x}")).collect()
}

fn new_family() -> String {
    random_bytes::<21>().iter().map(|byte| char::from(FAMILY_ALPHABET[usize::from(*byte) % 64])).collect()
}

fn refresh_expiry() -> Timestamp {
    Timestamp(chrono::Utc::now() + chrono::Duration::days(REFRESH_LIFETIME_DAYS))
}

async fn login(State(state): State<AppState>, Json(body): Json<Value>) -> ApiResult<Json<Value>> {
    let (Some(username), Some(password)) = (text_field(&body, "username"), text_field(&body, "password")) else {
        return Err(ApiError::new(400, "Username and password required"));
    };
    let invalid = || ApiError::new(401, "Invalid credentials");
    let user = state.db.user_by_username(username).await?.filter(|user| user.enabled).ok_or_else(invalid)?;
    if !verify_password(password.to_owned(), user.password_hash.clone()).await? {
        return Err(invalid());
    }
    let access_token = sign_access_token(&state.config.jwt_secret, &state.config.jwt_access_expiry, user.id, &user.username, &user.role)?;
    let refresh_token = new_refresh_token();
    state.db.create_refresh_token(&refresh_token, user.id, refresh_expiry(), &new_family()).await?;
    state.db.set_last_login(user.id).await?;
    Ok(Json(json!({
        "accessToken": access_token,
        "refreshToken": refresh_token,
        "user": {"id": user.id, "username": user.username, "displayName": user.display_name, "role": user.role},
    })))
}

/// Each refresh token works once. The check for a used token coming back is carried over as it
/// was, and it finds nothing: the used row is deleted below. See "Açık sorunlar" in TASARIM.md.
async fn refresh(State(state): State<AppState>, Json(body): Json<Value>) -> ApiResult<Json<Value>> {
    let Some(token) = text_field(&body, "refreshToken") else {
        return Err(ApiError::new(400, "Refresh token required"));
    };
    let invalid = || ApiError::new(401, "Invalid refresh token");
    let Some(stored) = state.db.refresh_token(token).await? else {
        if let Some(replaced) = state.db.refresh_token_replaced_by(token).await? {
            tracing::warn!(user_id = replaced.user_id, "Refresh token reuse detected. Revoking all tokens.");
            state.db.delete_user_refresh_tokens(replaced.user_id).await?;
        }
        return Err(invalid());
    };
    let user = state.db.user_by_id(stored.user_id).await?;
    let usable = stored.expires_at > Timestamp::now();
    let Some(user) = user.filter(|user| user.enabled && usable) else {
        state.db.delete_refresh_token(stored.id).await?;
        return Err(invalid());
    };

    let new_token = new_refresh_token();
    // A second tab refreshing at once may have removed this row meanwhile.
    if !state.db.mark_refresh_token_replaced(stored.id, &new_token).await? {
        return Err(invalid());
    }
    let family = stored.family.clone().unwrap_or_else(new_family);
    state.db.create_refresh_token(&new_token, user.id, refresh_expiry(), &family).await?;
    state.db.delete_refresh_token(stored.id).await?;

    let access_token = sign_access_token(&state.config.jwt_secret, &state.config.jwt_access_expiry, user.id, &user.username, &user.role)?;
    Ok(Json(json!({"accessToken": access_token, "refreshToken": new_token})))
}

async fn logout(State(state): State<AppState>, Json(body): Json<Value>) -> ApiResult<StatusCode> {
    if let Some(token) = text_field(&body, "refreshToken") {
        state.db.delete_refresh_token_value(token).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn me(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Value>> {
    let user = state.db.user_by_id(user.id).await?.ok_or_else(|| ApiError::new(404, "User not found"))?;
    Ok(Json(json!({"user": {"id": user.id, "username": user.username, "displayName": user.display_name, "role": user.role}})))
}

async fn change_password(State(state): State<AppState>, user: AuthUser, Json(body): Json<Value>) -> ApiResult<StatusCode> {
    let (Some(current), Some(new_password)) = (text_field(&body, "currentPassword"), text_field(&body, "newPassword")) else {
        return Err(ApiError::new(400, "Both passwords required"));
    };
    if let Some(problem) = validate_password(new_password) {
        return Err(ApiError::new(400, problem));
    }
    let stored = state.db.user_by_id(user.id).await?.ok_or_else(|| ApiError::new(404, "User not found"))?;
    if !verify_password(current.to_owned(), stored.password_hash).await? {
        return Err(ApiError::new(401, "Current password is incorrect"));
    }
    let hash = hash_password(new_password.to_owned()).await?;
    state.db.set_password_hash(user.id, &hash).await?;
    // Every session ends when the password changes.
    state.db.delete_user_refresh_tokens(user.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router(state: AppState) -> Router<AppState> {
    let limited_auth = axum::middleware::from_fn_with_state(state.clone(), crate::limits::auth_limit);
    let limited_me = axum::middleware::from_fn_with_state(state, crate::limits::me_limit);
    Router::new()
        .route("/login", post(login).route_layer(limited_auth.clone()))
        .route("/refresh", post(refresh).route_layer(limited_auth.clone()))
        .route("/password", put(change_password).route_layer(limited_auth))
        .route("/logout", post(logout))
        .route("/me", get(me).route_layer(limited_me))
}
