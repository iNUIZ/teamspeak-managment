//! One struct per table, generated from `schema.prisma`. Column names are kept exactly.
//! They derive neither `Debug` nor `Serialize`: rows hold password hashes and encrypted
//! secrets, so API responses and logs must be built field by field on purpose.

#![allow(missing_docs)]

use crate::Timestamp;

#[derive(Clone, sqlx::FromRow)]
pub struct User {
    #[sqlx(rename = "id")]
    pub id: i64,
    #[sqlx(rename = "username")]
    pub username: String,
    #[sqlx(rename = "passwordHash")]
    pub password_hash: String,
    #[sqlx(rename = "displayName")]
    pub display_name: String,
    #[sqlx(rename = "role")]
    pub role: String,
    #[sqlx(rename = "enabled")]
    pub enabled: bool,
    #[sqlx(rename = "createdAt")]
    pub created_at: Timestamp,
    #[sqlx(rename = "updatedAt")]
    pub updated_at: Timestamp,
    #[sqlx(rename = "lastLoginAt")]
    pub last_login_at: Option<Timestamp>,
}

#[derive(Clone, sqlx::FromRow)]
pub struct RefreshToken {
    #[sqlx(rename = "id")]
    pub id: i64,
    #[sqlx(rename = "token")]
    pub token: String,
    #[sqlx(rename = "userId")]
    pub user_id: i64,
    #[sqlx(rename = "expiresAt")]
    pub expires_at: Timestamp,
    #[sqlx(rename = "createdAt")]
    pub created_at: Timestamp,
    #[sqlx(rename = "family")]
    pub family: Option<String>,
    #[sqlx(rename = "replacedBy")]
    pub replaced_by: Option<String>,
}

#[derive(Clone, sqlx::FromRow)]
pub struct UserServerAccess {
    #[sqlx(rename = "id")]
    pub id: i64,
    #[sqlx(rename = "userId")]
    pub user_id: i64,
    #[sqlx(rename = "serverConfigId")]
    pub server_config_id: i64,
}

#[derive(Clone, sqlx::FromRow)]
pub struct TsServerConfig {
    #[sqlx(rename = "id")]
    pub id: i64,
    #[sqlx(rename = "name")]
    pub name: String,
    #[sqlx(rename = "host")]
    pub host: String,
    #[sqlx(rename = "webqueryPort")]
    pub webquery_port: i64,
    #[sqlx(rename = "apiKey")]
    pub api_key: String,
    #[sqlx(rename = "useHttps")]
    pub use_https: bool,
    #[sqlx(rename = "sshPort")]
    pub ssh_port: i64,
    #[sqlx(rename = "sshUsername")]
    pub ssh_username: Option<String>,
    #[sqlx(rename = "sshPassword")]
    pub ssh_password: Option<String>,
    #[sqlx(rename = "sshHostKeyFingerprint")]
    pub ssh_host_key_fingerprint: Option<String>,
    #[sqlx(rename = "enabled")]
    pub enabled: bool,
    #[sqlx(rename = "isDemo")]
    pub is_demo: bool,
    #[sqlx(rename = "metricsEnabled")]
    pub metrics_enabled: bool,
    #[sqlx(rename = "metricsPort")]
    pub metrics_port: i64,
    #[sqlx(rename = "metricsHost")]
    pub metrics_host: Option<String>,
    #[sqlx(rename = "createdAt")]
    pub created_at: Timestamp,
    #[sqlx(rename = "updatedAt")]
    pub updated_at: Timestamp,
}

#[derive(Clone, sqlx::FromRow)]
pub struct BotFlow {
    #[sqlx(rename = "id")]
    pub id: i64,
    #[sqlx(rename = "name")]
    pub name: String,
    #[sqlx(rename = "description")]
    pub description: Option<String>,
    #[sqlx(rename = "flowData")]
    pub flow_data: String,
    #[sqlx(rename = "serverConfigId")]
    pub server_config_id: i64,
    #[sqlx(rename = "virtualServerId")]
    pub virtual_server_id: i64,
    #[sqlx(rename = "enabled")]
    pub enabled: bool,
    #[sqlx(rename = "createdAt")]
    pub created_at: Timestamp,
    #[sqlx(rename = "updatedAt")]
    pub updated_at: Timestamp,
}

