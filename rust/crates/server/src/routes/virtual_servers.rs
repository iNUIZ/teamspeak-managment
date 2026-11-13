//! Virtual servers of a TeamSpeak instance. The encrypted snapshot backup is still to come.

use axum::extract::Path;
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde_json::Value;
use std::collections::HashMap;
use teamhub_ts_query::Priority;

use super::ts::{body_params, params, FlexJson};
use crate::audit::{run_remote_audited, RemoteAction};
use crate::auth::{AuthUser, ServerUser};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// Only these settings may be changed from the panel.
const ALLOWED_EDIT_PARAMS: [&str; 29] = [
    "virtualserver_name", "virtualserver_welcomemessage", "virtualserver_maxclients", "virtualserver_password",
    "virtualserver_hostmessage", "virtualserver_hostmessage_mode", "virtualserver_default_server_group",
    "virtualserver_default_channel_group", "virtualserver_default_channel_admin_group", "virtualserver_hostbanner_url",
    "virtualserver_hostbanner_gfx_url", "virtualserver_hostbanner_gfx_interval", "virtualserver_hostbanner_mode",
    "virtualserver_hostbutton_tooltip", "virtualserver_hostbutton_url", "virtualserver_hostbutton_gfx_url",
    "virtualserver_icon_id", "virtualserver_codec_encryption_mode", "virtualserver_needed_identity_security_level",
    "virtualserver_min_client_version", "virtualserver_antiflood_points_tick_reduce",
    "virtualserver_antiflood_points_needed_command_block", "virtualserver_antiflood_points_needed_ip_block",
    "virtualserver_log_client", "virtualserver_log_query", "virtualserver_log_channel", "virtualserver_log_permissions",
    "virtualserver_log_server", "virtualserver_log_filetransfer",
];

fn require_admin(user: &AuthUser) -> ApiResult<()> {
    if user.is_admin() {
        Ok(())
    } else {
        Err(ApiError::new(403, "Insufficient permissions"))
    }
}

fn sid_of(path: &HashMap<String, String>) -> ApiResult<u32> {
    path.get("sid")
        .and_then(|sid| sid.parse::<u32>().ok())
        .filter(|sid| *sid <= 65_535)
        .ok_or_else(|| ApiError::new(400, "Server id must be an integer between 0 and 65535"))
}

type VsPath = Path<HashMap<String, String>>;

async fn list(axum::extract::State(state): axum::extract::State<AppState>, server: ServerUser) -> ApiResult<Json<Value>> {
    Ok(Json(state.pool.get(server.config_id)?.execute(0, "serverlist", Vec::new(), Priority::High).await?))
}

async fn info(axum::extract::State(state): axum::extract::State<AppState>, server: ServerUser, Path(path): VsPath) -> ApiResult<Json<Value>> {
    Ok(Json(state.pool.get(server.config_id)?.execute(sid_of(&path)?, "serverinfo", Vec::new(), Priority::Normal).await?))
}

async fn edit(axum::extract::State(state): axum::extract::State<AppState>, server: ServerUser, Path(path): VsPath, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    require_admin(&server.user)?;
    let sid = sid_of(&path)?;
    let filtered: Vec<(String, String)> = body_params(&body).into_iter().filter(|(key, _)| ALLOWED_EDIT_PARAMS.contains(&key.as_str())).collect();
    let client = state.pool.get(server.config_id)?;
    let remote = RemoteAction { action: "virtual_server.edit", connection_id: server.config_id, virtual_server_id: Some(i64::from(sid)), target_type: "virtual_server", target_id: Some(sid.to_string()) };
    let dispatch = async { client.execute(sid, "serveredit", filtered, Priority::Normal).await.map_err(ApiError::from) };
    Ok(Json(run_remote_audited(&state.db, &server.user, remote, dispatch, |_| None).await?))
}

async fn create(axum::extract::State(state): axum::extract::State<AppState>, server: ServerUser, FlexJson(body): FlexJson) -> ApiResult<(StatusCode, Json<Value>)> {
    require_admin(&server.user)?;
    let result = state.pool.get(server.config_id)?.execute(0, "servercreate", body_params(&body), Priority::Normal).await?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn instance_action(state: &AppState, server: &ServerUser, sid: u32, action: &'static str, command: &str) -> ApiResult<Json<Value>> {
    require_admin(&server.user)?;
    let client = state.pool.get(server.config_id)?;
    let sid_text = sid.to_string();
    let remote = RemoteAction { action, connection_id: server.config_id, virtual_server_id: Some(i64::from(sid)), target_type: "virtual_server", target_id: Some(sid_text.clone()) };
    let dispatch = async { client.execute(0, command, params(&[("sid", &sid_text)]), Priority::Normal).await.map_err(ApiError::from) };
    Ok(Json(run_remote_audited(&state.db, &server.user, remote, dispatch, |_| None).await?))
}

async fn start(axum::extract::State(state): axum::extract::State<AppState>, server: ServerUser, Path(path): VsPath) -> ApiResult<Json<Value>> {
    instance_action(&state, &server, sid_of(&path)?, "virtual_server.start", "serverstart").await
}

async fn stop(axum::extract::State(state): axum::extract::State<AppState>, server: ServerUser, Path(path): VsPath) -> ApiResult<Json<Value>> {
    instance_action(&state, &server, sid_of(&path)?, "virtual_server.stop", "serverstop").await
}

async fn remove(axum::extract::State(state): axum::extract::State<AppState>, server: ServerUser, Path(path): VsPath) -> ApiResult<Json<Value>> {
    require_admin(&server.user)?;
    let sid = sid_of(&path)?.to_string();
    Ok(Json(state.pool.get(server.config_id)?.execute(0, "serverdelete", params(&[("sid", &sid)]), Priority::Normal).await?))
}

async fn deploy_snapshot(axum::extract::State(state): axum::extract::State<AppState>, server: ServerUser, Path(path): VsPath, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    require_admin(&server.user)?;
    Ok(Json(state.pool.get(server.config_id)?.execute_post(sid_of(&path)?, "serversnapshotdeploy", body_params(&body), Priority::Normal).await?))
}

async fn connection_info(axum::extract::State(state): axum::extract::State<AppState>, server: ServerUser, Path(path): VsPath) -> ApiResult<Json<Value>> {
    Ok(Json(state.pool.get(server.config_id)?.execute(sid_of(&path)?, "serverrequestconnectioninfo", Vec::new(), Priority::Normal).await?))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{sid}", put(edit).delete(remove))
        .route("/{sid}/info", get(info))
        .route("/{sid}/start", post(start))
        .route("/{sid}/stop", post(stop))
        .route("/{sid}/snapshot/deploy", post(deploy_snapshot))
        .route("/{sid}/connection-info", get(connection_info))
}
