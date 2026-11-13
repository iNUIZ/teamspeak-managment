//! Music bots from the panel. Admins only. Avatars, speech settings and video streaming follow
//! with their own steps; until then those paths are absent, not pretending.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde_json::{json, Value};
use std::collections::HashMap;
use teamhub_db::bots::BotRowChanges;
use teamhub_db::models::{MusicBot, Song};
use teamhub_media::youtube::YtDlpOptions;
use teamhub_voice::automation::{AutomationSettings, AutomationTarget};
use teamhub_voice::chat::commands::{canonical_command, COMMAND_HELP};
use teamhub_voice::chat::format::AutomationSource;
use teamhub_voice::chat::service::PlayedLink;
use teamhub_voice::manager::{item_json, normalize_external_host, BotManager, BotSettings, ManagerError, NewBot, VoicePortLookup};
use teamhub_voice::queue::{QueueFull, RepeatMode, MAX_QUEUE_ITEMS};
use teamhub_voice::{QueueItem, Source};

use super::ts::FlexJson;
use crate::audit::{run_remote_audited, RemoteAction};
use crate::auth::{AdminUser, AuthUser};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

impl From<ManagerError> for ApiError {
    fn from(error: ManagerError) -> Self {
        match error {
            ManagerError::NotFound => Self::new(404, "Music bot not found"),
            ManagerError::NotConnected => Self::new(409, "Bot is not connected").details("Start the bot, then try again.").reason("bot_not_connected").retryable(true),
            ManagerError::AlreadyRunning => Self::new(409, "This bot is already started").reason("bot_already_started"),
            ManagerError::Invalid(message) => Self::new(400, message),
            ManagerError::Failed(message) => Self::new(502, "The bot could not connect").details(message),
        }
    }
}

impl From<QueueFull> for ApiError {
    fn from(error: QueueFull) -> Self {
        Self::new(409, error.to_string()).details("Remove some tracks or clear the queue, then try again.").reason("queue_full")
    }
}

/// Finds a virtual server's voice port through the query connection.
pub struct PoolPorts(pub Arc<crate::pool::ConnectionPool>);

impl VoicePortLookup for PoolPorts {
    fn voice_port(&self, server: i64, sid: i64) -> teamhub_voice::chat::service::BoxFuture<'static, Result<u16, String>> {
        let client = self.0.get(server);
        Box::pin(async move {
            let client = client.map_err(|error| error.message)?;
            let info = client.execute(u32::try_from(sid).unwrap_or(1), "serverinfo", Vec::new(), teamhub_ts_query::Priority::Normal).await.map_err(|error| error.to_string())?;
            let record = if info.is_array() { &info[0] } else { &info };
            let value = &record["virtualserver_port"];
            value.as_str().and_then(|text| text.parse().ok()).or_else(|| value.as_u64().and_then(|port| u16::try_from(port).ok())).filter(|port| *port >= 1).ok_or_else(|| "Voice port could not be discovered. Set Voice Port manually.".to_owned())
        })
    }
}

fn parse_id(raw: &str) -> ApiResult<i64> {
    raw.parse::<i64>().ok().filter(|id| *id > 0).ok_or_else(|| ApiError::new(400, "Invalid bot ID"))
}

pub(crate) fn manager(state: &AppState) -> ApiResult<&Arc<BotManager>> {
    state.bots.as_ref().ok_or_else(|| ApiError::new(503, "Music bots are not available"))
}

/// A whole number from JSON, as a number or text.
fn int_field(body: &Value, name: &str) -> Option<i64> {
    match &body[name] {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

/// TeamSpeak refuses a nickname outside 3-30 characters at connect, so a bot saved with one
/// could never start.
fn valid_nickname(value: &Value) -> ApiResult<Option<String>> {
    if value.is_null() {
        return Ok(None);
    }
    let text = value.as_str().map(str::trim).unwrap_or_default();
    if !(3..=30).contains(&text.chars().count()) {
        return Err(ApiError::new(400, "Bot nickname must be 3-30 characters (TeamSpeak limit)"));
    }
    Ok(Some(text.to_owned()))
}

/// Missing leaves it alone, null or empty clears it.
fn external_host_field(body: &Value) -> ApiResult<Option<Option<String>>> {
    let Some(value) = body.get("externalHost") else { return Ok(None) };
    let invalid = || ApiError::new(400, "Enter a TeamSpeak address such as mynick, ts.example.com or 203.0.113.5:9987");
    match value {
        Value::Null => Ok(Some(None)),
        Value::String(text) => Ok(Some(normalize_external_host(text).map_err(|_| invalid())?)),
        _ => Err(invalid()),
    }
}

/// The saved form of a command channel list: a JSON array, or nothing when empty.
fn serialize_channels(ids: &[String]) -> Option<String> {
    let mut unique: Vec<&str> = Vec::new();
    for id in ids.iter().map(|id| id.trim()).filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())) {
        if !unique.contains(&id) {
            unique.push(id);
        }
    }
    (!unique.is_empty()).then(|| json!(unique).to_string())
}

fn parse_channels(raw: Option<&str>) -> Vec<String> {
    let parsed: Option<Value> = raw.and_then(|text| serde_json::from_str(text).ok());
    let list = parsed.as_ref().and_then(Value::as_array).cloned().unwrap_or_default();
    list.iter()
        .filter_map(|value| value.as_str().map(str::to_owned).or_else(|| value.as_i64().map(|id| id.to_string())))
        .map(|id| id.trim().to_owned())
        .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
        .collect()
}