#[derive(Clone, sqlx::FromRow)]
pub struct BotVariable {
    #[sqlx(rename = "id")]
    pub id: i64,
    #[sqlx(rename = "flowId")]
    pub flow_id: i64,
    #[sqlx(rename = "name")]
    pub name: String,
    #[sqlx(rename = "value")]
    pub value: String,
    #[sqlx(rename = "scope")]
    pub scope: String,
}

#[derive(Clone, sqlx::FromRow)]
pub struct BotExecution {
    #[sqlx(rename = "id")]
    pub id: i64,
    #[sqlx(rename = "flowId")]
    pub flow_id: i64,
    #[sqlx(rename = "triggeredBy")]
    pub triggered_by: String,
    #[sqlx(rename = "triggerData")]
    pub trigger_data: Option<String>,
    #[sqlx(rename = "status")]
    pub status: String,
    #[sqlx(rename = "startedAt")]
    pub started_at: Timestamp,
    #[sqlx(rename = "endedAt")]
    pub ended_at: Option<Timestamp>,
    #[sqlx(rename = "error")]
    pub error: Option<String>,
}

#[derive(Clone, sqlx::FromRow)]
pub struct BotExecutionLog {
    #[sqlx(rename = "id")]
    pub id: i64,
    #[sqlx(rename = "executionId")]
    pub execution_id: Option<i64>,
    #[sqlx(rename = "serverConfigId")]
    pub server_config_id: i64,
    #[sqlx(rename = "flowId")]
    pub flow_id: Option<i64>,
    #[sqlx(rename = "nodeId")]
    pub node_id: Option<String>,
    #[sqlx(rename = "nodeName")]
    pub node_name: Option<String>,
    #[sqlx(rename = "level")]
    pub level: String,
    #[sqlx(rename = "message")]
    pub message: String,
    #[sqlx(rename = "data")]
    pub data: Option<String>,
    #[sqlx(rename = "timestamp")]
    pub timestamp: Timestamp,
}

#[derive(Clone, sqlx::FromRow)]
pub struct AppSetting {
    #[sqlx(rename = "key")]
    pub key: String,
    #[sqlx(rename = "value")]
    pub value: String,
    #[sqlx(rename = "updatedAt")]
    pub updated_at: Timestamp,
}

#[derive(Clone, sqlx::FromRow)]
pub struct MusicBot {
    #[sqlx(rename = "id")]
    pub id: i64,
    #[sqlx(rename = "name")]
    pub name: String,
    #[sqlx(rename = "serverConfigId")]
    pub server_config_id: i64,
    #[sqlx(rename = "nickname")]
    pub nickname: String,
    #[sqlx(rename = "serverPassword")]
    pub server_password: Option<String>,
    #[sqlx(rename = "externalHost")]
    pub external_host: Option<String>,
    #[sqlx(rename = "defaultChannel")]
    pub default_channel: Option<String>,
    #[sqlx(rename = "channelPassword")]
    pub channel_password: Option<String>,
    #[sqlx(rename = "commandChannelIds")]
    pub command_channel_ids: Option<String>,
    #[sqlx(rename = "virtualServerId")]
    pub virtual_server_id: i64,
    #[sqlx(rename = "voicePort")]
    pub voice_port: i64,
    #[sqlx(rename = "volume")]
    pub volume: i64,
    #[sqlx(rename = "identityData")]
    pub identity_data: Option<String>,
    #[sqlx(rename = "avatarMode")]
    pub avatar_mode: String,
    #[sqlx(rename = "avatarFile")]
    pub avatar_file: Option<String>,
    #[sqlx(rename = "avatarMd5")]
    pub avatar_md5: Option<String>,
    #[sqlx(rename = "autoStart")]
    pub auto_start: bool,
    #[sqlx(rename = "automationSettings")]
    pub automation_settings: Option<String>,
    #[sqlx(rename = "automationState")]
    pub automation_state: Option<String>,
    #[sqlx(rename = "streamPreset")]
    pub stream_preset: String,
    #[sqlx(rename = "sidecarPort")]
    pub sidecar_port: i64,
    #[sqlx(rename = "createdAt")]
    pub created_at: Timestamp,
    #[sqlx(rename = "updatedAt")]
    pub updated_at: Timestamp,
}

