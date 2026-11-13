//! Clients on a virtual server.

use std::collections::{HashMap, VecDeque};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{Path, Query};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use teamhub_ts_query::{Error as QueryError, Priority};

use super::ts::{params, FlexJson, VsRequest};
use crate::audit::{run_remote_audited, RemoteAction};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

type Ids = Path<HashMap<String, String>>;

const AVATAR_CACHE_TTL: Duration = Duration::from_secs(5 * 60);
const AVATAR_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_AVATAR_BYTES: usize = 2 * 1024 * 1024;
const MAX_CACHED_AVATARS: usize = 256;
const NO_AVATAR: &str = "This client has no TeamSpeak profile avatar";

fn id_of(path: &HashMap<String, String>) -> &str {
    path.get("id").map_or("", String::as_str)
}

/// A JSON value as query text, or nothing when absent.
fn text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

/// The old `value || default`: empty, zero and missing all take the default.
fn truthy_or(value: &Value, default: &str) -> String {
    match value {
        Value::String(text) if !text.is_empty() => text.clone(),
        Value::Number(number) if number.as_f64() != Some(0.0) => number.to_string(),
        Value::Bool(true) => "true".to_owned(),
        _ => default.to_owned(),
    }
}

async fn list(r: VsRequest) -> ApiResult<Json<Value>> {
    let mut flags = params(&[("-uid", ""), ("-away", ""), ("-voice", ""), ("-times", ""), ("-groups", ""), ("-info", ""), ("-country", "")]);
    // Client addresses are for admins only.
    if r.user.is_admin() {
        flags.extend(params(&[("-ip", "")]));
    }
    Ok(Json(r.read_urgent("clientlist", flags).await?))
}

async fn database(r: VsRequest, Query(q): Query<HashMap<String, String>>) -> ApiResult<Json<Value>> {
    r.require_admin()?;
    let or = |key: &str, default: &str| q.get(key).filter(|v| !v.is_empty()).cloned().unwrap_or_else(|| default.to_owned());
    Ok(Json(r.read("clientdblist", params(&[("start", &or("start", "0")), ("duration", &or("duration", "100"))])).await?))
}

async fn database_entry(r: VsRequest, Path(p): Ids) -> ApiResult<Json<Value>> {
    r.require_admin()?;
    Ok(Json(r.read("clientdbinfo", params(&[("cldbid", p.get("cldbid").map_or("", String::as_str))])).await?))
}

async fn info(r: VsRequest, Path(p): Ids) -> ApiResult<Json<Value>> {
    Ok(Json(r.read("clientinfo", params(&[("clid", id_of(&p))])).await?))
}

async fn groups(r: VsRequest, Path(p): Ids) -> ApiResult<Json<Value>> {
    Ok(Json(r.read("servergroupsbyclientid", params(&[("cldbid", id_of(&p))])).await?))
}

async fn kick(r: VsRequest, Path(p): Ids, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let clid = id_of(&p);
    let mut sent = params(&[("clid", clid), ("reasonid", &truthy_or(&body["reasonid"], "5"))]);
    if let Some(reason) = text(&body["reasonmsg"]) {
        sent.push(("reasonmsg".to_owned(), reason));
    }
    Ok(Json(r.change("client.kick", "client", Some(clid.to_owned()), "clientkick", sent).await?))
}

async fn ban(r: VsRequest, Path(p): Ids, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let clid = id_of(&p);
    let mut sent = params(&[("clid", clid), ("time", &truthy_or(&body["time"], "0"))]);
    if let Some(reason) = text(&body["banreason"]) {
        sent.push(("banreason".to_owned(), reason));
    }
    Ok(Json(r.change("client.ban", "client", Some(clid.to_owned()), "banclient", sent).await?))
}

async fn move_client(r: VsRequest, Path(p): Ids, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let clid = id_of(&p);
    let mut sent = params(&[("clid", clid)]);
    for key in ["cid", "cpw"] {
        if let Some(value) = text(&body[key]) {
            sent.push((key.to_owned(), value));
        }
    }
    Ok(Json(r.change("client.move", "client", Some(clid.to_owned()), "clientmove", sent).await?))
}