fn channels_field(value: &Value) -> Option<Vec<String>> {
    match value {
        Value::Array(items) => Some(items.iter().filter_map(|item| item.as_str().map(str::to_owned).or_else(|| item.as_i64().map(|id| id.to_string()))).collect()),
        Value::String(text) => Some(text.split(|c: char| c.is_whitespace() || c == ',').filter(|part| !part.is_empty()).map(str::to_owned).collect()),
        Value::Null => None,
        other => Some(vec![other.to_string()]),
    }
}

fn item_from_song(song: Song) -> QueueItem {
    QueueItem {
        id: song.id.to_string(),
        title: song.title,
        artist: song.artist,
        duration: song.duration,
        source: Source::from_stored(&song.source),
        file_path: song.file_path,
        source_url: song.source_url,
        stream_url: None,
    }
}

/// The video part of a bot's status while no stream exists.
fn no_video() -> Value {
    json!({
        "streaming": false, "streamId": null, "source": null, "preset": null, "framerate": null, "bitrate": null,
        "startedAt": null, "viewerCount": 0, "viewers": [], "sidecar": null, "quality": null, "encoder": null,
        "noViewer": {"timeoutSec": null, "stopAt": null}, "lastStop": null, "sourceMode": null, "health": null,
    })
}

async fn record_panel(state: &AppState, id: i64, user: &AuthUser, action: String) {
    state.activity.record(id, "panel", &user.username, &action).await;
}

pub(crate) async fn audited<T>(state: &AppState, user: &AuthUser, id: i64, action: &'static str, dispatch: impl std::future::Future<Output = ApiResult<T>>) -> ApiResult<T> {
    let connection = state.db.music_bot(id).await?.map_or(0, |bot| bot.server_config_id);
    let remote = RemoteAction { action, connection_id: connection, virtual_server_id: None, target_type: "music_bot", target_id: Some(id.to_string()) };
    run_remote_audited(&state.db, user, remote, dispatch, |_| None).await
}

// ─── The bots ──────────────────────────────────────────────────

async fn server_summary(state: &AppState, server_id: i64) -> ApiResult<Value> {
    Ok(state.db.server(server_id).await?.map_or(Value::Null, |server| json!({"id": server.id, "name": server.name, "host": server.host})))
}

fn bot_summary(manager: &BotManager, bot: &MusicBot, server: Value) -> Value {
    let (status, now_playing) = manager.status_of(bot.id);
    json!({
        "id": bot.id,
        "name": bot.name,
        "serverConfigId": bot.server_config_id,
        "serverConfig": server,
        "nickname": bot.nickname,
        "externalHost": bot.external_host,
        "defaultChannel": bot.default_channel,
        "commandChannelIds": parse_channels(bot.command_channel_ids.as_deref()),
        "virtualServerId": bot.virtual_server_id,
        "voicePort": bot.voice_port,
        "volume": bot.volume,
        "autoStart": bot.auto_start,
        "automationSettings": AutomationSettings::from_stored(bot.automation_settings.as_deref()).to_json(),
        "avatarMode": bot.avatar_mode,
        "avatarMd5": bot.avatar_md5,
        "avatarError": null,
        "status": status,
        "nowPlaying": now_playing.as_ref().map(item_json),
        "createdAt": bot.created_at.to_iso(),
    })
}

async fn list(State(state): State<AppState>, _admin: AdminUser) -> ApiResult<Json<Value>> {
    let manager = manager(&state)?;
    let mut bots = Vec::new();
    for bot in state.db.music_bots().await? {
        let server = server_summary(&state, bot.server_config_id).await?;
        bots.push(bot_summary(manager, &bot, server));
    }
    Ok(Json(Value::Array(bots)))
}

/// Every bot's media, cheap enough to poll: only what is in memory.
async fn media(State(state): State<AppState>, _admin: AdminUser) -> ApiResult<Json<Value>> {
    let manager = manager(&state)?;
    let mut overview = Vec::new();
    for bot in state.db.music_bots().await? {
        let server_name = state.db.server(bot.server_config_id).await?.map(|server| server.name);
        let mut entry = manager.media_overview(bot.id).unwrap_or_else(|| json!({"botId": bot.id, "status": "stopped", "channelId": null, "channelName": null, "session": null, "music": null, "video": null, "lastMusicStop": null, "lastVideoStop": null, "automation": null}));
        entry["botName"] = json!(bot.name);
        entry["avatarMode"] = json!(bot.avatar_mode);
        entry["avatarMd5"] = json!(bot.avatar_md5);
        entry["avatarError"] = Value::Null;
        entry["serverConfigId"] = json!(bot.server_config_id);
        entry["serverName"] = json!(server_name);
        overview.push(entry);
    }
    Ok(Json(Value::Array(overview)))
}

async fn details(State(state): State<AppState>, _admin: AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let manager = manager(&state)?;
    let id = parse_id(&id)?;
    let bot = state.db.music_bot(id).await?.ok_or(ManagerError::NotFound)?;
    let (status, now_playing) = manager.status_of(id);
    let progress = manager.running(id).ok().and_then(|running| {
        let snapshot = running.snapshot();
        snapshot.now_playing.as_ref().map(|item| json!({"position": snapshot.position_seconds, "duration": item.duration.unwrap_or(0.0)}))
    });
    // The identity and Auto-DJ's working state never leave the program.
    Ok(Json(json!({
        "id": bot.id,
        "name": bot.name,
        "serverConfigId": bot.server_config_id,
        "serverConfig": server_summary(&state, bot.server_config_id).await?,
        "nickname": bot.nickname,
        "serverPassword": bot.server_password,
        "externalHost": bot.external_host,
        "defaultChannel": bot.default_channel,
        "channelPassword": bot.channel_password,
        "commandChannelIds": bot.command_channel_ids,
        "virtualServerId": bot.virtual_server_id,
        "voicePort": bot.voice_port,
        "volume": bot.volume,
        "avatarMode": bot.avatar_mode,
        "avatarFile": bot.avatar_file,
        "avatarMd5": bot.avatar_md5,
        "autoStart": bot.auto_start,
        "automationSettings": AutomationSettings::from_stored(bot.automation_settings.as_deref()).to_json(),
        "streamPreset": bot.stream_preset,
        "sidecarPort": bot.sidecar_port,
        "createdAt": bot.created_at.to_iso(),
        "updatedAt": bot.updated_at.to_iso(),
        "status": status,
        "nowPlaying": now_playing.as_ref().map(item_json),
        "playbackProgress": progress,
        "avatarError": null,
    })))
}

