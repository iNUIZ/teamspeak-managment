//! Sessions: the same JWTs as the old program, so nobody has to sign in again after the switch.

use std::time::Duration;

use axum::extract::{FromRequestParts, Path};
use axum::http::request::Parts;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// What a token holds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Claims {
    pub id: i64,
    pub username: String,
    pub role: String,
    pub iat: i64,
    pub exp: i64,
}

/// Parse `15m`, `7d`, `3600` the way the old token library did. Bare numbers are seconds.
pub fn parse_expiry(text: &str) -> Option<Duration> {
    let text = text.trim();
    let digits = text.find(|c: char| !c.is_ascii_digit()).unwrap_or(text.len());
    let value: u64 = text[..digits].parse().ok()?;
    let seconds = match text[digits..].trim().to_ascii_lowercase().as_str() {
        "" | "s" | "sec" | "secs" | "second" | "seconds" => 1,
        "m" | "min" | "mins" | "minute" | "minutes" => 60,
        "h" | "hr" | "hrs" | "hour" | "hours" => 3600,
        "d" | "day" | "days" => 86_400,
        "w" | "week" | "weeks" => 604_800,
        _ => return None,
    };
    Some(Duration::from_secs(value.checked_mul(seconds)?))
}

pub fn sign_access_token(secret: &str, expiry: &str, id: i64, username: &str, role: &str) -> ApiResult<String> {
    let now = chrono::Utc::now().timestamp();
    let lifetime = parse_expiry(expiry).unwrap_or(Duration::from_secs(15 * 60));
    let claims = Claims { id, username: username.to_owned(), role: role.to_owned(), iat: now, exp: now + lifetime.as_secs() as i64 };
    jsonwebtoken::encode(&Header::new(Algorithm::HS256), &claims, &EncodingKey::from_secret(secret.as_bytes())).map_err(ApiError::unexpected)
}

/// HS256 only: a token cannot choose its own algorithm.
pub fn verify_token(secret: &str, token: &str) -> Option<Claims> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.leeway = 0;
    jsonwebtoken::decode::<Claims>(token, &DecodingKey::from_secret(secret.as_bytes()), &validation).ok().map(|data| data.claims)
}

/// The signed-in user, with the role as it is now rather than when the token was made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthUser {
    pub id: i64,
    pub username: String,
    pub role: String,
}

impl AuthUser {
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let header = parts.headers.get(axum::http::header::AUTHORIZATION).and_then(|value| value.to_str().ok());
        let Some(token) = header.and_then(|value| value.strip_prefix("Bearer ")) else {
            return Err(ApiError::new(401, "No token provided"));
        };
        let Some(claims) = verify_token(&state.config.jwt_secret, token) else {
            return Err(ApiError::new(401, "Invalid or expired token"));
        };
        let Some(user) = state.db.user_by_id(claims.id).await.map_err(|_| ApiError::new(500, "Internal server error"))?.filter(|user| user.enabled) else {
            return Err(ApiError::new(401, "User account disabled or deleted"));
        };
        Ok(Self { id: user.id, username: claims.username, role: user.role })
    }
}

/// Only for admins.
#[derive(Debug, Clone)]
pub struct AdminUser(pub AuthUser);

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let user = AuthUser::from_request_parts(parts, state).await?;
        if !user.is_admin() {
            return Err(ApiError::new(403, "Insufficient permissions"));
        }
        Ok(Self(user))
    }
}

/// A user who may reach the server in the path's `configId`.
#[derive(Debug, Clone)]
pub struct ServerUser {
    pub user: AuthUser,
    pub config_id: i64,
}

impl FromRequestParts<AppState> for ServerUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let user = AuthUser::from_request_parts(parts, state).await?;
        let Path(params): Path<std::collections::HashMap<String, String>> =
            Path::from_request_parts(parts, state).await.map_err(|_| ApiError::new(400, "Invalid server config ID"))?;
        let config_id = params.get("configId").and_then(|id| id.parse::<i64>().ok()).ok_or_else(|| ApiError::new(400, "Invalid server config ID"))?;
        if !user.is_admin() && !state.db.has_server_access(user.id, config_id).await? {
            return Err(ApiError::new(403, "No access to this server"));
        }
        Ok(Self { user, config_id })
    }
}

