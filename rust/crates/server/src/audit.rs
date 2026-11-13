//! Administrative audit rows. Typed constructors only: request bodies, addresses, headers,
//! credentials and error text never get in.

use std::future::Future;

use teamhub_db::{Db, OwnedAudit};

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};

const MAX_TARGET_ID_CHARS: usize = 64;
const MAX_USERNAME_CHARS: usize = 128;

/// A random id in the usual UUID form.
pub fn new_id() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("the system has random numbers");
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}

/// The row for a local change that succeeded, written in one transaction with it.
pub fn local_success(actor: &AuthUser, action: &'static str, target_type: &'static str, target_id: Option<String>) -> OwnedAudit {
    OwnedAudit {
        id: new_id(),
        operation_id: new_id(),
        actor_user_id: actor.id,
        actor_username: actor.username.chars().take(MAX_USERNAME_CHARS).collect(),
        action: action.to_owned(),
        connection_id: None,
        virtual_server_id: None,
        target_type: Some(target_type.to_owned()),
        target_id: target_id.filter(|id| !id.is_empty()).map(|id| id.chars().take(MAX_TARGET_ID_CHARS).collect()),
        outcome: "success".to_owned(),
        result_code: Some("ok".to_owned()),
        completed: true,
    }
}

/// What an audit row says about a remote action: on which server and what it touched.
pub struct RemoteAction {
    pub action: &'static str,
    pub connection_id: i64,
    pub virtual_server_id: Option<i64>,
    pub target_type: &'static str,
    pub target_id: Option<String>,
}

/// Write the attempt before sending anything to TeamSpeak; if that fails nothing is sent. The
/// result is recorded afterwards, and a lost completion never fails what already happened.
pub async fn run_remote_audited<T>(
    db: &Db,
    actor: &AuthUser,
    remote: RemoteAction,
    dispatch: impl Future<Output = ApiResult<T>>,
    resolve_target: impl FnOnce(&T) -> Option<String>,
) -> ApiResult<T> {
    let operation_id = new_id();
    db.begin_audit(&OwnedAudit {
        id: new_id(),
        operation_id: operation_id.clone(),
        actor_user_id: actor.id,
        actor_username: actor.username.chars().take(MAX_USERNAME_CHARS).collect(),
        action: remote.action.to_owned(),
        connection_id: Some(remote.connection_id),
        virtual_server_id: remote.virtual_server_id,
        target_type: Some(remote.target_type.to_owned()),
        target_id: remote.target_id.filter(|id| !id.is_empty()).map(|id| id.chars().take(MAX_TARGET_ID_CHARS).collect()),
        outcome: "pending".to_owned(),
        result_code: None,
        completed: false,
    })
    .await?;
    match dispatch.await {
        Ok(value) => {
            let target: Option<String> = resolve_target(&value).map(|id| id.chars().take(MAX_TARGET_ID_CHARS).collect());
            if let Err(error) = db.complete_audit(&operation_id, "success", "ok", target.as_deref()).await {
                tracing::warn!(%error, "An audit row could not be completed");
            }
            Ok(value)
        }
        Err(error) => {
            let (outcome, code) = classify(&error);
            if let Err(audit_error) = db.complete_audit(&operation_id, outcome, code, None).await {
                tracing::warn!(error = %audit_error, "An audit row could not be completed");
            }
            Err(error)
        }
    }
}

/// Only fixed codes reach the table, never error text.
fn classify(error: &ApiError) -> (&'static str, &'static str) {
    if error.code.is_some() {
        return ("failure", "ts_error");
    }
    match error.status.as_u16() {
        404 => ("failure", "not_found"),
        504 => ("unknown", "timeout"),
        503 | 429 => ("failure", "network_error"),
        400..=499 => ("failure", "validation_failed"),
        _ => ("failure", "unknown"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor() -> AuthUser {
        AuthUser { id: 1, username: "u".repeat(200), role: "admin".to_owned() }
    }

    #[test]
    fn ids_look_like_uuids_and_differ() {
        let (a, b) = (new_id(), new_id());
        assert_ne!(a, b);
        assert_eq!(a.len(), 36);
        assert_eq!(&a[14..15], "4");
    }

    #[test]
    fn failures_get_fixed_codes() {
        assert_eq!(classify(&ApiError::new(502, "x").code(2568)), ("failure", "ts_error"));
        assert_eq!(classify(&ApiError::new(404, "x")), ("failure", "not_found"));
        assert_eq!(classify(&ApiError::new(400, "secret text")), ("failure", "validation_failed"));
        assert_eq!(classify(&ApiError::new(504, "x")), ("unknown", "timeout"));
        assert_eq!(classify(&ApiError::new(500, "x")), ("failure", "unknown"));
    }

    #[test]
    fn long_values_are_cut() {
        let row = local_success(&actor(), "user.update", "user", Some("x".repeat(100)));
        assert_eq!(row.actor_username.len(), MAX_USERNAME_CHARS);
        assert_eq!(row.target_id.unwrap().len(), MAX_TARGET_ID_CHARS);
        assert_eq!(local_success(&actor(), "user.update", "user", Some(String::new())).target_id, None);
    }
}
