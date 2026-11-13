//! The routes that mostly hand a request on to TeamSpeak: groups, permissions, bans, privilege
//! keys, complaints, messages and the instance. Parameters that can hold passwords, reasons or
//! keys stay out of the audit trail; only ids go in.

use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use teamhub_ts_query::{Priority, QueryParams};

use super::ts::{body_params, params, FlexJson, VsRequest};
use crate::auth::{AuthUser, ServerUser};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

type Ids = Path<HashMap<String, String>>;
type Created = (StatusCode, Json<Value>);

fn id<'a>(path: &'a HashMap<String, String>, name: &str) -> &'a str {
    path.get(name).map_or("", String::as_str)
}

/// The path decides which object: a body cannot name another one.
fn with(name: &str, value: &str, body: &Value) -> QueryParams {
    let mut all = params(&[(name, value)]);
    all.extend(body_params(body).into_iter().filter(|(key, _)| key != name));
    all
}

fn first_field(answer: &Value, field: &str) -> Option<String> {
    let value = &answer[0][field];
    value.as_str().map(str::to_owned).or_else(|| value.as_i64().map(|number| number.to_string()))
}

// ─── Server groups ─────────────────────────────────────────────

async fn sg_list(r: VsRequest) -> ApiResult<Json<Value>> {
    Ok(Json(r.read_urgent("servergrouplist", Vec::new()).await?))
}

async fn sg_create(r: VsRequest, FlexJson(body): FlexJson) -> ApiResult<Created> {
    let answer = r.change_resolving("server_group.create", "server_group", None, "servergroupadd", body_params(&body), |a| first_field(a, "sgid")).await?;
    Ok((StatusCode::CREATED, Json(answer)))
}

async fn sg_rename(r: VsRequest, Path(p): Ids, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let sgid = id(&p, "sgid");
    Ok(Json(r.change("server_group.update", "server_group", Some(sgid.to_owned()), "servergrouprename", with("sgid", sgid, &body)).await?))
}

async fn sg_delete(r: VsRequest, Path(p): Ids) -> ApiResult<Json<Value>> {
    let sgid = id(&p, "sgid");
    Ok(Json(r.change("server_group.delete", "server_group", Some(sgid.to_owned()), "servergroupdel", params(&[("sgid", sgid), ("force", "1")])).await?))
}

async fn sg_copy(r: VsRequest, Path(p): Ids, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let sgid = id(&p, "sgid");
    Ok(Json(r.change("server_group.copy", "server_group", Some(sgid.to_owned()), "servergroupcopy", with("ssgid", sgid, &body)).await?))
}

async fn sg_members(r: VsRequest, Path(p): Ids) -> ApiResult<Json<Value>> {
    Ok(Json(r.read("servergroupclientlist", params(&[("sgid", id(&p, "sgid")), ("-names", "")])).await?))
}

async fn sg_add_member(r: VsRequest, Path(p): Ids, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let sgid = id(&p, "sgid");
    let mut sent = params(&[("sgid", sgid)]);
    sent.extend(body_params(&body).into_iter().filter(|(key, _)| key == "cldbid"));
    Ok(Json(r.change("server_group.member_add", "server_group", Some(sgid.to_owned()), "servergroupaddclient", sent).await?))
}

async fn sg_remove_member(r: VsRequest, Path(p): Ids) -> ApiResult<Json<Value>> {
    let sgid = id(&p, "sgid");
    Ok(Json(r.change("server_group.member_remove", "server_group", Some(sgid.to_owned()), "servergroupdelclient", params(&[("sgid", sgid), ("cldbid", id(&p, "cldbid"))])).await?))
}

async fn sg_permissions(r: VsRequest, Path(p): Ids) -> ApiResult<Json<Value>> {
    Ok(Json(r.read("servergrouppermlist", params(&[("sgid", id(&p, "sgid")), ("-permsid", "")])).await?))
}

async fn sg_add_permission(r: VsRequest, Path(p): Ids, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let sgid = id(&p, "sgid");
    Ok(Json(r.change("server_group.permission_add", "server_group", Some(sgid.to_owned()), "servergroupaddperm", with("sgid", sgid, &body)).await?))
}