async fn create(State(state): State<AppState>, _admin: AdminUser, FlexJson(body): FlexJson) -> ApiResult<(StatusCode, Json<Value>)> {
    let manager = manager(&state)?;
    let name = body["name"].as_str().filter(|name| !name.is_empty());
    let server = int_field(&body, "serverConfigId").filter(|id| *id != 0);
    let (Some(name), Some(server)) = (name, server) else {
        return Err(ApiError::new(400, "name and serverConfigId are required"));
    };
    let id = manager
        .create(NewBot {
            name: name.to_owned(),
            server_config_id: server,
            nickname: valid_nickname(&body["nickname"])?,
            server_password: body["serverPassword"].as_str().map(str::to_owned),
            external_host: external_host_field(&body)?.flatten(),
            default_channel: body["defaultChannel"].as_str().map(str::to_owned),
            channel_password: body["channelPassword"].as_str().map(str::to_owned),
            command_channel_ids: channels_field(&body["commandChannelIds"]).unwrap_or_default(),
            virtual_server_id: int_field(&body, "virtualServerId"),
            voice_port: int_field(&body, "voicePort"),
            volume: int_field(&body, "volume"),
            auto_start: body["autoStart"].as_bool().unwrap_or(false),
            connect_after_create: body["connectAfterCreate"] != false,
        })
        .await?;
    Ok((StatusCode::CREATED, Json(json!({"id": id}))))
}

/// Missing leaves a text alone, null clears it.
fn optional_text(body: &Value, name: &str) -> Option<Option<String>> {
    body.get(name).map(|value| value.as_str().map(str::to_owned))
}

async fn update(State(state): State<AppState>, _admin: AdminUser, Path(id): Path<String>, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let manager = manager(&state)?;
    let id = parse_id(&id)?;
    let nickname = valid_nickname(&body["nickname"])?;
    let external_host = external_host_field(&body)?;
    let name = body["name"].as_str().map(str::to_owned);
    let mut command_channels = channels_field(&body["commandChannelIds"]).map(|ids| serialize_channels(&ids));
    // Command channels belong to the connection's query, which an external server lacks.
    if external_host.as_ref().is_some_and(Option::is_some) {
        command_channels = Some(None);
    }
    let settings = BotSettings {
        name: name.clone(),
        nickname: nickname.clone(),
        server_password: optional_text(&body, "serverPassword"),
        external_host: external_host.clone(),
        default_channel: optional_text(&body, "defaultChannel"),
        channel_password: optional_text(&body, "channelPassword"),
        voice_port: int_field(&body, "voicePort"),
        volume: int_field(&body, "volume"),
        virtual_server_id: int_field(&body, "virtualServerId"),
    };
    let changes = BotRowChanges {
        name,
        nickname,
        server_password: settings.server_password.clone(),
        external_host,
        default_channel: settings.default_channel.clone(),
        channel_password: settings.channel_password.clone(),
        command_channel_ids: command_channels,
        virtual_server_id: settings.virtual_server_id,
        voice_port: settings.voice_port,
        volume: settings.volume,
        auto_start: body["autoStart"].as_bool(),
    };
    manager.update(id, &settings, &changes).await?;
    Ok(Json(json!({"success": true})))
}

async fn remove(State(state): State<AppState>, _admin: AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let id = parse_id(&id)?;
    manager(&state)?.remove(id).await?;
    Ok(Json(json!({"success": true})))
}

async fn start(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let id = parse_id(&id)?;
    manager(&state)?.start(id).await?;
    record_panel(&state, id, &admin, "start".to_owned()).await;
    Ok(Json(json!({"success": true})))
}

async fn stop(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let id = parse_id(&id)?;
    manager(&state)?.stop(id).await?;
    record_panel(&state, id, &admin, "stop".to_owned()).await;
    Ok(Json(json!({"success": true})))
}

async fn home(State(state): State<AppState>, _admin: AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    manager(&state)?.home(parse_id(&id)?).await?;
    Ok(Json(json!({"success": true})))
}

async fn restart(State(state): State<AppState>, _admin: AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    manager(&state)?.restart(parse_id(&id)?).await?;
    Ok(Json(json!({"success": true})))
}

async fn channels(State(state): State<AppState>, _admin: AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let manager = manager(&state)?;
    let id = parse_id(&id)?;
    if !manager.contains(id) {
        return Err(ManagerError::NotFound.into());
    }
    let Ok(bot) = manager.running(id) else {
        return Ok(Json(json!({"current": 0, "channels": []})));
    };
    let channels: Vec<Value> = bot.client().channels().into_iter().map(|channel| json!({"cid": channel.cid, "name": channel.name, "parentId": channel.parent_id, "order": channel.order})).collect();
    Ok(Json(json!({"current": bot.client().current_channel(), "channels": channels})))
}