#[derive(Clone, sqlx::FromRow)]
pub struct Song {
    #[sqlx(rename = "id")]
    pub id: i64,
    #[sqlx(rename = "title")]
    pub title: String,
    #[sqlx(rename = "artist")]
    pub artist: Option<String>,
    #[sqlx(rename = "duration")]
    pub duration: Option<f64>,
    #[sqlx(rename = "filePath")]
    pub file_path: String,
    #[sqlx(rename = "source")]
    pub source: String,
    #[sqlx(rename = "sourceUrl")]
    pub source_url: Option<String>,
    #[sqlx(rename = "fileSize")]
    pub file_size: Option<i64>,
    #[sqlx(rename = "serverConfigId")]
    pub server_config_id: i64,
    #[sqlx(rename = "createdAt")]
    pub created_at: Timestamp,
}

#[derive(Clone, sqlx::FromRow)]
pub struct Playlist {
    #[sqlx(rename = "id")]
    pub id: i64,
    #[sqlx(rename = "name")]
    pub name: String,
    #[sqlx(rename = "mode")]
    pub mode: String,
    #[sqlx(rename = "musicBotId")]
    pub music_bot_id: Option<i64>,
    #[sqlx(rename = "youtubePlaylistId")]
    pub youtube_playlist_id: Option<String>,
    #[sqlx(rename = "serverConfigId")]
    pub server_config_id: Option<i64>,
    #[sqlx(rename = "createdAt")]
    pub created_at: Timestamp,
}

#[derive(Clone, sqlx::FromRow)]
pub struct PlaylistSong {
    #[sqlx(rename = "id")]
    pub id: i64,
    #[sqlx(rename = "playlistId")]
    pub playlist_id: i64,
    #[sqlx(rename = "songId")]
    pub song_id: i64,
    #[sqlx(rename = "position")]
    pub position: i64,
}

#[derive(Clone, sqlx::FromRow)]
pub struct RadioStation {
    #[sqlx(rename = "id")]
    pub id: i64,
    #[sqlx(rename = "name")]
    pub name: String,
    #[sqlx(rename = "url")]
    pub url: String,
    #[sqlx(rename = "genre")]
    pub genre: Option<String>,
    #[sqlx(rename = "imageUrl")]
    pub image_url: Option<String>,
    #[sqlx(rename = "serverConfigId")]
    pub server_config_id: i64,
    #[sqlx(rename = "createdAt")]
    pub created_at: Timestamp,
}

#[derive(Clone, sqlx::FromRow)]
pub struct IptvPlaylist {
    #[sqlx(rename = "id")]
    pub id: i64,
    #[sqlx(rename = "name")]
    pub name: String,
    #[sqlx(rename = "sourceType")]
    pub source_type: String,
    #[sqlx(rename = "url")]
    pub url: Option<String>,
    #[sqlx(rename = "sourcePath")]
    pub source_path: Option<String>,
    #[sqlx(rename = "originalFilename")]
    pub original_filename: Option<String>,
    #[sqlx(rename = "serverConfigId")]
    pub server_config_id: i64,
    #[sqlx(rename = "autoRefreshMinutes")]
    pub auto_refresh_minutes: i64,
    #[sqlx(rename = "lastRefreshedAt")]
    pub last_refreshed_at: Option<Timestamp>,
    #[sqlx(rename = "lastError")]
    pub last_error: Option<String>,
    #[sqlx(rename = "createdAt")]
    pub created_at: Timestamp,
    #[sqlx(rename = "updatedAt")]
    pub updated_at: Timestamp,
}

#[derive(Clone, sqlx::FromRow)]
pub struct VideoQueueEntry {
    #[sqlx(rename = "id")]
    pub id: String,
    #[sqlx(rename = "musicBotId")]
    pub music_bot_id: i64,
    #[sqlx(rename = "position")]
    pub position: i64,
    #[sqlx(rename = "source")]
    pub source: String,
    #[sqlx(rename = "title")]
    pub title: String,
    #[sqlx(rename = "durationSec")]
    pub duration_sec: Option<i64>,
    #[sqlx(rename = "sourceMode")]
    pub source_mode: String,
    #[sqlx(rename = "addedBy")]
    pub added_by: Option<String>,
    #[sqlx(rename = "createdAt")]
    pub created_at: Timestamp,
}