async fn sg_delete_permission(r: VsRequest, Path(p): Ids, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let sgid = id(&p, "sgid");
    Ok(Json(r.change("server_group.permission_delete", "server_group", Some(sgid.to_owned()), "servergroupdelperm", with("sgid", sgid, &body)).await?))
}

pub fn server_groups() -> Router<AppState> {
    Router::new()
        .route("/", get(sg_list).post(sg_create))
        .route("/{sgid}", put(sg_rename).delete(sg_delete))
        .route("/{sgid}/copy", post(sg_copy))
        .route("/{sgid}/members", get(sg_members).post(sg_add_member))
        .route("/{sgid}/members/{cldbid}", delete(sg_remove_member))
        .route("/{sgid}/permissions", get(sg_permissions).put(sg_add_permission).delete(sg_delete_permission))
}

// ─── Channel groups ────────────────────────────────────────────

async fn cg_list(r: VsRequest) -> ApiResult<Json<Value>> {
    Ok(Json(r.read_urgent("channelgrouplist", Vec::new()).await?))
}

async fn cg_create(r: VsRequest, FlexJson(body): FlexJson) -> ApiResult<Created> {
    let answer = r.change_resolving("channel_group.create", "channel_group", None, "channelgroupadd", body_params(&body), |a| first_field(a, "cgid")).await?;
    Ok((StatusCode::CREATED, Json(answer)))
}

async fn cg_rename(r: VsRequest, Path(p): Ids, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let cgid = id(&p, "cgid");
    Ok(Json(r.change("channel_group.update", "channel_group", Some(cgid.to_owned()), "channelgrouprename", with("cgid", cgid, &body)).await?))
}

async fn cg_delete(r: VsRequest, Path(p): Ids) -> ApiResult<Json<Value>> {
    let cgid = id(&p, "cgid");
    Ok(Json(r.change("channel_group.delete", "channel_group", Some(cgid.to_owned()), "channelgroupdel", params(&[("cgid", cgid), ("force", "1")])).await?))
}

async fn cg_clients(r: VsRequest, Path(p): Ids) -> ApiResult<Json<Value>> {
    Ok(Json(r.read("channelgroupclientlist", params(&[("cgid", id(&p, "cgid"))])).await?))
}

async fn cg_assign(r: VsRequest, Path(p): Ids, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let cgid = id(&p, "cgid");
    Ok(Json(r.change("channel_group.assign", "channel_group", Some(cgid.to_owned()), "setclientchannelgroup", with("cgid", cgid, &body)).await?))
}

async fn cg_permissions(r: VsRequest, Path(p): Ids) -> ApiResult<Json<Value>> {
    Ok(Json(r.read("channelgrouppermlist", params(&[("cgid", id(&p, "cgid")), ("-permsid", "")])).await?))
}

async fn cg_add_permission(r: VsRequest, Path(p): Ids, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let cgid = id(&p, "cgid");
    Ok(Json(r.change("channel_group.permission_add", "channel_group", Some(cgid.to_owned()), "channelgroupaddperm", with("cgid", cgid, &body)).await?))
}

async fn cg_delete_permission(r: VsRequest, Path(p): Ids, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let cgid = id(&p, "cgid");
    Ok(Json(r.change("channel_group.permission_delete", "channel_group", Some(cgid.to_owned()), "channelgroupdelperm", with("cgid", cgid, &body)).await?))
}

pub fn channel_groups() -> Router<AppState> {
    Router::new()
        .route("/", get(cg_list).post(cg_create))
        .route("/{cgid}", put(cg_rename).delete(cg_delete))
        .route("/{cgid}/clients", get(cg_clients))
        .route("/{cgid}/assign", post(cg_assign))
        .route("/{cgid}/permissions", get(cg_permissions).put(cg_add_permission).delete(cg_delete_permission))
}

// ─── Permissions ───────────────────────────────────────────────

async fn perm_list(r: VsRequest) -> ApiResult<Json<Value>> {
    Ok(Json(r.read("permissionlist", Vec::new()).await?))
}