async fn join(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(id): Path<String>, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let manager = manager(&state)?;
    let id = parse_id(&id)?;
    if !manager.contains(id) {
        return Err(ManagerError::NotFound.into());
    }
    let cid = int_field(&body, "cid").filter(|cid| *cid > 0).ok_or_else(|| ApiError::new(400, "Choose a channel"))?;
    let bot = manager.running(id)?;
    bot.join_channel(cid.unsigned_abs());
    let action = body["name"].as_str().filter(|name| !name.is_empty()).map_or_else(|| format!("!here → #{cid}"), |name| format!("!here → {}", name.chars().take(60).collect::<String>()));
    record_panel(&state, id, &admin, action).await;
    Ok(Json(json!({"success": true, "current": cid})))
}

async fn automation(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(id): Path<String>, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let manager = manager(&state)?;
    let id = parse_id(&id)?;
    let settings = AutomationSettings::parse(&body).ok_or_else(|| ApiError::new(400, "Invalid automation settings"))?;
    let existing = state.db.music_bot(id).await?.ok_or(ManagerError::NotFound)?;
    match settings.source {
        Some(AutomationTarget { kind: AutomationSource::Playlist, id: playlist }) => {
            let found = state.db.playlist(playlist).await?;
            let bot_server = match found.as_ref().and_then(|found| found.music_bot_id) {
                Some(owner) => state.db.music_bot(owner).await?.map(|bot| bot.server_config_id),
                None => None,
            };
            let allowed = found.is_some_and(|found| found.server_config_id.is_none_or(|server| server == existing.server_config_id)) && bot_server.is_none_or(|server| server == existing.server_config_id);
            if !allowed {
                return Err(ApiError::new(400, "Choose a playlist shared with this bot’s server"));
            }
        }
        Some(AutomationTarget { kind: AutomationSource::Radio, id: station }) => {
            if state.db.radio_station(existing.server_config_id, station).await?.is_none() {
                return Err(ApiError::new(400, "Choose a radio station from this bot’s server"));
            }
        }
        None => {}
    }
    let stored = settings.to_json();
    let dispatch = async {
        state.db.set_bot_automation_settings(id, &stored.to_string()).await?;
        manager.configure_automation(id, settings.clone())?;
        Ok(())
    };
    audited(&state, &admin, id, "music_bot.automation_update", dispatch).await?;
    record_panel(&state, id, &admin, if settings.enabled { "Auto-DJ: on" } else { "Auto-DJ: off" }.to_owned()).await;
    Ok(Json(stored))
}

// ─── Playing ───────────────────────────────────────────────────

/// The bot, connected and able to take a track.
pub(crate) fn ready(manager: &BotManager, id: i64) -> ApiResult<Arc<teamhub_voice::VoiceBot>> {
    if !manager.contains(id) {
        return Err(ManagerError::NotFound.into());
    }
    manager.running(id).map_err(|_| ApiError::new(400, "Bot is not connected"))
}

pub(crate) fn save_history(state: &AppState, id: i64, server: i64, items: Vec<QueueItem>) {
    state.bots.as_ref().map(|manager| manager.chat().save_requests_for(server, id, &items));
}

async fn play(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(id): Path<String>, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let manager = manager(&state)?;
    let id = parse_id(&id)?;
    let song_id = int_field(&body, "songId").filter(|id| *id != 0).ok_or_else(|| ApiError::new(400, "songId is required"))?;
    if !manager.contains(id) {
        return Err(ManagerError::NotFound.into());
    }
    let bot = ready(manager, id)?;
    manager.chat().cancel_requests(id);
    let song = state.db.song(song_id).await?.ok_or_else(|| ApiError::new(404, "Song not found"))?;
    let server = song.server_config_id;
    let item = item_from_song(song);
    let queue = manager.queue(id)?;
    let dispatch = async {
        {
            let mut queue = queue.lock().expect("queue lock");
            queue.add(item.clone())?;
            let last = queue.len() - 1;
            queue.play_at(last);
        }
        bot.play_item(item.clone());
        bot.settle().await;
        Ok(())
    };
    audited(&state, &admin, id, "media.music.start", dispatch).await?;
    save_history(&state, id, server, vec![item]);
    Ok(Json(json!({"success": true})))
}

async fn play_url(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(id): Path<String>, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let manager = manager(&state)?;
    let id = parse_id(&id)?;
    let url = body["url"].as_str().filter(|url| !url.is_empty()).ok_or_else(|| ApiError::new(400, "url is required"))?;
    let enqueue = body["enqueue"].as_bool().unwrap_or(false);
    ready(manager, id)?;
    let played = manager.chat().play_link(id, url, enqueue).await.map_err(map_play_error)?;
    let action = if enqueue { format!("!add {}", url.chars().take(120).collect::<String>()) } else { format!("!play {}", url.chars().take(120).collect::<String>()) };
    record_panel(&state, id, &admin, action).await;
    Ok(Json(played_json(&played)))
}

pub(crate) fn played_json(played: &PlayedLink) -> Value {
    json!({
        "success": true,
        "queued": 1 + played.queued_in_background,
        "playlist": played.queued_in_background > 0,
        "playlistTitle": played.playlist_title,
        "queueItem": {"id": played.first.id, "title": played.first.title},
    })
}