async fn poke(r: VsRequest, Path(p): Ids, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let clid = id_of(&p);
    let mut sent = params(&[("clid", clid)]);
    if let Some(message) = text(&body["msg"]) {
        sent.push(("msg".to_owned(), message));
    }
    Ok(Json(r.change("client.poke", "client", Some(clid.to_owned()), "clientpoke", sent).await?))
}

async fn message(r: VsRequest, Path(p): Ids, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let clid = id_of(&p);
    let mut sent = params(&[("targetmode", "1"), ("target", clid)]);
    if let Some(message) = text(&body["msg"]) {
        sent.push(("msg".to_owned(), message));
    }
    Ok(Json(r.change("client.message", "client", Some(clid.to_owned()), "sendtextmessage", sent).await?))
}

async fn permissions(r: VsRequest, Path(p): Ids) -> ApiResult<Json<Value>> {
    match r.client.execute(r.sid, "clientpermlist", params(&[("cldbid", id_of(&p)), ("-permsid", "")]), Priority::Normal).await {
        Ok(answer) => Ok(Json(answer)),
        // A client with no permissions yet.
        Err(QueryError::Api { code: 1281, .. }) => Ok(Json(json!([]))),
        Err(error) => Err(error.into()),
    }
}

/// The number TeamSpeak wants for a permission name.
async fn permission_id(r: &VsRequest, body: &Value) -> ApiResult<String> {
    let name = text(&body["permsid"]).unwrap_or_default();
    let answer = r.read("permidgetbyname", params(&[("permsid", &name)])).await?;
    text(&answer[0]["permid"]).filter(|id| !id.is_empty() && id != "0").ok_or_else(|| ApiError::new(400, format!("Unknown permission: {name}")))
}

async fn change_permission(r: &VsRequest, cldbid: &str, action: &'static str, command: &str, sent: Vec<(String, String)>) -> ApiResult<Json<Value>> {
    let remote = RemoteAction { action, connection_id: r.config_id, virtual_server_id: Some(i64::from(r.sid)), target_type: "client", target_id: Some(cldbid.to_owned()) };
    let dispatch = async { r.client.execute_post(r.sid, command, sent, Priority::Normal).await.map_err(ApiError::from) };
    run_remote_audited(&r.state.db, &r.user, remote, dispatch, |_| None).await?;
    Ok(Json(json!({"success": true})))
}

async fn add_permission(r: VsRequest, Path(p): Ids, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    r.require_admin()?;
    let cldbid = id_of(&p);
    let permid = permission_id(&r, &body).await?;
    let number = |key: &str| text(&body[key]).unwrap_or_else(|| "0".to_owned());
    let sent = params(&[("cldbid", cldbid), ("permid", &permid), ("permvalue", &number("permvalue")), ("permnegated", &number("permnegated")), ("permskip", &number("permskip"))]);
    change_permission(&r, cldbid, "client.permission_add", "clientaddperm", sent).await
}

async fn delete_permission(r: VsRequest, Path(p): Ids, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    r.require_admin()?;
    let cldbid = id_of(&p);
    let permid = permission_id(&r, &body).await?;
    change_permission(&r, cldbid, "client.permission_delete", "clientdelperm", params(&[("cldbid", cldbid), ("permid", &permid)])).await
}

// ─── Profile pictures ──────────────────────────────────────────

/// What the bytes really are; anything but an image is refused.
fn avatar_content_type(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]) {
        return Some("image/png");
    }
    if data.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some("image/jpeg");
    }
    if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    None
}