async fn perm_find(r: VsRequest, Query(q): Query<HashMap<String, String>>) -> ApiResult<Json<Value>> {
    let sent: QueryParams = ["permid", "permsid"].iter().filter_map(|key| q.get(*key).filter(|v| !v.is_empty()).map(|v| ((*key).to_owned(), v.clone()))).collect();
    Ok(Json(r.read("permfind", sent).await?))
}

async fn perm_overview(r: VsRequest, Path(p): Ids, Query(q): Query<HashMap<String, String>>) -> ApiResult<Json<Value>> {
    let or_zero = |key: &str| q.get(key).filter(|v| !v.is_empty()).cloned().unwrap_or_else(|| "0".to_owned());
    Ok(Json(r.read("permoverview", params(&[("cldbid", id(&p, "cldbid")), ("cid", &or_zero("cid")), ("permid", &or_zero("permid"))])).await?))
}

pub fn permissions() -> Router<AppState> {
    Router::new().route("/", get(perm_list)).route("/find", get(perm_find)).route("/overview/{cldbid}", get(perm_overview))
}

// ─── Bans ──────────────────────────────────────────────────────

async fn ban_list(r: VsRequest) -> ApiResult<Json<Value>> {
    r.require_admin()?;
    Ok(Json(r.read("banlist", Vec::new()).await?))
}

async fn ban_add(r: VsRequest, FlexJson(body): FlexJson) -> ApiResult<Created> {
    let answer = r.change_resolving("ban.create", "ban", None, "banadd", body_params(&body), |a| first_field(a, "banid")).await?;
    Ok((StatusCode::CREATED, Json(answer)))
}

async fn ban_delete(r: VsRequest, Path(p): Ids) -> ApiResult<Json<Value>> {
    let banid = id(&p, "banid");
    Ok(Json(r.change("ban.delete", "ban", Some(banid.to_owned()), "bandel", params(&[("banid", banid)])).await?))
}

async fn ban_delete_all(r: VsRequest) -> ApiResult<Json<Value>> {
    Ok(Json(r.change("ban.delete_all", "ban", None, "bandelall", Vec::new()).await?))
}

pub fn bans() -> Router<AppState> {
    Router::new().route("/", get(ban_list).post(ban_add).delete(ban_delete_all)).route("/{banid}", delete(ban_delete))
}

// ─── Privilege keys ────────────────────────────────────────────