/// Downloader text stays out of answers: it can hold addresses and session details.
pub(crate) fn map_play_error(message: String) -> ApiError {
    let lower = message.to_lowercase();
    if message.starts_with("yt-dlp") && (lower.contains("sign in to confirm") || lower.contains("not a bot") || lower.contains("cookies")) {
        return ApiError::new(422, "The media provider requires verification")
            .details("Refresh your YouTube cookies in Library → YouTube access and check outbound connectivity. Cookies do not guarantee access.")
            .reason("media_auth_required")
            .retryable(false);
    }
    if message.starts_with("yt-dlp") {
        return ApiError::new(502, "The media download failed").details("Review the media diagnostics and try another source.").reason("media_download_failed").retryable(true);
    }
    if message.starts_with("Could not") || message.starts_with("No YouTube match") {
        return ApiError::new(502, message);
    }
    ApiError::new(409, message)
}

async fn play_radio(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(id): Path<String>, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let manager = manager(&state)?;
    let id = parse_id(&id)?;
    let station_id = int_field(&body, "stationId").filter(|id| *id != 0).ok_or_else(|| ApiError::new(400, "stationId is required"))?;
    let bot = ready(manager, id)?;
    let station = state.db.radio_station_any(station_id).await?.ok_or_else(|| ApiError::new(404, "Radio station not found"))?;
    let item = QueueItem {
        id: format!("radio_{}", station.id),
        title: station.name.clone(),
        artist: Some(station.genre.clone().unwrap_or_else(|| "Radio".to_owned())),
        duration: None,
        source: Source::Radio,
        file_path: String::new(),
        source_url: None,
        stream_url: Some(station.url.clone()),
    };
    let dispatch = async {
        bot.play_item(item.clone());
        bot.settle().await;
        Ok(())
    };
    audited(&state, &admin, id, "media.music.start", dispatch).await?;
    save_history(&state, id, station.server_config_id, vec![item]);
    record_panel(&state, id, &admin, format!("!radio {station_id}")).await;
    Ok(Json(json!({"success": true})))
}

/// Do something on a connected bot; on a stopped one there is nothing to do.
async fn on_running(state: &AppState, id: &str, action: impl FnOnce(&teamhub_voice::VoiceBot)) -> ApiResult<i64> {
    let manager = manager(state)?;
    let id = parse_id(id)?;
    if !manager.contains(id) {
        return Err(ManagerError::NotFound.into());
    }
    if let Ok(bot) = manager.running(id) {
        action(&bot);
        bot.settle().await;
    }
    Ok(id)
}

async fn pause(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let id = on_running(&state, &id, |bot| bot.pause()).await?;
    record_panel(&state, id, &admin, "!pause".to_owned()).await;
    Ok(Json(json!({"success": true})))
}

async fn resume(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let id = on_running(&state, &id, |bot| bot.resume()).await?;
    record_panel(&state, id, &admin, "!pause (devam)".to_owned()).await;
    Ok(Json(json!({"success": true})))
}

async fn skip(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let id = on_running(&state, &id, |bot| bot.skip()).await?;
    record_panel(&state, id, &admin, "!skip".to_owned()).await;
    Ok(Json(json!({"success": true})))
}

async fn previous(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let id = on_running(&state, &id, |bot| bot.previous()).await?;
    record_panel(&state, id, &admin, "!prev".to_owned()).await;
    Ok(Json(json!({"success": true})))
}

async fn seek(State(state): State<AppState>, _admin: AdminUser, Path(id): Path<String>, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let seconds = match &body["seconds"] {
        Value::Number(number) => number.as_f64().unwrap_or(0.0),
        Value::String(text) => text.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    };
    on_running(&state, &id, |bot| bot.seek(seconds)).await?;
    Ok(Json(json!({"success": true})))
}

async fn stop_playback(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let manager = manager(&state)?;
    let id = parse_id(&id)?;
    if !manager.contains(id) {
        return Err(ManagerError::NotFound.into());
    }
    manager.chat().cancel_requests(id);
    let queue = manager.queue(id)?;
    let running = manager.running(id).ok();
    let dispatch = async {
        // Radio keeps what is up next; stopping music clears it.
        let live = running.as_ref().is_some_and(|bot| bot.snapshot().now_playing.is_some_and(|item| item.stream_url.is_some()));
        if !live {
            queue.lock().expect("queue lock").clear();
        }
        if let Some(bot) = &running {
            bot.stop_audio();
            bot.settle().await;
        }
        Ok(())
    };
    audited(&state, &admin, id, "media.music.stop", dispatch).await?;
    record_panel(&state, id, &admin, "!stop".to_owned()).await;
    Ok(Json(json!({"success": true})))
}

/// 0 is a real level. Only a number or a whole numeric text counts.
fn parse_volume(raw: &Value) -> ApiResult<u8> {
    let number = match raw {
        Value::Number(number) => number.as_f64(),
        Value::String(text) if !text.trim().is_empty() => text.trim().parse().ok(),
        _ => None,
    };
    let volume = number.filter(|value| value.is_finite()).ok_or_else(|| ApiError::new(400, "volume must be a number from 0 to 100"))?;
    Ok(volume.round().clamp(0.0, 100.0) as u8)
}

async fn volume(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(id): Path<String>, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let manager = manager(&state)?;
    let id = parse_id(&id)?;
    let volume = parse_volume(&body["volume"])?;
    if manager.contains(id) {
        manager.set_volume(id, volume)?;
    }
    state.db.set_bot_volume(id, i64::from(volume)).await?;
    record_panel(&state, id, &admin, format!("!vol {volume}")).await;
    Ok(Json(json!({"success": true, "volume": volume})))
}

fn repeat_text(mode: RepeatMode) -> &'static str {
    match mode {
        RepeatMode::Off => "off",
        RepeatMode::Track => "track",
        RepeatMode::Queue => "queue",
    }
}