/// The picture's address from TeamSpeak, only on the one place myTeamSpeak keeps them.
pub fn parse_avatar_url(raw: &str) -> ApiResult<Option<url::Url>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    let value = raw.split_once(',').map_or(raw, |(_, rest)| rest);
    let url = url::Url::parse(value).map_err(|_| ApiError::new(502, "TeamSpeak returned an invalid profile avatar URL"))?;
    let trusted = url.scheme() == "https"
        && url.host_str() == Some("storage.googleapis.com")
        && url.port().is_none()
        && url.path().starts_with("/ts-sys-myts-avatars/")
        && url.username().is_empty()
        && url.password().is_none();
    if !trusted {
        return Err(ApiError::new(502, "TeamSpeak returned an untrusted profile avatar URL"));
    }
    Ok(Some(url))
}

const BASE64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn base64url(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 4 / 3 + 3);
    for chunk in bytes.chunks(3) {
        let number = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        for index in 0..=chunk.len() {
            out.push(char::from(BASE64URL[((number >> (18 - index * 6)) & 63) as usize]));
        }
    }
    out
}

#[derive(Clone)]
struct Avatar {
    data: Vec<u8>,
    content_type: &'static str,
    etag: String,
    verified_at: Instant,
}

#[derive(Default)]
struct AvatarStore {
    entries: HashMap<String, Avatar>,
    order: VecDeque<String>,
    missing_until: HashMap<String, Instant>,
}

static AVATARS: LazyLock<Mutex<AvatarStore>> = LazyLock::new(Mutex::default);

async fn fetch_avatar(url: url::Url) -> ApiResult<Avatar> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(AVATAR_TIMEOUT)
        .build()
        .map_err(ApiError::unexpected)?;
    let too_large = || ApiError::new(502, "TeamSpeak profile avatar is too large");
    let failed = || ApiError::new(502, "TeamSpeak profile avatar could not be fetched");
    let mut response = client.get(url).send().await.map_err(|_| failed())?;
    if !response.status().is_success() {
        return Err(ApiError::new(502, format!("TeamSpeak profile avatar returned HTTP {}", response.status().as_u16())));
    }
    if response.content_length().is_some_and(|size| size > MAX_AVATAR_BYTES as u64) {
        return Err(too_large());
    }
    let mut data = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| failed())? {
        data.extend_from_slice(&chunk);
        if data.len() > MAX_AVATAR_BYTES {
            return Err(too_large());
        }
    }
    let content_type = avatar_content_type(&data).ok_or_else(|| ApiError::new(502, "TeamSpeak profile avatar is not a supported image"))?;
    let etag = format!("\"{}\"", base64url(&Sha256::digest(&data)));
    Ok(Avatar { data, content_type, etag, verified_at: Instant::now() })
}

fn avatar_response(avatar: &Avatar, headers: &HeaderMap) -> Response {
    let common = [
        (header::CACHE_CONTROL, "private, max-age=300, must-revalidate".to_owned()),
        (header::ETAG, avatar.etag.clone()),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
    ];
    if headers.get(header::IF_NONE_MATCH).and_then(|value| value.to_str().ok()) == Some(avatar.etag.as_str()) {
        return (StatusCode::NOT_MODIFIED, common).into_response();
    }
    (StatusCode::OK, common, [(header::CONTENT_TYPE, avatar.content_type)], avatar.data.clone()).into_response()
}