#[derive(Clone, sqlx::FromRow)]
pub struct IptvChannelPick {
    #[sqlx(rename = "serverConfigId")]
    pub server_config_id: i64,
    #[sqlx(rename = "playlistId")]
    pub playlist_id: i64,
    #[sqlx(rename = "channelKey")]
    pub channel_key: String,
    #[sqlx(rename = "name")]
    pub name: String,
    #[sqlx(rename = "favourite")]
    pub favourite: bool,
    #[sqlx(rename = "lastStreamedAt")]
    pub last_streamed_at: Option<Timestamp>,
}

#[derive(Clone, sqlx::FromRow)]
pub struct IptvChannel {
    #[sqlx(rename = "id")]
    pub id: i64,
    #[sqlx(rename = "playlistId")]
    pub playlist_id: i64,
    #[sqlx(rename = "name")]
    pub name: String,
    #[sqlx(rename = "url")]
    pub url: String,
    #[sqlx(rename = "logo")]
    pub logo: Option<String>,
    #[sqlx(rename = "groupTitle")]
    pub group_title: Option<String>,
    #[sqlx(rename = "tvgId")]
    pub tvg_id: Option<String>,
    #[sqlx(rename = "tvgCountry")]
    pub tvg_country: Option<String>,
    #[sqlx(rename = "tvgLanguage")]
    pub tvg_language: Option<String>,
    #[sqlx(rename = "position")]
    pub position: i64,
}

#[derive(Clone, sqlx::FromRow)]
pub struct ChatCommand {
    #[sqlx(rename = "id")]
    pub id: i64,
    #[sqlx(rename = "serverConfigId")]
    pub server_config_id: i64,
    #[sqlx(rename = "name")]
    pub name: String,
    #[sqlx(rename = "response")]
    pub response: String,
    #[sqlx(rename = "description")]
    pub description: Option<String>,
    #[sqlx(rename = "enabled")]
    pub enabled: bool,
    #[sqlx(rename = "createdAt")]
    pub created_at: Timestamp,
    #[sqlx(rename = "updatedAt")]
    pub updated_at: Timestamp,
}

#[derive(Clone, sqlx::FromRow)]
pub struct Widget {
    #[sqlx(rename = "id")]
    pub id: i64,
    #[sqlx(rename = "name")]
    pub name: String,
    #[sqlx(rename = "token")]
    pub token: String,
    #[sqlx(rename = "serverConfigId")]
    pub server_config_id: i64,
    #[sqlx(rename = "virtualServerId")]
    pub virtual_server_id: i64,
    #[sqlx(rename = "theme")]
    pub theme: String,
    #[sqlx(rename = "showChannelTree")]
    pub show_channel_tree: bool,
    #[sqlx(rename = "showClients")]
    pub show_clients: bool,
    #[sqlx(rename = "hideEmptyChannels")]
    pub hide_empty_channels: bool,
    #[sqlx(rename = "maxChannelDepth")]
    pub max_channel_depth: i64,
    #[sqlx(rename = "createdAt")]
    pub created_at: Timestamp,
    #[sqlx(rename = "updatedAt")]
    pub updated_at: Timestamp,
}

#[derive(Clone, sqlx::FromRow)]
pub struct MusicRequest {
    #[sqlx(rename = "id")]
    pub id: i64,
    #[sqlx(rename = "title")]
    pub title: String,
    #[sqlx(rename = "artist")]
    pub artist: Option<String>,
    #[sqlx(rename = "duration")]
    pub duration: Option<f64>,
    #[sqlx(rename = "url")]
    pub url: String,
    #[sqlx(rename = "source")]
    pub source: String,
    #[sqlx(rename = "status")]
    pub status: String,
    #[sqlx(rename = "serverConfigId")]
    pub server_config_id: i64,
    #[sqlx(rename = "musicBotId")]
    pub music_bot_id: i64,
    #[sqlx(rename = "requestedAt")]
    pub requested_at: Timestamp,
}

