//! API errors, with the same JSON the panel already reads.

use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Map, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
    pub details: Option<String>,
    pub reason: Option<&'static str>,
    pub retryable: Option<bool>,
    pub retry_after_seconds: Option<u64>,
    /// TeamSpeak's own error number.
    pub code: Option<i64>,
    /// Finds the cause of an unexpected error in the log.
    pub error_id: Option<String>,
}

pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    pub fn new(status: u16, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            message: message.into(),
            details: None,
            reason: None,
            retryable: None,
            retry_after_seconds: None,
            code: None,
            error_id: None,
        }
    }

    pub fn details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    pub fn reason(mut self, reason: &'static str) -> Self {
        self.reason = Some(reason);
        self
    }

    pub fn retryable(mut self, retryable: bool) -> Self {
        self.retryable = Some(retryable);
        self
    }

    pub fn retry_after(mut self, seconds: u64) -> Self {
        self.retry_after_seconds = Some(seconds);
        self
    }

    pub fn code(mut self, code: i64) -> Self {
        self.code = Some(code);
        self
    }

    /// Something this program did not expect. The cause goes to the log under an id, and only
    /// the id reaches the user.
    pub fn unexpected(cause: impl std::fmt::Display) -> Self {
        let mut bytes = [0u8; 4];
        let _ = getrandom::fill(&mut bytes);
        let id: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        tracing::error!(error_id = %id, %cause, "Unexpected error");
        let mut error = Self::new(500, "Something went wrong")
            .details("Try again. If it keeps happening, check the server logs.")
            .reason("unexpected_error");
        error.error_id = Some(id);
        error
    }

    fn body(&self) -> Value {
        let mut body = Map::new();
        body.insert("error".to_owned(), json!(self.message));
        if let Some(details) = &self.details {
            body.insert("details".to_owned(), json!(details));
        }
        if let Some(reason) = self.reason {
            body.insert("reason".to_owned(), json!(reason));
        }
        if let Some(retryable) = self.retryable {
            body.insert("retryable".to_owned(), json!(retryable));
        }
        if let Some(seconds) = self.retry_after_seconds {
            body.insert("retryAfterSeconds".to_owned(), json!(seconds));
        }
        if let Some(code) = self.code {
            body.insert("code".to_owned(), json!(code));
        }
        if let Some(id) = &self.error_id {
            body.insert("errorId".to_owned(), json!(id));
        }
        Value::Object(body)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let retry_after = self.retry_after_seconds;
        let mut response = (self.status, Json(self.body())).into_response();
        if let Some(seconds) = retry_after {
            if let Ok(value) = HeaderValue::from_str(&seconds.to_string()) {
                response.headers_mut().insert(header::RETRY_AFTER, value);
            }
        }
        response
    }
}

impl From<teamhub_db::Error> for ApiError {
    fn from(error: teamhub_db::Error) -> Self {
        Self::unexpected(error)
    }
}

/// TeamSpeak's answer to a query, as the panel expects it.
impl From<teamhub_ts_query::Error> for ApiError {
    fn from(error: teamhub_ts_query::Error) -> Self {
        use teamhub_ts_query::Error as Query;
        match error {
            Query::Invalid(message) => Self::new(400, message),
            Query::Flood { retry_after_seconds } => Self::new(429, "TeamSpeak Query temporarily paused")
                .details(format!(
                    "TeamSpeak flood protection is active. TS6 Manager will retry automatically after the cooldown (about {retry_after_seconds}s). If this repeats, verify the TS6 Manager host/IP is present in the TeamSpeak Query allow-list."
                ))
                .reason("ts_query_flood")
                .retryable(true)
                .retry_after(retry_after_seconds),
            Query::Unavailable { retry_after_seconds } => Self::new(503, "TeamSpeak Query is still starting")
                .details(format!(
                    "The TeamSpeak server is up but Query is not ready yet (common for ~30–90s after a fresh compose start). Wait a few seconds and retry — about {retry_after_seconds}s is usually enough."
                ))
                .reason("ts_query_starting")
                .retryable(true)
                .retry_after(retry_after_seconds),
            Query::Api { code, message } => from_api_error(code, &message),
        }
    }
}