async fn avatar(r: VsRequest, Path(p): Ids, headers: HeaderMap) -> ApiResult<Response> {
    let clid = id_of(&p).parse::<u64>().ok().filter(|clid| *clid > 0).ok_or_else(|| ApiError::new(400, "A valid connected client ID is required"))?;
    let missing_key = format!("{}:{}:{clid}", r.config_id, r.sid);
    {
        let mut store = AVATARS.lock().expect("avatar lock");
        match store.missing_until.get(&missing_key) {
            Some(until) if *until > Instant::now() => return Err(ApiError::new(404, NO_AVATAR)),
            Some(_) => {
                store.missing_until.remove(&missing_key);
            }
            None => {}
        }
    }
    let answer = r.read("clientinfo", params(&[("clid", &clid.to_string())])).await?;
    let info = &answer[0];
    let Some(url) = parse_avatar_url(info["client_myteamspeak_avatar"].as_str().unwrap_or_default())? else {
        let mut store = AVATARS.lock().expect("avatar lock");
        store.missing_until.retain(|_, until| *until > Instant::now());
        store.missing_until.insert(missing_key, Instant::now() + AVATAR_CACHE_TTL);
        return Err(ApiError::new(404, NO_AVATAR));
    };
    let who = info["client_unique_identifier"].as_str().map_or_else(|| clid.to_string(), str::to_owned);
    let key = format!("{}:{}:{who}:{url}", r.config_id, r.sid);
    let cached = AVATARS.lock().expect("avatar lock").entries.get(&key).filter(|entry| entry.verified_at.elapsed() < AVATAR_CACHE_TTL).cloned();
    if let Some(entry) = cached {
        return Ok(avatar_response(&entry, &headers));
    }
    let fresh = fetch_avatar(url).await?;
    {
        let mut store = AVATARS.lock().expect("avatar lock");
        if store.entries.insert(key.clone(), fresh.clone()).is_none() {
            store.order.push_back(key);
        }
        while store.entries.len() > MAX_CACHED_AVATARS {
            let Some(oldest) = store.order.pop_front() else { break };
            store.entries.remove(&oldest);
        }
    }
    Ok(avatar_response(&fresh, &headers))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/database", get(database))
        .route("/database/{cldbid}", get(database_entry))
        .route("/{id}", get(info))
        .route("/{id}/avatar", get(avatar))
        .route("/{id}/groups", get(groups))
        .route("/{id}/kick", post(kick))
        .route("/{id}/ban", post(ban))
        .route("/{id}/move", post(move_client))
        .route("/{id}/poke", post(poke))
        .route("/{id}/message", post(message))
        .route("/{id}/permissions", get(permissions).put(add_permission).delete(delete_permission))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_real_images_pass() {
        assert_eq!(avatar_content_type(&[137, 80, 78, 71, 13, 10, 26, 10, 0]), Some("image/png"));
        assert_eq!(avatar_content_type(&[0xff, 0xd8, 0xff, 0]), Some("image/jpeg"));
        assert_eq!(avatar_content_type(b"GIF89a.."), Some("image/gif"));
        assert_eq!(avatar_content_type(b"RIFF1234WEBPxx"), Some("image/webp"));
        assert_eq!(avatar_content_type(b"<svg onload=x>"), None);
        assert_eq!(avatar_content_type(b""), None);
    }

    #[test]
    fn pictures_come_only_from_the_known_place() {
        let good = "https://storage.googleapis.com/ts-sys-myts-avatars/a.png";
        assert_eq!(parse_avatar_url(good).unwrap().unwrap().as_str(), good);
        assert_eq!(parse_avatar_url(&format!("1,{good}")).unwrap().unwrap().as_str(), good, "the old value has a prefix");
        assert_eq!(parse_avatar_url("  ").unwrap(), None);
        for bad in [
            "nonsense",
            "http://storage.googleapis.com/ts-sys-myts-avatars/a.png",
            "https://evil.example/ts-sys-myts-avatars/a.png",
            "https://storage.googleapis.com/other/a.png",
            "https://storage.googleapis.com:8443/ts-sys-myts-avatars/a.png",
            "https://u:p@storage.googleapis.com/ts-sys-myts-avatars/a.png",
        ] {
            assert!(parse_avatar_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn base64url_matches_node() {
        assert_eq!(base64url(b"f"), "Zg");
        assert_eq!(base64url(b"fo"), "Zm8");
        assert_eq!(base64url(b"foo"), "Zm9v");
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
    }

    #[test]
    fn the_old_or_defaults_are_kept() {
        assert_eq!(truthy_or(&json!(0), "5"), "5");
        assert_eq!(truthy_or(&json!(""), "5"), "5");
        assert_eq!(truthy_or(&json!(null), "5"), "5");
        assert_eq!(truthy_or(&json!(4), "5"), "4");
    }
}