#[derive(Clone, sqlx::FromRow)]
pub struct StreamSession {
    #[sqlx(rename = "id")]
    pub id: i64,
    #[sqlx(rename = "musicBotId")]
    pub music_bot_id: i64,
    #[sqlx(rename = "source")]
    pub source: String,
    #[sqlx(rename = "preset")]
    pub preset: String,
    #[sqlx(rename = "startedAt")]
    pub started_at: Timestamp,
    #[sqlx(rename = "endedAt")]
    pub ended_at: Option<Timestamp>,
    #[sqlx(rename = "peakViewers")]
    pub peak_viewers: i64,
}

#[derive(Clone, sqlx::FromRow)]
pub struct AdminAuditEvent {
    #[sqlx(rename = "id")]
    pub id: String,
    #[sqlx(rename = "operationId")]
    pub operation_id: String,
    #[sqlx(rename = "actorUserId")]
    pub actor_user_id: i64,
    #[sqlx(rename = "actorUsername")]
    pub actor_username: Option<String>,
    #[sqlx(rename = "action")]
    pub action: String,
    #[sqlx(rename = "connectionId")]
    pub connection_id: Option<i64>,
    #[sqlx(rename = "virtualServerId")]
    pub virtual_server_id: Option<i64>,
    #[sqlx(rename = "targetType")]
    pub target_type: Option<String>,
    #[sqlx(rename = "targetId")]
    pub target_id: Option<String>,
    #[sqlx(rename = "outcome")]
    pub outcome: String,
    #[sqlx(rename = "resultCode")]
    pub result_code: Option<String>,
    #[sqlx(rename = "createdAt")]
    pub created_at: Timestamp,
    #[sqlx(rename = "completedAt")]
    pub completed_at: Option<Timestamp>,
}

#[derive(Clone, sqlx::FromRow)]
pub struct ActivityJournalTarget {
    #[sqlx(rename = "id")]
    pub id: i64,
    #[sqlx(rename = "serverConfigId")]
    pub server_config_id: i64,
    #[sqlx(rename = "virtualServerId")]
    pub virtual_server_id: i64,
    #[sqlx(rename = "enabled")]
    pub enabled: bool,
    #[sqlx(rename = "createdAt")]
    pub created_at: Timestamp,
    #[sqlx(rename = "updatedAt")]
    pub updated_at: Timestamp,
}

#[derive(Clone, sqlx::FromRow)]
pub struct ClientActivity {
    #[sqlx(rename = "id")]
    pub id: String,
    #[sqlx(rename = "observedAt")]
    pub observed_at: Timestamp,
    #[sqlx(rename = "serverConfigId")]
    pub server_config_id: i64,
    #[sqlx(rename = "virtualServerId")]
    pub virtual_server_id: i64,
    #[sqlx(rename = "connectionGeneration")]
    pub connection_generation: i64,
    #[sqlx(rename = "eventKind")]
    pub event_kind: String,
    #[sqlx(rename = "clientId")]
    pub client_id: i64,
    #[sqlx(rename = "nickname")]
    pub nickname: Option<String>,
    #[sqlx(rename = "uniqueId")]
    pub unique_id: Option<String>,
    #[sqlx(rename = "databaseId")]
    pub database_id: Option<i64>,
    #[sqlx(rename = "clientType")]
    pub client_type: Option<i64>,
    #[sqlx(rename = "classification")]
    pub classification: String,
    #[sqlx(rename = "identityProvenance")]
    pub identity_provenance: String,
    #[sqlx(rename = "createdAt")]
    pub created_at: Timestamp,
}

/// Every table name, in schema order.
pub const TABLES: [&str; 25] = ["User", "RefreshToken", "UserServerAccess", "TsServerConfig", "BotFlow", "BotVariable", "BotExecution", "BotExecutionLog", "AppSetting", "MusicBot", "Song", "Playlist", "PlaylistSong", "RadioStation", "IptvPlaylist", "VideoQueueEntry", "IptvChannelPick", "IptvChannel", "ChatCommand", "Widget", "MusicRequest", "StreamSession", "AdminAuditEvent", "ActivityJournalTarget", "ClientActivity"];