async fn playback_state(State(state): State<AppState>, _admin: AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let manager = manager(&state)?;
    let id = parse_id(&id)?;
    if !manager.contains(id) {
        return Err(ManagerError::NotFound.into());
    }
    let (status, now_playing) = manager.status_of(id);
    let snapshot = manager.running(id).ok().map(|bot| bot.snapshot());
    let saved_volume = state.db.music_bot(id).await?.map_or(0, |bot| bot.volume);
    let queue = manager.queue(id)?;
    let queue = queue.lock().expect("queue lock");
    Ok(Json(json!({
        "status": status,
        "nowPlaying": now_playing.as_ref().map(item_json),
        "position": snapshot.as_ref().map_or(0.0, |snapshot| snapshot.position_seconds),
        "duration": now_playing.as_ref().and_then(|item| item.duration).unwrap_or(0.0),
        "volume": snapshot.as_ref().map_or(saved_volume, |snapshot| i64::from(snapshot.volume)),
        "queue": queue.get_all().into_iter().map(item_json).collect::<Vec<_>>(),
        "currentIndex": queue.index().map_or(-1, |index| index as i64),
        "shuffle": queue.shuffle(),
        "repeat": repeat_text(queue.repeat()),
        "isStreaming": now_playing.as_ref().is_some_and(|item| item.stream_url.is_some()),
        "videoStream": no_video(),
    })))
}

async fn activity(State(state): State<AppState>, _admin: AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let id = parse_id(&id)?;
    if !manager(&state)?.contains(id) {
        return Err(ManagerError::NotFound.into());
    }
    Ok(Json(Value::Array(state.activity.list(id).await?)))
}

async fn search(State(state): State<AppState>, _admin: AdminUser, Path(id): Path<String>, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let manager = manager(&state)?;
    if !manager.contains(parse_id(&id)?) {
        return Err(ManagerError::NotFound.into());
    }
    let query = body["query"].as_str().map(str::trim).unwrap_or_default();
    if query.chars().count() < 2 {
        return Err(ApiError::new(400, "Enter at least 2 characters to search"));
    }
    let (cookies, results) = manager.chat().search_music(query).await;
    let results: Vec<Value> = results
        .into_iter()
        .map(|result| {
            let source = if result.source == teamhub_voice::chat::search::SearchSource::Spotify { "Spotify" } else { "YouTube" };
            json!({"title": result.title, "artist": result.artist, "duration": result.duration, "url": result.url, "source": source})
        })
        .collect();
    Ok(Json(json!({"cookies": cookies, "results": results})))
}

async fn extensions(State(state): State<AppState>, _admin: AdminUser, Path(id): Path<String>) -> ApiResult<([(header::HeaderName, &'static str); 1], Json<Value>)> {
    let id = parse_id(&id)?;
    let bot = state.db.music_bot(id).await?.ok_or(ManagerError::NotFound)?;
    let stored = state.db.setting(&format!("bot_extensions:{id}")).await?;
    let settings: Value = stored.and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_else(|| json!({"disabledCommands": []}));
    let builtin: Vec<Value> = COMMAND_HELP
        .iter()
        .map(|line| {
            let name = line.usage.trim_start_matches('!').split(' ').next().unwrap_or_default();
            json!({"name": name, "usage": line.usage, "blurb": line.blurb, "canonical": canonical_command(name)})
        })
        .collect();
    let custom: Vec<Value> = state.db.chat_commands(bot.server_config_id).await?.into_iter().map(|command| json!({"name": command.name, "description": command.description, "enabled": command.enabled})).collect();
    let answer = json!({"disabledCommands": settings["disabledCommands"], "serverConfigId": bot.server_config_id, "builtin": builtin, "custom": custom});
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(answer)))
}

async fn set_extensions(State(state): State<AppState>, _admin: AdminUser, Path(id): Path<String>, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let id = parse_id(&id)?;
    state.db.music_bot(id).await?.ok_or(ManagerError::NotFound)?;
    let invalid = || ApiError::new(400, "Invalid bot command settings");
    let commands = body["disabledCommands"].as_array().filter(|commands| commands.len() <= 256).ok_or_else(invalid)?;
    let mut disabled: Vec<String> = Vec::new();
    for command in commands {
        let name = command.as_str().ok_or_else(invalid)?;
        let valid = (1..=32).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-');
        if !valid {
            return Err(invalid());
        }
        let canonical = canonical_command(name).to_owned();
        if !disabled.contains(&canonical) {
            disabled.push(canonical);
        }
    }
    let settings = json!({"disabledCommands": disabled});
    state.db.set_setting(&format!("bot_extensions:{id}"), &settings.to_string()).await?;
    Ok(Json(settings))
}

// ─── The queue ─────────────────────────────────────────────────

async fn queue_list(State(state): State<AppState>, _admin: AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let queue = manager(&state)?.queue(parse_id(&id)?)?;
    let queue = queue.lock().expect("queue lock");
    Ok(Json(json!({"items": queue.get_all().into_iter().map(item_json).collect::<Vec<_>>(), "shuffle": queue.shuffle(), "repeat": repeat_text(queue.repeat())})))
}

async fn queue_add(State(state): State<AppState>, _admin: AdminUser, Path(id): Path<String>, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let manager = manager(&state)?;
    let id = parse_id(&id)?;
    let queue = manager.queue(id)?;
    let song = match int_field(&body, "songId") {
        Some(song_id) => state.db.song(song_id).await?,
        None => None,
    };
    let song = song.ok_or_else(|| ApiError::new(404, "Song not found"))?;
    let server = song.server_config_id;
    let item = item_from_song(song);
    let length = {
        let mut queue = queue.lock().expect("queue lock");
        queue.add(item.clone())?;
        queue.len()
    };
    save_history(&state, id, server, vec![item]);
    Ok(Json(json!({"success": true, "queueLength": length})))
}

