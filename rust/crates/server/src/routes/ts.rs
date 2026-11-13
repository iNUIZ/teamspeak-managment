//! What the routes that talk to a TeamSpeak server share.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{FromRequestParts, Path};
use axum::http::request::Parts;
use serde_json::Value;
use teamhub_ts_query::{Priority, QueryParams, WebQueryClient};

use crate::audit::{run_remote_audited, RemoteAction};
use crate::auth::{AuthUser, ServerUser};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// A JSON body, where an empty one counts as `{}`: many panel actions send no body at all.
pub struct FlexJson(pub Value);

impl<S: Send + Sync> axum::extract::FromRequest<S> for FlexJson {
    type Rejection = ApiError;

    async fn from_request(request: axum::extract::Request, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = axum::body::Bytes::from_request(request, state).await.map_err(|_| ApiError::new(413, "The request is too large"))?;
        if bytes.is_empty() {
            return Ok(Self(Value::Object(serde_json::Map::new())));
        }
        serde_json::from_slice(&bytes).map(Self).map_err(|_| ApiError::new(400, "The request is not valid JSON"))
    }
}

/// One query parameter's text. Lists and objects are not query values, so they are left out.
fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

/// A JSON body as query parameters.
pub fn body_params(body: &Value) -> QueryParams {
    body.as_object().map_or_else(Vec::new, |object| object.iter().filter_map(|(key, value)| scalar(value).map(|value| (key.clone(), value))).collect())
}

/// Parameters from literal pairs.
pub fn params(pairs: &[(&str, &str)]) -> QueryParams {
    pairs.iter().map(|(key, value)| ((*key).to_owned(), (*value).to_owned())).collect()
}

/// A request on one virtual server, by someone who may reach that server.
pub struct VsRequest {
    pub user: AuthUser,
    pub config_id: i64,
    pub sid: u32,
    pub client: Arc<WebQueryClient>,
    pub state: AppState,
}

impl FromRequestParts<AppState> for VsRequest {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let ServerUser { user, config_id } = ServerUser::from_request_parts(parts, state).await?;
        let Path(path): Path<HashMap<String, String>> = Path::from_request_parts(parts, state).await.map_err(|_| ApiError::new(400, "Invalid path"))?;
        let sid = path
            .get("sid")
            .and_then(|sid| sid.parse::<u32>().ok())
            .filter(|sid| *sid <= 65_535)
            .ok_or_else(|| ApiError::new(400, "Server id must be an integer between 0 and 65535"))?;
        let client = state.pool.get(config_id)?;
        Ok(Self { user, config_id, sid, client, state: state.clone() })
    }
}

impl VsRequest {
    pub fn require_admin(&self) -> ApiResult<()> {
        if self.user.is_admin() {
            Ok(())
        } else {
            Err(ApiError::new(403, "Insufficient permissions"))
        }
    }

    pub async fn read(&self, command: &str, params: QueryParams) -> ApiResult<Value> {
        Ok(self.client.execute(self.sid, command, params, Priority::Normal).await?)
    }

    pub async fn read_urgent(&self, command: &str, params: QueryParams) -> ApiResult<Value> {
        Ok(self.client.execute(self.sid, command, params, Priority::High).await?)
    }

    /// An admin's change on TeamSpeak: only admins, and written to the audit trail. The
    /// parameters stay out of the trail, since they can hold passwords and messages.
    pub async fn change(&self, action: &'static str, target_type: &'static str, target_id: Option<String>, command: &str, params: QueryParams) -> ApiResult<Value> {
        self.change_resolving(action, target_type, target_id, command, params, |_| None).await
    }

    pub async fn change_resolving(
        &self,
        action: &'static str,
        target_type: &'static str,
        target_id: Option<String>,
        command: &str,
        params: QueryParams,
        resolve_target: impl FnOnce(&Value) -> Option<String>,
    ) -> ApiResult<Value> {
        self.require_admin()?;
        let remote = RemoteAction { action, connection_id: self.config_id, virtual_server_id: Some(i64::from(self.sid)), target_type, target_id };
        let dispatch = async { self.client.execute(self.sid, command, params, Priority::Normal).await.map_err(ApiError::from) };
        run_remote_audited(&self.state.db, &self.user, remote, dispatch, resolve_target).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bodies_become_query_values() {
        let found = body_params(&json!({"a": "x", "b": 1, "c": true, "d": null, "e": [1], "f": {"g": 1}}));
        assert_eq!(found, [("a".to_owned(), "x".to_owned()), ("b".to_owned(), "1".to_owned()), ("c".to_owned(), "true".to_owned())]);
        assert!(body_params(&json!("x")).is_empty());
    }
}