/// Decode every row of every table; a type that no longer matches fails here. Returns row counts.
pub async fn decode_all(pool: &sqlx::SqlitePool) -> Result<Vec<(&'static str, usize)>, sqlx::Error> {
    Ok(vec![
        ("User", sqlx::query_as::<_, User>("SELECT * FROM \"User\"").fetch_all(pool).await?.len()),
        ("RefreshToken", sqlx::query_as::<_, RefreshToken>("SELECT * FROM \"RefreshToken\"").fetch_all(pool).await?.len()),
        ("UserServerAccess", sqlx::query_as::<_, UserServerAccess>("SELECT * FROM \"UserServerAccess\"").fetch_all(pool).await?.len()),
        ("TsServerConfig", sqlx::query_as::<_, TsServerConfig>("SELECT * FROM \"TsServerConfig\"").fetch_all(pool).await?.len()),
        ("BotFlow", sqlx::query_as::<_, BotFlow>("SELECT * FROM \"BotFlow\"").fetch_all(pool).await?.len()),
        ("BotVariable", sqlx::query_as::<_, BotVariable>("SELECT * FROM \"BotVariable\"").fetch_all(pool).await?.len()),
        ("BotExecution", sqlx::query_as::<_, BotExecution>("SELECT * FROM \"BotExecution\"").fetch_all(pool).await?.len()),
        ("BotExecutionLog", sqlx::query_as::<_, BotExecutionLog>("SELECT * FROM \"BotExecutionLog\"").fetch_all(pool).await?.len()),
        ("AppSetting", sqlx::query_as::<_, AppSetting>("SELECT * FROM \"AppSetting\"").fetch_all(pool).await?.len()),
        ("MusicBot", sqlx::query_as::<_, MusicBot>("SELECT * FROM \"MusicBot\"").fetch_all(pool).await?.len()),
        ("Song", sqlx::query_as::<_, Song>("SELECT * FROM \"Song\"").fetch_all(pool).await?.len()),
        ("Playlist", sqlx::query_as::<_, Playlist>("SELECT * FROM \"Playlist\"").fetch_all(pool).await?.len()),
        ("PlaylistSong", sqlx::query_as::<_, PlaylistSong>("SELECT * FROM \"PlaylistSong\"").fetch_all(pool).await?.len()),
        ("RadioStation", sqlx::query_as::<_, RadioStation>("SELECT * FROM \"RadioStation\"").fetch_all(pool).await?.len()),
        ("IptvPlaylist", sqlx::query_as::<_, IptvPlaylist>("SELECT * FROM \"IptvPlaylist\"").fetch_all(pool).await?.len()),
        ("VideoQueueEntry", sqlx::query_as::<_, VideoQueueEntry>("SELECT * FROM \"VideoQueueEntry\"").fetch_all(pool).await?.len()),
        ("IptvChannelPick", sqlx::query_as::<_, IptvChannelPick>("SELECT * FROM \"IptvChannelPick\"").fetch_all(pool).await?.len()),
        ("IptvChannel", sqlx::query_as::<_, IptvChannel>("SELECT * FROM \"IptvChannel\"").fetch_all(pool).await?.len()),
        ("ChatCommand", sqlx::query_as::<_, ChatCommand>("SELECT * FROM \"ChatCommand\"").fetch_all(pool).await?.len()),
        ("Widget", sqlx::query_as::<_, Widget>("SELECT * FROM \"Widget\"").fetch_all(pool).await?.len()),
        ("MusicRequest", sqlx::query_as::<_, MusicRequest>("SELECT * FROM \"MusicRequest\"").fetch_all(pool).await?.len()),
        ("StreamSession", sqlx::query_as::<_, StreamSession>("SELECT * FROM \"StreamSession\"").fetch_all(pool).await?.len()),
        ("AdminAuditEvent", sqlx::query_as::<_, AdminAuditEvent>("SELECT * FROM \"AdminAuditEvent\"").fetch_all(pool).await?.len()),
        ("ActivityJournalTarget", sqlx::query_as::<_, ActivityJournalTarget>("SELECT * FROM \"ActivityJournalTarget\"").fetch_all(pool).await?.len()),
        ("ClientActivity", sqlx::query_as::<_, ClientActivity>("SELECT * FROM \"ClientActivity\"").fetch_all(pool).await?.len()),
    ])
}
