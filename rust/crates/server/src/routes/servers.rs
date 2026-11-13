//! TeamSpeak server connections. Keys and passwords are saved encrypted, never reach an audit row
//! and never come back out. Connection tests, the demo server and ready-made bots are still to come.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{json, Value};
use teamhub_db::models::TsServerConfig;
use teamhub_db::ServerFields;
use teamhub_ts_query::host::{sanitize_host, validate_port};
use teamhub_ts_query::HostPolicy;

use super::text_field;
use crate::audit::local_success;
use crate::auth::{AdminUser, AuthUser};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

const DEFAULT_WEBQUERY_PORT: u16 = 10080;
const DEFAULT_SSH_PORT: u16 = 10022;
const DEFAULT_METRICS_PORT: u16 = 9187;

fn public_fields(server: &TsServerConfig, admin: bool) -> Value {
    let mut fields = json!({
        "id": server.id,
        "name": server.name,
        "host": server.host,
        "webqueryPort": server.webquery_port,
        "useHttps": server.use_https,
        "sshPort": server.ssh_port,
        "enabled": server.enabled,
        "isDemo": server.is_demo,
        "createdAt": server.created_at.to_iso(),
        "hasSshCredentials": server.ssh_username.is_some() && server.ssh_password.is_some(),
    });
    if admin {
        fields["metricsEnabled"] = json!(server.metrics_enabled);
        fields["metricsPort"] = json!(server.metrics_port);
        fields["metricsHost"] = json!(server.metrics_host);
    }
    fields
}

fn parse_id(raw: &str) -> ApiResult<i64> {
    raw.parse().map_err(|_| ApiError::new(400, "Invalid server config ID"))
}

/// A port from JSON: a number or numeric text; missing or empty takes the default.
fn port_field(body: &Value, name: &str, default: u16) -> ApiResult<i64> {
    let number = match &body[name] {
        Value::Null => None,
        Value::String(text) if text.is_empty() => None,
        Value::String(text) => Some(text.parse::<i64>().map_err(|_| ApiError::new(400, "Port must be an integer between 1 and 65535"))?),
        value => Some(value.as_i64().ok_or_else(|| ApiError::new(400, "Port must be an integer between 1 and 65535"))?),
    };
    Ok(i64::from(validate_port(number, default)?))
}

fn optional_metrics_host(body: &Value) -> ApiResult<Option<Option<String>>> {
    match &body["metricsHost"] {
        Value::Null if body.get("metricsHost").is_none() => Ok(None),
        Value::Null => Ok(Some(None)),
        Value::String(text) if text.is_empty() => Ok(Some(None)),
        Value::String(text) => Ok(Some(Some(sanitize_host(text, HostPolicy::from_env())?))),
        _ => Err(ApiError::new(400, "Invalid metrics host")),
    }
}