async fn queue_playlist(State(state): State<AppState>, _admin: AdminUser, Path(id): Path<String>, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let manager = manager(&state)?;
    let id = parse_id(&id)?;
    let queue = manager.queue(id)?;
    manager.chat().cancel_requests(id);
    let playlist_id = int_field(&body, "playlistId").ok_or_else(|| ApiError::new(404, "Playlist not found"))?;
    let playlist = state.db.playlist(playlist_id).await?.ok_or_else(|| ApiError::new(404, "Playlist not found"))?;
    let songs = state.db.playlist_songs_any(playlist_id).await?;
    let server = songs.first().map(|song| song.server_config_id).or(playlist.server_config_id).unwrap_or(0);
    let items: Vec<QueueItem> = songs.into_iter().map(item_from_song).collect();
    let clear_first = body["clearFirst"].as_bool().unwrap_or(false);
    let autoplay = body["autoplay"].as_bool().unwrap_or(clear_first);

    // The whole playlist fits or none of it is queued; a refused load never clears first.
    let start_item = {
        let mut queue = queue.lock().expect("queue lock");
        if clear_first {
            if items.len() > MAX_QUEUE_ITEMS {
                return Err(QueueFull { requested: items.len(), room: MAX_QUEUE_ITEMS }.into());
            }
            queue.clear();
        }
        let play_index = queue.len();
        queue.add_many(items.clone())?;
        (autoplay && !items.is_empty()).then(|| queue.play_at(play_index).cloned()).flatten()
    };
    let mut play_error = None;
    if let Some(item) = start_item {
        match ready(manager, id) {
            Ok(bot) => {
                bot.play_item(item);
                bot.settle().await;
            }
            Err(error) => play_error = Some(error.message),
        }
    }
    save_history(&state, id, server, items);
    let length = queue.lock().expect("queue lock").len();
    Ok(Json(json!({"success": true, "queueLength": length, "playError": play_error})))
}

/// An edit by position may name the item the panel saw there. Pruning shifts positions, so a
/// stale position is refused rather than changing another track.
fn check_entry(all: &[&QueueItem], index: usize, expected: Option<&str>) -> ApiResult<()> {
    if let Some(expected) = expected {
        if all.get(index).map(|item| item.id.as_str()) != Some(expected) {
            return Err(ApiError::new(409, "The queue changed").details("Check the queue, then try again.").reason("queue_changed"));
        }
    }
    Ok(())
}

async fn queue_remove(State(state): State<AppState>, _admin: AdminUser, Path((id, index)): Path<(String, String)>, Query(q): Query<HashMap<String, String>>) -> ApiResult<Json<Value>> {
    let queue = manager(&state)?.queue(parse_id(&id)?)?;
    let index: usize = index.parse().map_err(|_| ApiError::new(400, "Invalid queue index"))?;
    let mut queue = queue.lock().expect("queue lock");
    check_entry(&queue.get_all(), index, q.get("id").map(String::as_str))?;
    queue.remove_at(index);
    Ok(Json(json!({"success": true})))
}

async fn queue_clear(State(state): State<AppState>, _admin: AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let manager = manager(&state)?;
    let id = parse_id(&id)?;
    let queue = manager.queue(id)?;
    manager.chat().cancel_requests(id);
    queue.lock().expect("queue lock").clear();
    if let Ok(bot) = manager.running(id) {
        bot.stop_audio();
        bot.settle().await;
    }
    Ok(Json(json!({"success": true})))
}

async fn queue_shuffle(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(id): Path<String>, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let id = parse_id(&id)?;
    let queue = manager(&state)?.queue(id)?;
    let enabled = body["enabled"].as_bool().unwrap_or(true);
    queue.lock().expect("queue lock").set_shuffle(enabled);
    record_panel(&state, id, &admin, format!("!shuffle {}", if enabled { "on" } else { "off" })).await;
    Ok(Json(json!({"success": true, "shuffle": enabled})))
}

async fn queue_repeat(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(id): Path<String>, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let id = parse_id(&id)?;
    let queue = manager(&state)?.queue(id)?;
    let text = body["mode"].as_str().unwrap_or("off");
    let mode = match text {
        "off" => RepeatMode::Off,
        "track" => RepeatMode::Track,
        "queue" => RepeatMode::Queue,
        _ => return Err(ApiError::new(400, "Invalid repeat mode")),
    };
    queue.lock().expect("queue lock").set_repeat(mode);
    record_panel(&state, id, &admin, format!("!repeat {text}")).await;
    Ok(Json(json!({"success": true, "repeat": text})))
}

async fn queue_play(State(state): State<AppState>, AdminUser(admin): AdminUser, Path((id, index)): Path<(String, String)>, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let manager = manager(&state)?;
    let id = parse_id(&id)?;
    let queue = manager.queue(id)?;
    let index: usize = index.parse().map_err(|_| ApiError::new(400, "Invalid queue index"))?;
    let item = {
        let mut queue = queue.lock().expect("queue lock");
        check_entry(&queue.get_all(), index, body["id"].as_str())?;
        queue.play_at(index).cloned()
    };
    let item = item.ok_or_else(|| ApiError::new(400, "Invalid queue index"))?;
    let bot = ready(manager, id)?;
    let answer = json!({"success": true, "nowPlaying": {"title": item.title, "artist": item.artist}});
    let dispatch = async {
        bot.play_item(item);
        bot.settle().await;
        Ok(())
    };
    audited(&state, &admin, id, "media.music.start", dispatch).await?;
    Ok(Json(answer))
}