/// `None` when acceptable, else the problem.
pub fn validate_password(password: &str) -> Option<&'static str> {
    if password.chars().count() < 8 {
        return Some("Password must be at least 8 characters long");
    }
    if !password.bytes().any(|b| b.is_ascii_uppercase()) {
        return Some("Password must contain at least one uppercase letter");
    }
    if !password.bytes().any(|b| b.is_ascii_lowercase()) {
        return Some("Password must contain at least one lowercase letter");
    }
    if !password.bytes().any(|b| b.is_ascii_digit()) {
        return Some("Password must contain at least one digit");
    }
    None
}

/// bcrypt takes a quarter second on purpose, so it runs away from the voice bots' tasks.
pub async fn hash_password(password: String) -> ApiResult<String> {
    tokio::task::spawn_blocking(move || teamhub_core::password::hash(&password)).await.map_err(ApiError::unexpected)?.map_err(ApiError::unexpected)
}

pub async fn verify_password(password: String, hash: String) -> ApiResult<bool> {
    tokio::task::spawn_blocking(move || teamhub_core::password::verify(&password, &hash)).await.map_err(ApiError::unexpected)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test-secret";

    #[test]
    fn expiry_texts_are_read() {
        assert_eq!(parse_expiry("15m"), Some(Duration::from_secs(900)));
        assert_eq!(parse_expiry("7d"), Some(Duration::from_secs(604_800)));
        assert_eq!(parse_expiry("3600"), Some(Duration::from_secs(3600)));
        assert_eq!(parse_expiry("2 hours"), Some(Duration::from_secs(7200)));
        assert_eq!(parse_expiry("soon"), None);
        assert_eq!(parse_expiry("5x"), None);
    }

    #[test]
    fn a_token_verifies_with_its_secret_only() {
        let token = sign_access_token(SECRET, "15m", 3, "ali", "admin").unwrap();
        let claims = verify_token(SECRET, &token).unwrap();
        assert_eq!((claims.id, claims.username.as_str(), claims.role.as_str()), (3, "ali", "admin"));
        assert_eq!(claims.exp - claims.iat, 900);
        assert!(verify_token("other", &token).is_none());
        assert!(verify_token(SECRET, "nonsense").is_none());
    }

    #[test]
    fn an_old_programs_token_is_accepted() {
        // Made by Node's jsonwebtoken with this secret, expiring in the year 2286.
        let token = jsonwebtoken::encode(
            &Header::new(Algorithm::HS256),
            &serde_json::json!({"id": 7, "username": "eski", "role": "viewer", "iat": 1_700_000_000, "exp": 9_999_999_999i64}),
            &EncodingKey::from_secret(SECRET.as_bytes()),
        )
        .unwrap();
        assert_eq!(verify_token(SECRET, &token).unwrap().username, "eski");
    }

    #[test]
    fn expired_tokens_and_other_algorithms_are_refused() {
        let key = EncodingKey::from_secret(SECRET.as_bytes());
        let claims = serde_json::json!({"id": 1, "username": "u", "role": "admin", "iat": 1, "exp": 2});
        let expired = jsonwebtoken::encode(&Header::new(Algorithm::HS256), &claims, &key).unwrap();
        assert!(verify_token(SECRET, &expired).is_none());
        let future = serde_json::json!({"id": 1, "username": "u", "role": "admin", "iat": 1, "exp": 9_999_999_999i64});
        let other = jsonwebtoken::encode(&Header::new(Algorithm::HS512), &future, &key).unwrap();
        assert!(verify_token(SECRET, &other).is_none());
    }

    #[test]
    fn passwords_need_all_kinds_of_character() {
        assert_eq!(validate_password("Aa1"), Some("Password must be at least 8 characters long"));
        assert_eq!(validate_password("aaaaaaa1"), Some("Password must contain at least one uppercase letter"));
        assert_eq!(validate_password("AAAAAAA1"), Some("Password must contain at least one lowercase letter"));
        assert_eq!(validate_password("Aaaaaaaa"), Some("Password must contain at least one digit"));
        assert_eq!(validate_password("Aaaaaaa1"), None);
    }
}