async fn list(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Value>> {
    let servers: Vec<Value> = state.db.servers().await?.iter().map(|server| public_fields(server, user.is_admin())).collect();
    Ok(Json(Value::Array(servers)))
}

async fn create(State(state): State<AppState>, AdminUser(admin): AdminUser, Json(body): Json<Value>) -> ApiResult<(StatusCode, Json<Value>)> {
    let (Some(name), Some(host), Some(api_key)) = (text_field(&body, "name"), text_field(&body, "host"), text_field(&body, "apiKey")) else {
        return Err(ApiError::new(400, "Name, host, and API key are required"));
    };
    let encrypt = |secret: &str| state.cipher.encrypt(secret).map_err(ApiError::unexpected);
    let fields = ServerFields {
        name: name.to_owned(),
        host: sanitize_host(host, HostPolicy::from_env())?,
        webquery_port: port_field(&body, "webqueryPort", DEFAULT_WEBQUERY_PORT)?,
        api_key: encrypt(api_key)?,
        use_https: body["useHttps"].as_bool().unwrap_or(false),
        ssh_port: port_field(&body, "sshPort", DEFAULT_SSH_PORT)?,
        ssh_username: text_field(&body, "sshUsername").map(str::to_owned),
        ssh_password: text_field(&body, "sshPassword").map(encrypt).transpose()?,
        enabled: true,
        metrics_enabled: body["metricsEnabled"].as_bool().unwrap_or(false),
        metrics_port: port_field(&body, "metricsPort", DEFAULT_METRICS_PORT)?,
        metrics_host: optional_metrics_host(&body)?.flatten(),
    };
    let operation = std::sync::Mutex::new(String::new());
    let id = state
        .db
        .create_server_audited(&fields, |id| {
            let mut row = local_success(&admin, "connection.create", "connection", Some(id.to_string()));
            row.connection_id = Some(id);
            operation.lock().expect("operation lock").clone_from(&row.operation_id);
            row
        })
        .await?;
    if let Err(error) = state.pool.add(id, &fields.host, fields.webquery_port, api_key, fields.use_https) {
        tracing::warn!(error = %error.message, "The new connection could not join the pool");
        let operation = operation.lock().expect("operation lock").clone();
        state.db.mark_audit_partial(&operation, "pool_refresh_failed").await?;
    }
    // The same stations and bots as a fresh install; never holds up the answer.
    let (db, bots) = (state.db.clone(), state.bots.clone());
    tokio::spawn(async move { crate::bootstrap::seed_server(&db, bots.as_ref(), id).await });
    Ok((StatusCode::CREATED, Json(json!({"id": id, "name": name}))))
}

async fn details(State(state): State<AppState>, user: AuthUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let server = state.db.server(parse_id(&id)?).await?.ok_or_else(|| ApiError::new(404, "Server config not found"))?;
    Ok(Json(public_fields(&server, user.is_admin())))
}

/// The edit form leaves secrets and the SSH username empty when they stay as they are.
async fn update(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(id): Path<String>, Json(body): Json<Value>) -> ApiResult<Json<Value>> {
    let id = parse_id(&id)?;
    let current = state.db.server(id).await?.ok_or_else(|| ApiError::new(404, "Server config not found"))?;
    let mut fields = ServerFields::from_row(&current);
    if let Some(name) = body["name"].as_str() {
        fields.name = name.to_owned();
    }
    if let Some(host) = body["host"].as_str() {
        fields.host = sanitize_host(host, HostPolicy::from_env())?;
    }
    if body.get("webqueryPort").is_some() {
        fields.webquery_port = port_field(&body, "webqueryPort", DEFAULT_WEBQUERY_PORT)?;
    }
    if body.get("sshPort").is_some() {
        fields.ssh_port = port_field(&body, "sshPort", DEFAULT_SSH_PORT)?;
    }
    if body.get("metricsPort").is_some() {
        fields.metrics_port = port_field(&body, "metricsPort", DEFAULT_METRICS_PORT)?;
    }
    if let Some(metrics_host) = optional_metrics_host(&body)? {
        fields.metrics_host = metrics_host;
    }
    if let Some(flag) = body["useHttps"].as_bool() {
        fields.use_https = flag;
    }
    if let Some(flag) = body["enabled"].as_bool() {
        fields.enabled = flag;
    }
    if let Some(flag) = body["metricsEnabled"].as_bool() {
        fields.metrics_enabled = flag;
    }
    if let Some(username) = text_field(&body, "sshUsername") {
        fields.ssh_username = Some(username.to_owned());
    }
    let mut credentials_changed = false;
    if let Some(key) = text_field(&body, "apiKey") {
        fields.api_key = state.cipher.encrypt(key).map_err(ApiError::unexpected)?;
        credentials_changed = true;
    }
    if let Some(password) = text_field(&body, "sshPassword") {
        fields.ssh_password = Some(state.cipher.encrypt(password).map_err(ApiError::unexpected)?);
        credentials_changed = true;
    }
    let action = if credentials_changed { "connection.credentials_changed" } else { "connection.update" };
    let mut audit = local_success(&admin, action, "connection", Some(id.to_string()));
    audit.connection_id = Some(id);
    if !state.db.update_server_audited(id, &fields, &audit).await? {
        return Err(ApiError::new(404, "Server config not found"));
    }
    if let Err(error) = state.pool.refresh(id, &state.db, &state.cipher).await {
        tracing::warn!(error = %error.message, "The changed connection could not rejoin the pool");
        state.db.mark_audit_partial(&audit.operation_id, "pool_refresh_failed").await?;
    }
    Ok(Json(json!({"id": id, "name": fields.name})))
}

async fn remove(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(id): Path<String>) -> ApiResult<StatusCode> {
    let id = parse_id(&id)?;
    let mut audit = local_success(&admin, "connection.delete", "connection", Some(id.to_string()));
    audit.connection_id = Some(id);
    if !state.db.delete_server_audited(id, &audit).await? {
        return Err(ApiError::new(404, "Server config not found"));
    }
    state.pool.remove(id);
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list).post(create)).route("/{id}", get(details).put(update).delete(remove))
}