/// The key is a secret; this lets the panel tell keys apart. Same form as the old program.
pub fn key_fingerprint(key: &str) -> String {
    Sha256::digest(key.as_bytes()).iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprints_are_the_full_sha256_like_the_old_program() {
        assert_eq!(key_fingerprint("abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}

async fn token_list(r: VsRequest) -> ApiResult<Json<Value>> {
    r.require_admin()?;
    let mut rows = r.read("privilegekeylist", Vec::new()).await?;
    if let Some(rows) = rows.as_array_mut() {
        for row in rows {
            let fingerprint = row["token"].as_str().map(key_fingerprint);
            row["token_fingerprint"] = json!(fingerprint);
        }
    }
    Ok(Json(rows))
}

async fn token_create(r: VsRequest, FlexJson(body): FlexJson) -> ApiResult<Created> {
    let invalid = || ApiError::new(400, "Invalid privilege key parameters");
    let kind = body["tokentype"].as_i64().filter(|kind| matches!(kind, 0 | 1)).ok_or_else(invalid)?;
    let id1 = body["tokenid1"].as_i64().filter(|id| *id >= 1).ok_or_else(invalid)?;
    let id2 = if body.get("tokenid2").is_some() { body["tokenid2"].as_i64().ok_or_else(invalid)? } else { 0 };
    let description = match body.get("tokendescription") {
        None => "",
        Some(value) => value.as_str().ok_or_else(invalid)?,
    };
    if id2 < 0 || (kind == 1 && id2 < 1) || description.chars().count() > 255 {
        return Err(invalid());
    }
    let sent = params(&[("tokentype", &kind.to_string()), ("tokenid1", &id1.to_string()), ("tokenid2", &id2.to_string()), ("tokendescription", description)]);
    // The key in the answer never reaches the audit trail.
    let answer = r.change("privilege_key.create", "privilege_key", None, "privilegekeyadd", sent).await?;
    Ok((StatusCode::CREATED, Json(answer)))
}

async fn token_delete(r: VsRequest, Path(p): Ids) -> ApiResult<Json<Value>> {
    // The key is a secret in the path: it is not stored as the target.
    Ok(Json(r.change("privilege_key.delete", "privilege_key", None, "privilegekeydelete", params(&[("token", id(&p, "token"))])).await?))
}

pub fn tokens() -> Router<AppState> {
    Router::new().route("/", get(token_list).post(token_create)).route("/{token}", delete(token_delete))
}

// ─── Complaints ────────────────────────────────────────────────

async fn complaint_list(r: VsRequest, Query(q): Query<HashMap<String, String>>) -> ApiResult<Json<Value>> {
    let sent = q.get("tcldbid").filter(|v| !v.is_empty()).map_or_else(Vec::new, |v| params(&[("tcldbid", v)]));
    Ok(Json(r.read("complainlist", sent).await?))
}

async fn complaint_add(r: VsRequest, FlexJson(body): FlexJson) -> ApiResult<Created> {
    r.require_admin()?;
    Ok((StatusCode::CREATED, Json(r.read("complainadd", body_params(&body)).await?)))
}

async fn complaint_delete(r: VsRequest, Path(p): Ids) -> ApiResult<Json<Value>> {
    r.require_admin()?;
    Ok(Json(r.read("complaindel", params(&[("tcldbid", id(&p, "tcldbid")), ("fcldbid", id(&p, "fcldbid"))])).await?))
}

pub fn complaints() -> Router<AppState> {
    Router::new().route("/", get(complaint_list).post(complaint_add)).route("/{tcldbid}/{fcldbid}", delete(complaint_delete))
}

// ─── Messages ──────────────────────────────────────────────────

fn is_uid(text: &str) -> bool {
    (16..=128).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='))
}

async fn message_list(r: VsRequest) -> ApiResult<Json<Value>> {
    Ok(Json(r.read("messagelist", Vec::new()).await?))
}

async fn message_sender(r: VsRequest) -> ApiResult<Json<Value>> {
    r.require_admin()?;
    let answer = r.read("whoami", Vec::new()).await?;
    let sender = if answer.is_array() { &answer[0] } else { &answer };
    let text = |key: &str| sender[key].as_str().filter(|value| !value.is_empty()).map(str::to_owned);
    Ok(Json(json!({"nickname": text("client_nickname"), "loginName": text("client_login_name")})))
}

async fn message_get(r: VsRequest, Path(p): Ids) -> ApiResult<Json<Value>> {
    Ok(Json(r.read("messageget", params(&[("msgid", id(&p, "msgid"))])).await?))
}

async fn message_send(r: VsRequest, FlexJson(body): FlexJson) -> ApiResult<Created> {
    r.require_admin()?;
    let mode = body["mode"].as_str().unwrap_or("offline").to_owned();
    if mode != "offline" && mode != "chat" {
        return Err(ApiError::new(400, "Invalid message delivery mode"));
    }
    let message = body["message"].as_str().filter(|text| !text.trim().is_empty() && text.chars().count() <= 8192).ok_or_else(|| ApiError::new(400, "Message must contain between 1 and 8192 characters"))?;
    if !body["expectedUid"].is_null() && !body["expectedUid"].as_str().is_some_and(is_uid) {
        return Err(ApiError::new(400, "Invalid recipient UID"));
    }
    let remote = crate::audit::RemoteAction {
        action: "client.message",
        connection_id: r.config_id,
        virtual_server_id: Some(i64::from(r.sid)),
        target_type: "client",
        target_id: None,
    };
    if mode == "chat" {
        let clid = body["clid"].as_i64().filter(|clid| *clid >= 1).ok_or_else(|| ApiError::new(400, "Select an online recipient"))?.to_string();
        let expected = body["expectedUid"].as_str();
        let remote = crate::audit::RemoteAction { target_id: Some(clid.clone()), ..remote };
        let dispatch = async {
            let rows = r.read("clientinfo", params(&[("clid", &clid)])).await?;
            let recipient = if rows.is_array() { &rows[0] } else { &rows };
            if expected.is_some_and(|uid| recipient["client_unique_identifier"].as_str() != Some(uid)) {
                return Err(ApiError::new(409, "Recipient changed. Refresh the recipient list before sending."));
            }
            r.read("sendtextmessage", params(&[("targetmode", "1"), ("target", &clid), ("msg", message)])).await
        };
        crate::audit::run_remote_audited(&r.state.db, &r.user, remote, dispatch, |_| None).await?;
    } else {
        let cluid = body["cluid"].as_str().filter(|uid| is_uid(uid));
        let subject = body["subject"].as_str().filter(|text| !text.trim().is_empty() && text.chars().count() <= 255);
        let (Some(cluid), Some(subject)) = (cluid, subject) else {
            return Err(ApiError::new(400, "A valid recipient UID and subject are required"));
        };
        let dispatch = r.read("messageadd", params(&[("cluid", cluid), ("subject", subject), ("message", message)]));
        crate::audit::run_remote_audited(&r.state.db, &r.user, remote, dispatch, |_| None).await?;
    }
    Ok((StatusCode::CREATED, Json(json!({"accepted": true, "mode": mode}))))
}

async fn message_delete(r: VsRequest, Path(p): Ids) -> ApiResult<Json<Value>> {
    r.require_admin()?;
    Ok(Json(r.read("messagedel", params(&[("msgid", id(&p, "msgid"))])).await?))
}

pub fn messages() -> Router<AppState> {
    Router::new().route("/", get(message_list).post(message_send)).route("/sender", get(message_sender)).route("/{msgid}", get(message_get).delete(message_delete))
}

// ─── Instance ──────────────────────────────────────────────────

const ALLOWED_INSTANCE_PARAMS: [&str; 9] = [
    "serverinstance_guest_serverquery_group",
    "serverinstance_template_serveradmin_group",
    "serverinstance_template_serverdefault_group",
    "serverinstance_template_channeladmin_group",
    "serverinstance_template_channeldefault_group",
    "serverinstance_filetransfer_port",
    "serverinstance_serverquery_flood_commands",
    "serverinstance_serverquery_flood_time",
    "serverinstance_serverquery_ban_time",
];

async fn instance_read(state: &AppState, server: &ServerUser, command: &str) -> ApiResult<Json<Value>> {
    Ok(Json(state.pool.get(server.config_id)?.execute(0, command, Vec::new(), Priority::High).await?))
}

async fn instance_info(State(state): State<AppState>, server: ServerUser) -> ApiResult<Json<Value>> {
    instance_read(&state, &server, "instanceinfo").await
}

async fn instance_host(State(state): State<AppState>, server: ServerUser) -> ApiResult<Json<Value>> {
    instance_read(&state, &server, "hostinfo").await
}

async fn instance_version(State(state): State<AppState>, server: ServerUser) -> ApiResult<Json<Value>> {
    instance_read(&state, &server, "version").await
}

fn require_admin(user: &AuthUser) -> ApiResult<()> {
    if user.is_admin() {
        Ok(())
    } else {
        Err(ApiError::new(403, "Insufficient permissions"))
    }
}

async fn instance_edit(State(state): State<AppState>, server: ServerUser, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    require_admin(&server.user)?;
    let filtered: QueryParams = body_params(&body).into_iter().filter(|(key, _)| ALLOWED_INSTANCE_PARAMS.contains(&key.as_str())).collect();
    Ok(Json(state.pool.get(server.config_id)?.execute(0, "instanceedit", filtered, Priority::High).await?))
}

pub fn instance() -> Router<AppState> {
    Router::new().route("/", get(instance_info).put(instance_edit)).route("/host", get(instance_host)).route("/version", get(instance_version))
}