fn from_api_error(code: i64, message: &str) -> ApiError {
    let lower = message.to_lowercase();
    if code == 2052 || lower.contains("file input/output error") {
        return ApiError::new(502, "TeamSpeak log file unavailable")
            .details(format!(
                "TeamSpeak could not read the server logfile (error {code}: {message}). This is usually a log directory permission, lock, or rotation issue on the TeamSpeak host — not a Manager connection failure. Retry once after a few seconds, or check the TeamSpeak logs volume/permissions (see docs/public/troubleshooting.md)."
            ))
            .reason("ts_logview_io")
            .code(code);
    }
    if code == 2568 || lower.contains("insufficient client permission") || lower.contains("insufficient permission") {
        return ApiError::new(403, "Insufficient TeamSpeak permission for this action")
            .details(format!(
                "TeamSpeak denied the request (error {code}: {message}). Successful reads or connection diagnostics do not authorize writes — grant the Query identity the permissions required for this action."
            ))
            .reason("ts_permission_denied")
            .code(code);
    }
    ApiError::new(502, "TeamSpeak API Error").details(message).code(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use teamhub_ts_query::Error as Query;

    async fn parts(error: ApiError) -> (u16, Option<String>, Value) {
        let response = error.into_response();
        let status = response.status().as_u16();
        let retry = response.headers().get("retry-after").map(|value| value.to_str().unwrap().to_owned());
        let bytes = axum::body::to_bytes(response.into_body(), 65_536).await.unwrap();
        (status, retry, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn only_what_is_set_is_sent() {
        let (status, _, body) = parts(ApiError::new(400, "Bad")).await;
        assert_eq!(status, 400);
        assert_eq!(body, json!({"error": "Bad"}));
    }

    #[tokio::test]
    async fn a_flood_says_when_to_come_back() {
        let (status, retry, body) = parts(Query::Flood { retry_after_seconds: 60 }.into()).await;
        assert_eq!((status, retry.as_deref()), (429, Some("60")));
        assert_eq!(body["reason"], "ts_query_flood");
        assert_eq!(body["retryable"], true);
        assert_eq!(body["retryAfterSeconds"], 60);
    }

    #[tokio::test]
    async fn a_server_still_starting_is_503() {
        let (status, retry, body) = parts(Query::Unavailable { retry_after_seconds: 5 }.into()).await;
        assert_eq!((status, retry.as_deref()), (503, Some("5")));
        assert_eq!(body["reason"], "ts_query_starting");
    }

    #[tokio::test]
    async fn teamspeak_errors_keep_their_number() {
        let (status, _, body) = parts(Query::Api { code: 1281, message: "database empty result set".to_owned() }.into()).await;
        assert_eq!(status, 502);
        assert_eq!(body, json!({"error": "TeamSpeak API Error", "code": 1281, "details": "database empty result set"}));

        let (status, _, body) = parts(Query::Api { code: 2568, message: "insufficient client permissions".to_owned() }.into()).await;
        assert_eq!((status, body["reason"].as_str()), (403, Some("ts_permission_denied")));
        assert_eq!(body["code"], 2568);

        let (status, _, body) = parts(Query::Api { code: 2052, message: "file input/output error".to_owned() }.into()).await;
        assert_eq!((status, body["reason"].as_str()), (502, Some("ts_logview_io")));
    }

    #[tokio::test]
    async fn unexpected_errors_show_no_cause() {
        let (status, _, body) = parts(ApiError::unexpected("database secret path /x/y")).await;
        assert_eq!(status, 500);
        assert_eq!(body["reason"], "unexpected_error");
        assert_eq!(body["errorId"].as_str().unwrap().len(), 8);
        assert!(!body.to_string().contains("secret"));
    }
}