async fn queue_move(State(state): State<AppState>, _admin: AdminUser, Path(id): Path<String>, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let queue = manager(&state)?.queue(parse_id(&id)?)?;
    let (Some(from), Some(to)) = (body["from"].as_u64(), body["to"].as_u64()) else {
        return Err(ApiError::new(400, "from and to are required"));
    };
    let (from, to) = (from as usize, to as usize);
    let mut queue = queue.lock().expect("queue lock");
    check_entry(&queue.get_all(), from, body["id"].as_str())?;
    if !queue.move_entry(from, to) {
        return Err(ApiError::new(400, "Invalid indices"));
    }
    Ok(Json(json!({"success": true})))
}

/// Video has its own step. Until then the status is honest and starting says why not.
async fn stream_status(State(state): State<AppState>, _admin: AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    if !manager(&state)?.contains(parse_id(&id)?) {
        return Err(ManagerError::NotFound.into());
    }
    Ok(Json(no_video()))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/media", get(media))
        .route("/{id}", get(details).put(update).delete(remove))
        .route("/{id}/automation", put(automation))
        .route("/{id}/extensions", get(extensions).put(set_extensions))
        .route("/{id}/start", post(start))
        .route("/{id}/stop", post(stop))
        .route("/{id}/home", post(home))
        .route("/{id}/restart", post(restart))
        .route("/{id}/channels", get(channels))
        .route("/{id}/join", post(join))
        .route("/{id}/play", post(play))
        .route("/{id}/play-url", post(play_url))
        .route("/{id}/play-radio", post(play_radio))
        .route("/{id}/pause", post(pause))
        .route("/{id}/resume", post(resume))
        .route("/{id}/stop-playback", post(stop_playback))
        .route("/{id}/skip", post(skip))
        .route("/{id}/previous", post(previous))
        .route("/{id}/seek", post(seek))
        .route("/{id}/volume", post(volume))
        .route("/{id}/state", get(playback_state))
        .route("/{id}/activity", get(activity))
        .route("/{id}/search", post(search))
        .route("/{id}/queue", get(queue_list).post(queue_add).delete(queue_clear))
        .route("/{id}/queue/playlist", post(queue_playlist))
        .route("/{id}/queue/shuffle", post(queue_shuffle))
        .route("/{id}/queue/repeat", post(queue_repeat))
        .route("/{id}/queue/move", put(queue_move))
        .route("/{id}/queue/{index}", delete(queue_remove))
        .route("/{id}/queue/{index}/play", post(queue_play))
        .route("/{id}/stream/status", get(stream_status))
}

/// The real music sources for the running program.
pub fn yt_dlp_options() -> YtDlpOptions {
    // An admin's saved upload first, then the optional deployment file.
    let saved = std::path::Path::new("data").join("yt-cookies.txt");
    let cookie_file = if saved.exists() {
        Some(saved)
    } else {
        std::env::var("YT_COOKIE_FILE").ok().map(std::path::PathBuf::from).filter(|path| path.exists())
    };
    YtDlpOptions { cookie_file }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_is_read_carefully() {
        assert_eq!(parse_volume(&json!(0)).unwrap(), 0, "0 is mute, not missing");
        assert_eq!(parse_volume(&json!("30")).unwrap(), 30);
        assert_eq!(parse_volume(&json!(150)).unwrap(), 100);
        for bad in [json!("20abc"), json!(""), json!(null), json!(true)] {
            assert!(parse_volume(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn nicknames_fit_teamspeak() {
        assert_eq!(valid_nickname(&json!(null)).unwrap(), None);
        assert_eq!(valid_nickname(&json!("  Bot ")).unwrap().as_deref(), Some("Bot"));
        assert!(valid_nickname(&json!("ab")).is_err());
        assert!(valid_nickname(&json!("x".repeat(31))).is_err());
    }

    #[test]
    fn command_channels_save_like_before() {
        assert_eq!(serialize_channels(&["1".to_owned(), "1".to_owned(), " 2 ".to_owned(), "x".to_owned()]).as_deref(), Some(r#"["1","2"]"#));
        assert_eq!(serialize_channels(&[]), None);
        assert_eq!(parse_channels(Some(r#"["1", 2, "x"]"#)), ["1", "2"]);
        assert_eq!(parse_channels(Some("nonsense")), Vec::<String>::new());
        assert_eq!(channels_field(&json!("1, 2 3")).unwrap(), ["1", "2", "3"]);
    }

    #[test]
    fn downloader_text_stays_out_of_answers() {
        let error = map_play_error("yt-dlp download failed (code 1): /secret/path".to_owned());
        assert_eq!(error.reason, Some("media_download_failed"));
        assert!(!format!("{error:?}").contains("secret"));
        assert_eq!(map_play_error("yt-dlp info failed: Sign in to confirm you're not a bot".to_owned()).status, 422);
    }

    #[test]
    fn stale_positions_are_refused() {
        let item = QueueItem { id: "a".to_owned(), title: String::new(), artist: None, duration: None, source: Source::Url, file_path: String::new(), source_url: None, stream_url: None };
        assert!(check_entry(&[&item], 0, Some("a")).is_ok());
        assert_eq!(check_entry(&[&item], 0, Some("b")).unwrap_err().reason, Some("queue_changed"));
        assert!(check_entry(&[&item], 5, None).is_ok());
    }
}
