//! All music bots: keeps each bot's settings and queue across connections, starts and stops
//! them, and reconnects after an unexpected disconnect. What runs is a [`VoiceBot`]; what stays is
//! a [`Slot`], so a bot exists here whether connected or not.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use serde_json::{json, Value};
use teamhub_core::FieldCipher;
use teamhub_db::models::MusicBot;
use teamhub_db::{Db, Timestamp};
use teamhub_media::cache::MediaCache;
use teamhub_media::youtube::YtDlpOptions;
use teamhub_ts_proto::Identity;
use tokio::sync::{broadcast, watch};

use crate::automation::{AutomationController, AutomationSettings, AutomationTarget};
use crate::chat::format::AutomationSource;
use crate::bot::{BotConfig, BotEvent, Media, QueueItem, Snapshot, Status, VoiceBot};
use crate::chat::service::{BotEntry, BoxFuture, ChatService};
use crate::queue::PlayQueue;

const MAX_RECONNECT_ATTEMPTS: u32 = 10;
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(30);
const RECONNECT_GRACE: Duration = Duration::from_secs(5);
const PROGRESS_EVERY: Duration = Duration::from_secs(1);
const DEFAULT_VOICE_PORT: u16 = 9987;
const DEFAULT_BOT_LIMIT: i64 = 10;
/// What a new bot starts with: quick to make, and what servers require by default. The stronger
/// identity is made in the background, since it can take half a minute.
const PLACEHOLDER_LEVEL: u32 = 8;
const FINAL_LEVEL: u32 = 23;
const NICKNAME_LOOKUP_URL: &str = "https://named.myteamspeak.com/lookup?name=";
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(10);

pub type Result<T> = std::result::Result<T, ManagerError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagerError {
    NotFound,
    NotConnected,
    AlreadyRunning,
    Invalid(String),
    Failed(String),
}

impl std::fmt::Display for ManagerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => f.write_str("Music bot not found"),
            Self::NotConnected => f.write_str("Bot is not connected"),
            Self::AlreadyRunning => f.write_str("Bot is already running"),
            Self::Invalid(message) | Self::Failed(message) => f.write_str(message),
        }
    }
}

impl From<teamhub_db::Error> for ManagerError {
    fn from(error: teamhub_db::Error) -> Self {
        Self::Failed(error.to_string())
    }
}

/// Reading a virtual server's voice port, through the query connection.
pub trait VoicePortLookup: Send + Sync {
    fn voice_port(&self, server_config_id: i64, virtual_server_id: i64) -> BoxFuture<'static, std::result::Result<u16, String>>;
}

// ─── External servers ──────────────────────────────────────────

/// Where a saved external address points, before any lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalAddress {
    pub host: String,
    pub port: Option<u16>,
}

fn parse_port(text: &str) -> std::result::Result<u16, String> {
    let port = (text.len() <= 5 && text.bytes().all(|b| b.is_ascii_digit())).then(|| text.parse::<u16>().ok()).flatten();
    port.filter(|port| *port >= 1).ok_or_else(|| "Invalid external server port".to_owned())
}

fn is_host_label(label: &str) -> bool {
    !label.is_empty() && label.len() <= 63 && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') && !label.starts_with('-') && !label.ends_with('-')
}

/// `host`, `host:port`, `[v6]:port` or a myTeamSpeak nickname such as `mynick`. Empty means none.
pub fn parse_external_host(input: &str) -> std::result::Result<Option<ExternalAddress>, String> {
    let text = input.trim();
    let invalid = || "Invalid external server address".to_owned();
    if text.is_empty() {
        return Ok(None);
    }
    if text.len() > 260 {
        return Err(invalid());
    }
    if let Some(rest) = text.strip_prefix('[') {
        let (host, tail) = rest.split_once(']').ok_or_else(invalid)?;
        if host.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(invalid());
        }
        let port = match tail.strip_prefix(':') {
            Some(port) => Some(parse_port(port)?),
            None if tail.is_empty() => None,
            None => return Err(invalid()),
        };
        return Ok(Some(ExternalAddress { host: host.to_owned(), port }));
    }
    if text.parse::<std::net::Ipv6Addr>().is_ok() {
        return Ok(Some(ExternalAddress { host: text.to_owned(), port: None }));
    }
    let (host, port) = match text.rsplit_once(':') {
        Some((host, port)) => (host, Some(parse_port(port)?)),
        None => (text, None),
    };
    if host.parse::<std::net::Ipv4Addr>().is_err() && !host.split('.').all(is_host_label) {
        return Err(invalid());
    }
    Ok(Some(ExternalAddress { host: host.to_ascii_lowercase(), port }))
}

/// The form to save, so `MyNick ` and `mynick` are one value.
pub fn normalize_external_host(input: &str) -> std::result::Result<Option<String>, String> {
    Ok(parse_external_host(input)?.map(|address| {
        let host = if address.host.contains(':') { format!("[{}]", address.host) } else { address.host };
        address.port.map_or_else(|| host.clone(), |port| format!("{host}:{port}"))
    }))
}

fn is_server_nickname(address: &ExternalAddress) -> bool {
    address.port.is_none() && !address.host.contains('.') && address.host.parse::<std::net::IpAddr>().is_err()
}

/// Where to connect. A dotless name is first asked of myTeamSpeak, as the TeamSpeak client does;
/// it can also be a local host, so an unknown name is tried as it is.
async fn resolve_external(stored: &str) -> std::result::Result<(String, u16), String> {
    let address = parse_external_host(stored)?.ok_or("Invalid external server address")?;
    if !is_server_nickname(&address) {
        return Ok((address.host, address.port.unwrap_or(DEFAULT_VOICE_PORT)));
    }
    let lookup = async {
        let client = reqwest::Client::builder().timeout(LOOKUP_TIMEOUT).build().ok()?;
        let url = reqwest::Url::parse(&format!("{NICKNAME_LOOKUP_URL}{}", address.host)).ok()?;
        let response = client.get(url).send().await.ok()?;
        if !response.status().is_success() {
            return None;
        }
        let text = response.text().await.ok()?;
        let first = text.split(|c: char| c.is_whitespace() || c == ',').find(|part| !part.is_empty())?.to_owned();
        parse_external_host(&first).ok().flatten()
    };
    match lookup.await.filter(|resolved| !is_server_nickname(resolved)) {
        Some(resolved) => Ok((resolved.host, resolved.port.unwrap_or(DEFAULT_VOICE_PORT))),
        None => Ok((address.host, DEFAULT_VOICE_PORT)),
    }
}

// ─── Reconnecting ──────────────────────────────────────────────

/// 1, 2, 4... seconds up to half a minute; `None` when it is time to give up.
pub fn reconnect_delay(attempt: u32) -> Option<Duration> {
    (attempt < MAX_RECONNECT_ATTEMPTS).then(|| Duration::from_secs(2u64.saturating_pow(attempt)).min(MAX_RECONNECT_DELAY))
}

/// The server said no (full, wrong password, banned...): trying again cannot help.
pub fn is_refusal_text(message: &str) -> bool {
    message.starts_with("TS3 error")
}

// ─── Identities ────────────────────────────────────────────────

/// The stored JSON, readable by the old program too.
fn identity_to_stored(identity: &Identity) -> String {
    json!({
        "privateKeyBigInt": identity.private_scalar(),
        "publicKeyString": identity.public_key_string,
        "keyOffset": identity.key_offset.to_string(),
        "uid": identity.uid,
    })
    .to_string()
}

fn identity_from_stored(text: &str) -> std::result::Result<Identity, String> {
    let data: Value = serde_json::from_str(text).map_err(|_| "Stored identity cannot be read".to_owned())?;
    let field = |key: &str| match &data[key] {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    };
    let (Some(scalar), Some(offset)) = (field("privateKeyBigInt"), field("keyOffset")) else {
        return Err("Stored identity cannot be read".to_owned());
    };
    Identity::from_stored(&scalar, &offset).map_err(|error| error.to_string())
}

async fn generate_identity(level: u32) -> std::result::Result<Identity, String> {
    tokio::task::spawn_blocking(move || Identity::generate(level))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

// ─── Slots ─────────────────────────────────────────────────────

/// A change the panel shows live.
#[derive(Debug, Clone, PartialEq)]
pub struct ManagerEvent {
    pub kind: &'static str,
    pub server_config_id: i64,
    pub payload: Value,
}

struct SlotState {
    name: String,
    nickname: String,
    server_config_id: i64,
    virtual_server_id: i64,
    external_host: Option<String>,
    voice_port: i64,
    server_password: Option<String>,
    default_channel: Option<String>,
    channel_password: Option<String>,
    volume: u8,
    identity: Option<Identity>,
    runtime: Option<Arc<VoiceBot>>,
    /// Between asking to start and being connected.
    starting: bool,
    /// Refused or given up: stays until the next start.
    failed: bool,
    last_error: Option<String>,
    manually_stopped: bool,
    reconnect_attempts: u32,
    /// Bumped by every manual start or stop, which cancels any reconnect in progress.
    reconnect_generation: u64,
    voice_target: (String, u16),
    music_started_at: Option<(u64, i64)>,
}

struct Slot {
    state: Mutex<SlotState>,
    queue: Arc<Mutex<PlayQueue<QueueItem>>>,
    automation: Arc<Mutex<AutomationController>>,
}

/// What a new bot is made from.
#[derive(Debug, Clone, Default)]
pub struct NewBot {
    pub name: String,
    pub server_config_id: i64,
    pub nickname: Option<String>,
    pub server_password: Option<String>,
    pub external_host: Option<String>,
    pub default_channel: Option<String>,
    pub channel_password: Option<String>,
    pub command_channel_ids: Vec<String>,
    pub virtual_server_id: Option<i64>,
    pub voice_port: Option<i64>,
    pub volume: Option<i64>,
    pub auto_start: bool,
    pub connect_after_create: bool,
}

/// Fields to change on a bot's connection; `None` leaves one alone.
#[derive(Debug, Clone, Default)]
pub struct BotSettings {
    pub name: Option<String>,
    pub nickname: Option<String>,
    pub server_password: Option<Option<String>>,
    pub external_host: Option<Option<String>>,
    pub default_channel: Option<Option<String>>,
    pub channel_password: Option<Option<String>>,
    pub voice_port: Option<i64>,
    pub volume: Option<i64>,
    pub virtual_server_id: Option<i64>,
}

pub struct BotManager {
    final_level: AtomicU32,
    db: Db,
    cipher: Arc<FieldCipher>,
    media: Media,
    chat: Arc<ChatService>,
    ports: RwLock<Option<Arc<dyn VoicePortLookup>>>,
    slots: RwLock<HashMap<i64, Arc<Slot>>>,
    events: broadcast::Sender<ManagerEvent>,
}

fn status_text(state: &SlotState) -> &'static str {
    if state.starting {
        return "starting";
    }
    if state.failed {
        return "error";
    }
    state.runtime.as_ref().map_or("stopped", |bot| bot.snapshot().status.as_str())
}

/// A queue item as the panel reads it.
pub fn item_json(item: &QueueItem) -> Value {
    let mut value = json!({"id": item.id, "title": item.title, "filePath": item.file_path, "source": item.source.as_str()});
    if let Some(artist) = &item.artist {
        value["artist"] = json!(artist);
    }
    if let Some(duration) = item.duration {
        value["duration"] = json!(duration);
    }
    if let Some(url) = &item.source_url {
        value["sourceUrl"] = json!(url);
    }
    if let Some(url) = &item.stream_url {
        value["streamUrl"] = json!(url);
    }
    value
}

impl BotManager {
    pub fn new(db: Db, cipher: Arc<FieldCipher>, cache: Arc<MediaCache>, yt_dlp: YtDlpOptions, chat: Arc<ChatService>) -> Arc<Self> {
        let (events, _) = broadcast::channel(1024);
        Arc::new(Self { db, cipher, media: Media { cache, yt_dlp }, chat, ports: RwLock::new(None), slots: RwLock::default(), events, final_level: AtomicU32::new(FINAL_LEVEL) })
    }

    /// The strength new bots grow to in the background. Only tests lower it.
    pub fn set_final_identity_level(&self, level: u32) {
        self.final_level.store(level, Ordering::Relaxed);
    }

    /// A new server comes with a music bot and a radio bot that goes back to its station when
    /// the queue is empty. Once per connection, and only while it has no bots.
    pub async fn seed_default_bots(self: &Arc<Self>, server: i64) -> Result<Vec<i64>> {
        let key = format!("default-bots-v1:{server}");
        if self.db.setting(&key).await?.is_some() {
            return Ok(Vec::new());
        }
        self.db.set_setting(&key, "seeded").await?;
        if self.db.music_bots().await?.iter().any(|bot| bot.server_config_id == server) {
            return Ok(Vec::new());
        }
        let staying = AutomationSettings { stay_in_channel: true, ..AutomationSettings::default() };
        let music = self.create_default_bot(server, "Müzik Botu", staying.clone()).await?;
        let station = self.db.first_radio_station(server).await?;
        let radio_settings = match station {
            Some(station) => AutomationSettings {
                enabled: true,
                source: Some(AutomationTarget { kind: AutomationSource::Radio, id: station.id }),
                quiet_period_seconds: 30,
                ..staying
            },
            None => staying,
        };
        let radio = self.create_default_bot(server, "Radyo Botu", radio_settings).await?;
        Ok(vec![music, radio])
    }

    async fn create_default_bot(self: &Arc<Self>, server: i64, name: &str, settings: AutomationSettings) -> Result<i64> {
        let id = self
            .create(NewBot {
                name: name.to_owned(),
                server_config_id: server,
                nickname: Some(name.to_owned()),
                server_password: None,
                external_host: None,
                default_channel: None,
                channel_password: None,
                command_channel_ids: Vec::new(),
                virtual_server_id: None,
                voice_port: None,
                volume: None,
                auto_start: true,
                connect_after_create: true,
            })
            .await?;
        self.db.set_bot_automation_settings(id, &settings.to_json().to_string()).await?;
        self.configure_automation(id, settings)?;
        Ok(id)
    }

    pub fn set_port_lookup(&self, lookup: Arc<dyn VoicePortLookup>) {
        *self.ports.write().expect("ports lock") = Some(lookup);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ManagerEvent> {
        self.events.subscribe()
    }

    pub fn chat(&self) -> &Arc<ChatService> {
        &self.chat
    }

    pub fn db(&self) -> &Db {
        &self.db
    }

    fn emit(&self, kind: &'static str, server_config_id: i64, payload: Value) {
        let _ = self.events.send(ManagerEvent { kind, server_config_id, payload });
    }

    fn slot(&self, id: i64) -> Result<Arc<Slot>> {
        self.slots.read().expect("slots lock").get(&id).cloned().ok_or(ManagerError::NotFound)
    }

    pub fn contains(&self, id: i64) -> bool {
        self.slots.read().expect("slots lock").contains_key(&id)
    }

    fn make_slot(&self, row: &MusicBot, identity: Option<Identity>) -> Arc<Slot> {
        let settings = AutomationSettings::from_stored(row.automation_settings.as_deref());
        Arc::new(Slot {
            state: Mutex::new(SlotState {
                name: row.name.clone(),
                nickname: row.nickname.clone(),
                server_config_id: row.server_config_id,
                virtual_server_id: row.virtual_server_id,
                external_host: row.external_host.clone(),
                voice_port: row.voice_port,
                server_password: row.server_password.clone(),
                default_channel: row.default_channel.clone(),
                channel_password: row.channel_password.clone(),
                volume: u8::try_from(row.volume.clamp(0, 100)).unwrap_or(100),
                identity,
                runtime: None,
                starting: false,
                failed: false,
                last_error: None,
                manually_stopped: false,
                reconnect_attempts: 0,
                reconnect_generation: 0,
                voice_target: (String::new(), 0),
                music_started_at: None,
            }),
            queue: Arc::default(),
            automation: Arc::new(Mutex::new(AutomationController::new(settings, row.automation_state.as_deref()))),
        })
    }

    /// Load every saved bot and start those marked to start. Nothing here blocks on a connection.
    pub async fn load_all(self: &Arc<Self>) -> Result<()> {
        for row in self.db.music_bots().await? {
            let identity = match &row.identity_data {
                Some(stored) => match self.cipher.decrypt(stored).map_err(|error| error.to_string()).and_then(|text| identity_from_stored(&text)) {
                    Ok(identity) => Some(identity),
                    Err(error) => {
                        tracing::error!(bot_id = row.id, error, "A bot's stored identity cannot be read");
                        None
                    }
                },
                None => None,
            };
            let slot = self.make_slot(&row, identity);
            let suppressed = slot.automation.lock().expect("automation lock").state().presence_suppressed;
            self.slots.write().expect("slots lock").insert(row.id, slot);
            if row.auto_start && !suppressed {
                let manager = self.clone();
                let id = row.id;
                tokio::spawn(async move {
                    if let Err(error) = manager.start(id).await {
                        tracing::warn!(bot_id = id, %error, "Auto-start failed");
                    }
                });
            }
        }
        Ok(())
    }

    /// Create a bot. It answers at once; the strong identity follows in the background.
    pub async fn create(self: &Arc<Self>, new: NewBot) -> Result<i64> {
        let limit = self.db.setting("max_music_bots").await?.and_then(|value| value.parse::<i64>().ok()).filter(|limit| *limit != 0).unwrap_or(DEFAULT_BOT_LIMIT);
        if self.db.music_bots().await?.len() as i64 >= limit {
            return Err(ManagerError::Invalid(format!("Music bot limit reached ({limit}). Adjust the limit in Settings.")));
        }
        if self.db.server(new.server_config_id).await?.is_none() {
            return Err(ManagerError::Invalid("Server config not found".to_owned()));
        }
        let identity = generate_identity(PLACEHOLDER_LEVEL).await.map_err(ManagerError::Failed)?;
        let stored = self.cipher.encrypt(&identity_to_stored(&identity)).map_err(|error| ManagerError::Failed(error.to_string()))?;
        // Command channels are for the connection's own server, not an external one.
        let channels = if new.external_host.is_some() { Vec::new() } else { new.command_channel_ids.clone() };
        let id = self
            .db
            .create_music_bot(&teamhub_db::bots::NewBotRow {
                name: new.name.clone(),
                server_config_id: new.server_config_id,
                nickname: new.nickname.clone().unwrap_or_else(|| "MediaBot".to_owned()),
                server_password: new.server_password.clone(),
                external_host: new.external_host.clone(),
                default_channel: new.default_channel.clone(),
                channel_password: new.channel_password.clone(),
                command_channel_ids: (!channels.is_empty()).then(|| json!(channels).to_string()),
                virtual_server_id: new.virtual_server_id.unwrap_or(1),
                voice_port: new.voice_port.unwrap_or(0),
                volume: new.volume.unwrap_or(50),
                auto_start: new.auto_start,
                identity_data: stored,
            })
            .await?;
        let row = self.db.music_bot(id).await?.ok_or(ManagerError::NotFound)?;
        self.slots.write().expect("slots lock").insert(id, self.make_slot(&row, Some(identity)));

        let manager = self.clone();
        tokio::spawn(async move { manager.upgrade_identity(id).await });
        if new.connect_after_create {
            let manager = self.clone();
            tokio::spawn(async move {
                if let Err(error) = manager.start(id).await {
                    tracing::warn!(bot_id = id, %error, "Initial connection failed");
                }
            });
        }
        Ok(id)
    }

    async fn upgrade_identity(&self, id: i64) {
        let identity = match generate_identity(self.final_level.load(Ordering::Relaxed)).await {
            Ok(identity) => identity,
            Err(error) => {
                tracing::warn!(bot_id = id, error, "Background identity upgrade failed");
                return;
            }
        };
        let Ok(stored) = self.cipher.encrypt(&identity_to_stored(&identity)) else { return };
        if let Err(error) = self.db.set_bot_identity(id, &stored).await {
            tracing::warn!(bot_id = id, %error, "Upgraded identity could not be saved");
            return;
        }
        // Used from the next start: a running connection keeps the keys it has.
        if let Ok(slot) = self.slot(id) {
            slot.state.lock().expect("slot lock").identity = Some(identity);
        }
    }

    /// Delete a bot and everything of it.
    pub async fn remove(self: &Arc<Self>, id: i64) -> Result<()> {
        if let Ok(slot) = self.slot(id) {
            slot.automation.lock().expect("automation lock").dispose();
            let _ = self.stop_slot(id, &slot, false).await;
        }
        self.slots.write().expect("slots lock").remove(&id);
        self.chat.unregister(id);
        self.db.delete_music_bot(id).await?;
        Ok(())
    }

    /// Change saved settings and keep the slot in step. The next start uses them.
    pub async fn update(&self, id: i64, settings: &BotSettings, row_changes: &teamhub_db::bots::BotRowChanges) -> Result<()> {
        if !self.db.update_music_bot(id, row_changes).await? {
            return Err(ManagerError::NotFound);
        }
        if let Ok(slot) = self.slot(id) {
            let mut state = slot.state.lock().expect("slot lock");
            if let Some(name) = &settings.name {
                state.name.clone_from(name);
            }
            if let Some(nickname) = &settings.nickname {
                state.nickname.clone_from(nickname);
            }
            if let Some(value) = &settings.server_password {
                state.server_password.clone_from(value);
            }
            if let Some(value) = &settings.external_host {
                state.external_host.clone_from(value);
            }
            if let Some(value) = &settings.default_channel {
                state.default_channel.clone_from(value);
            }
            if let Some(value) = &settings.channel_password {
                state.channel_password.clone_from(value);
            }
            if let Some(value) = settings.voice_port {
                state.voice_port = value;
            }
            if let Some(value) = settings.virtual_server_id {
                state.virtual_server_id = value;
            }
            if let Some(value) = settings.volume {
                state.volume = u8::try_from(value.clamp(0, 100)).unwrap_or(100);
            }
        }
        Ok(())
    }

    pub async fn start(self: &Arc<Self>, id: i64) -> Result<()> {
        let slot = self.slot(id)?;
        {
            let mut state = slot.state.lock().expect("slot lock");
            if state.starting || state.runtime.is_some() {
                return Err(ManagerError::AlreadyRunning);
            }
            state.manually_stopped = false;
            state.reconnect_attempts = 0;
            state.reconnect_generation += 1;
        }
        slot.automation.lock().expect("automation lock").resume_presence();
        self.connect(id, &slot).await
    }

    /// Resolve where to connect, connect, and watch the connection.
    async fn connect(self: &Arc<Self>, id: i64, slot: &Arc<Slot>) -> Result<()> {
        let (server, sid, external, voice_port) = {
            let mut state = slot.state.lock().expect("slot lock");
            state.starting = true;
            state.failed = false;
            state.last_error = None;
            (state.server_config_id, state.virtual_server_id, state.external_host.clone(), state.voice_port)
        };
        self.emit_status(id, server, "starting");
        match self.open_connection(id, slot, server, sid, external, voice_port).await {
            Ok(bot) => {
                {
                    let mut state = slot.state.lock().expect("slot lock");
                    state.starting = false;
                    state.reconnect_attempts = 0;
                    state.runtime = Some(bot.clone());
                }
                self.emit_status(id, server, "connected");
                self.attach(id, slot, &bot);
                Ok(())
            }
            Err(message) => {
                let refused = is_refusal_text(&message);
                {
                    let mut state = slot.state.lock().expect("slot lock");
                    state.starting = false;
                    state.last_error = Some(message.clone());
                    state.failed = refused;
                }
                if refused {
                    self.emit("music:bot:error", server, json!({"botId": id, "error": message}));
                    self.emit_status(id, server, "error");
                } else {
                    self.emit_status(id, server, "stopped");
                    self.schedule_reconnect(id, slot.clone());
                }
                Err(ManagerError::Failed(message))
            }
        }
    }

    async fn open_connection(&self, id: i64, slot: &Arc<Slot>, server: i64, sid: i64, external: Option<String>, voice_port: i64) -> std::result::Result<Arc<VoiceBot>, String> {
        let row = self.db.server(server).await.map_err(|error| error.to_string())?.ok_or("Bot configuration not found")?;
        let (host, port) = match external {
            Some(stored) => resolve_external(&stored).await?,
            None if voice_port != 0 => (row.host.clone(), u16::try_from(voice_port).map_err(|_| "Invalid voice port".to_owned())?),
            None => {
                let lookup = self.ports.read().expect("ports lock").clone();
                let lookup = lookup.ok_or("Voice port could not be discovered. Set Voice Port manually.")?;
                (row.host.clone(), lookup.voice_port(server, sid).await?)
            }
        };
        let identity = slot.state.lock().expect("slot lock").identity.clone();
        let identity = match identity {
            Some(identity) => identity,
            None => {
                // A bot whose stored identity was unreadable gets a new one rather than never starting.
                let fresh = generate_identity(PLACEHOLDER_LEVEL).await?;
                if let Ok(stored) = self.cipher.encrypt(&identity_to_stored(&fresh)) {
                    self.db.set_bot_identity(id, &stored).await.map_err(|error| error.to_string())?;
                }
                slot.state.lock().expect("slot lock").identity = Some(fresh.clone());
                fresh
            }
        };
        let kept_channel = {
            let automation = slot.automation.lock().expect("automation lock");
            automation.state().channel_id.filter(|_| automation.keep_channel()).map(|channel| channel.to_string())
        };
        let config = {
            let mut state = slot.state.lock().expect("slot lock");
            state.voice_target = (host.clone(), port);
            BotConfig {
                host,
                port,
                identity,
                nickname: state.nickname.clone(),
                server_password: state.server_password.clone(),
                default_channel: kept_channel.or_else(|| state.default_channel.clone()),
                channel_password: state.channel_password.clone(),
                volume: state.volume,
                media: self.media.clone(),
                queue: slot.queue.clone(),
            }
        };
        VoiceBot::start(config).await.map(Arc::new).map_err(|error| error.to_string())
    }

    /// Answer chat, run Auto-DJ, and follow this connection until it ends.
    fn attach(self: &Arc<Self>, id: i64, slot: &Arc<Slot>, bot: &Arc<VoiceBot>) {
        let entry = {
            let state = slot.state.lock().expect("slot lock");
            BotEntry {
                bot: bot.clone(),
                server_config_id: state.server_config_id,
                virtual_server_id: state.virtual_server_id,
                external: state.external_host.is_some(),
                name: state.name.clone(),
                nickname: state.nickname.clone(),
                automation: Some(slot.automation.clone()),
            }
        };
        self.chat.attach(id, entry.clone(), bot.subscribe_events());
        self.chat.run_automation(id, entry, bot.subscribe_events());

        let manager = self.clone();
        let slot = slot.clone();
        let mut states = bot.subscribe();
        let mut events = bot.subscribe_events();
        tokio::spawn(async move {
            manager.follow(id, &slot, &mut states, &mut events).await;
        });
    }

    /// Tell the panel about changes, and reconnect when the connection ends by itself.
    async fn follow(self: &Arc<Self>, id: i64, slot: &Arc<Slot>, states: &mut watch::Receiver<Snapshot>, events: &mut broadcast::Receiver<BotEvent>) {
        let server = slot.state.lock().expect("slot lock").server_config_id;
        let mut last = states.borrow().clone();
        let mut tick = tokio::time::interval(PROGRESS_EVERY);
        loop {
            tokio::select! {
                changed = states.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    let now = states.borrow_and_update().clone();
                    self.announce_changes(id, server, &last, &now);
                    last = now;
                }
                event = events.recv() => {
                    if matches!(event, Ok(BotEvent::Disconnected) | Err(broadcast::error::RecvError::Closed)) {
                        break;
                    }
                }
                _ = tick.tick() => {
                    if last.status == Status::Playing {
                        let now = states.borrow().clone();
                        let duration = now.now_playing.as_ref().and_then(|item| item.duration).unwrap_or(0.0);
                        self.emit("music:bot:progress", server, json!({"botId": id, "position": now.position_seconds, "duration": duration}));
                    }
                }
            }
        }
        // The connection is over.
        let stopped_on_purpose = {
            let mut state = slot.state.lock().expect("slot lock");
            state.runtime = None;
            state.music_started_at = None;
            state.manually_stopped
        };
        self.chat.unregister(id);
        self.emit_status(id, server, "stopped");
        if !stopped_on_purpose && self.contains(id) {
            tracing::info!(bot_id = id, "Unexpected disconnect, scheduling a reconnect");
            self.schedule_reconnect(id, slot.clone());
        }
    }

    fn announce_changes(&self, id: i64, server: i64, before: &Snapshot, now: &Snapshot) {
        if before.status != now.status {
            self.emit_status(id, server, now.status.as_str());
        }
        if before.now_playing != now.now_playing {
            match &now.now_playing {
                Some(item) => {
                    self.emit(
                        "music:bot:nowPlaying",
                        server,
                        json!({"botId": id, "song": {"id": item.id, "title": item.title, "artist": item.artist, "duration": item.duration, "source": item.source.as_str()}, "progress": null}),
                    );
                }
                None => {
                    let song = before.now_playing.as_ref().map(|item| item.id.clone());
                    self.emit("music:bot:trackEnd", server, json!({"botId": id, "songId": song}));
                }
            }
        }
        if before.volume != now.volume {
            self.emit("music:bot:volumeChange", server, json!({"botId": id, "volume": now.volume}));
        }
    }

    fn emit_status(&self, id: i64, server: i64, status: &str) {
        self.emit("music:bot:status", server, json!({"botId": id, "status": status}));
    }

    fn schedule_reconnect(self: &Arc<Self>, id: i64, slot: Arc<Slot>) {
        let (delay, generation, server) = {
            let mut state = slot.state.lock().expect("slot lock");
            let delay = reconnect_delay(state.reconnect_attempts);
            state.reconnect_attempts += 1;
            (delay, state.reconnect_generation, state.server_config_id)
        };
        let Some(delay) = delay else {
            tracing::error!(bot_id = id, "Reconnect attempts used up, giving up");
            self.emit("music:bot:reconnectFailed", server, json!({"botId": id}));
            return;
        };
        let manager = self.clone();
        tokio::spawn(async move {
            // The old connection needs a moment to leave, or the server sees two of this bot.
            tokio::time::sleep(delay + RECONNECT_GRACE).await;
            let current = {
                let state = slot.state.lock().expect("slot lock");
                state.reconnect_generation == generation && !state.manually_stopped && state.runtime.is_none() && !state.starting && !state.failed
            };
            if current && manager.contains(id) {
                let _ = manager.connect(id, &slot).await;
            }
        });
    }

    async fn stop_slot(&self, _id: i64, slot: &Arc<Slot>, suspend: bool) -> Result<()> {
        let bot = {
            let mut state = slot.state.lock().expect("slot lock");
            state.manually_stopped = true;
            state.reconnect_generation += 1;
            state.failed = false;
            state.runtime.clone()
        };
        if suspend {
            let saved = {
                let mut automation = slot.automation.lock().expect("automation lock");
                automation.suspend_presence();
                automation.take_stop();
                automation.take_state_to_save()
            };
            if let Some(saved) = saved {
                self.db.set_automation_state(_id, &saved).await?;
            }
        }
        if let Some(bot) = bot {
            bot.disconnect().await;
        }
        Ok(())
    }

    /// Stop a bot and keep it stopped, also across restarts of the program.
    pub async fn stop(&self, id: i64) -> Result<()> {
        let slot = self.slot(id)?;
        self.chat.cancel_requests(id);
        self.stop_slot(id, &slot, true).await
    }

    /// Disconnect and connect again, without changing what it was told to do.
    pub async fn restart(self: &Arc<Self>, id: i64) -> Result<()> {
        let slot = self.slot(id)?;
        self.stop_slot(id, &slot, false).await?;
        self.wait_until_stopped(&slot).await;
        self.start(id).await
    }

    /// Back to the configured channel, with automation suspended until the next start.
    pub async fn home(self: &Arc<Self>, id: i64) -> Result<()> {
        let slot = self.slot(id)?;
        if self.running(id).is_err() {
            return Err(ManagerError::NotConnected);
        }
        self.chat.cancel_requests(id);
        self.stop_slot(id, &slot, true).await?;
        self.wait_until_stopped(&slot).await;
        {
            let mut state = slot.state.lock().expect("slot lock");
            state.manually_stopped = false;
            state.reconnect_attempts = 0;
            state.reconnect_generation += 1;
        }
        self.connect(id, &slot).await
    }

    async fn wait_until_stopped(&self, slot: &Arc<Slot>) {
        for _ in 0..200 {
            if slot.state.lock().expect("slot lock").runtime.is_none() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    pub async fn stop_all(&self) {
        let ids: Vec<i64> = self.slots.read().expect("slots lock").keys().copied().collect();
        for id in ids {
            if let Ok(slot) = self.slot(id) {
                let _ = self.stop_slot(id, &slot, false).await;
            }
        }
    }

    /// The connected bot.
    pub fn running(&self, id: i64) -> Result<Arc<VoiceBot>> {
        let slot = self.slot(id)?;
        let state = slot.state.lock().expect("slot lock");
        state.runtime.clone().filter(|bot| bot.snapshot().status != Status::Stopped).ok_or(ManagerError::NotConnected)
    }

    pub fn queue(&self, id: i64) -> Result<Arc<Mutex<PlayQueue<QueueItem>>>> {
        Ok(self.slot(id)?.queue.clone())
    }

    pub fn configure_automation(&self, id: i64, settings: AutomationSettings) -> Result<()> {
        self.slot(id)?.automation.lock().expect("automation lock").configure(settings);
        Ok(())
    }

    pub fn set_volume(&self, id: i64, volume: u8) -> Result<()> {
        let slot = self.slot(id)?;
        let mut state = slot.state.lock().expect("slot lock");
        state.volume = volume;
        if let Some(bot) = &state.runtime {
            bot.set_volume(volume);
        }
        Ok(())
    }

    /// `status` and `nowPlaying` for every list, from memory.
    pub fn status_of(&self, id: i64) -> (String, Option<QueueItem>) {
        let Ok(slot) = self.slot(id) else { return ("stopped".to_owned(), None) };
        let state = slot.state.lock().expect("slot lock");
        (status_text(&state).to_owned(), state.runtime.as_ref().and_then(|bot| bot.snapshot().now_playing))
    }

    /// The Bot hub summary. Never asks TeamSpeak anything.
    pub fn media_overview(&self, id: i64) -> Option<Value> {
        let slot = self.slot(id).ok()?;
        let automation = slot.automation.lock().expect("automation lock").summary();
        let automation_active = slot.automation.lock().expect("automation lock").active();
        let mut state = slot.state.lock().expect("slot lock");
        let status = status_text(&state);
        let bot = state.runtime.clone();
        let snapshot = bot.as_ref().map(|bot| bot.snapshot());
        let name = state.name.clone();
        let music_active = matches!(status, "playing" | "paused");
        let token = snapshot.as_ref().map_or(0, |snapshot| snapshot.playback_token);
        let started_at = if music_active {
            match state.music_started_at {
                Some((_, started_at)) => Some(started_at),
                None => {
                    let now = Timestamp::now().millis();
                    state.music_started_at = Some((token, now));
                    Some(now)
                }
            }
        } else {
            state.music_started_at = None;
            None
        };
        let track = snapshot.as_ref().and_then(|snapshot| snapshot.now_playing.clone());
        let channel = bot.as_ref().map_or(0, |bot| bot.client().current_channel());
        let channel_name = bot.as_ref().and_then(|bot| bot.client().channels().into_iter().find(|info| info.cid == channel).map(|info| info.name));
        let session = music_active.then(|| {
            json!({
                "id": format!("00000000-0000-4000-8000-{:06x}{:06x}", id & 0xff_ffff, started_at.unwrap_or(0) & 0xff_ffff),
                "kind": "music",
                "state": "active",
                "botId": id,
                "botName": name,
                "startedAt": started_at,
                "label": track.as_ref().map(|item| item.title.clone()),
            })
        });
        let music = music_active.then(|| {
            json!({
                "title": track.as_ref().map(|item| item.title.clone()),
                "artist": track.as_ref().and_then(|item| item.artist.clone()),
                "live": track.as_ref().is_some_and(|item| item.stream_url.is_some()),
                "position": snapshot.as_ref().map(|snapshot| snapshot.position_seconds),
                "duration": track.as_ref().and_then(|item| item.duration).filter(|duration| *duration > 0.0),
            })
        });
        let automation_json = automation.map(|summary| {
            json!({
                "autoDj": summary.auto_dj.is_some(),
                "sourceKind": summary.auto_dj.map(|kind| if kind == crate::chat::format::AutomationSource::Radio { "radio" } else { "playlist" }),
                "stayInChannel": summary.stay_in_channel,
                "playing": summary.auto_dj.is_some() && automation_active,
            })
        });
        let (voice_host, voice_port) = state.voice_target.clone();
        Some(json!({
            "botId": id,
            "status": status,
            "lastError": state.last_error.clone().or_else(|| snapshot.as_ref().and_then(|snapshot| snapshot.last_error.clone())),
            "voiceHost": (!voice_host.is_empty()).then_some(voice_host),
            "voicePort": (voice_port != 0).then_some(voice_port),
            "clientId": bot.as_ref().map(|bot| bot.client().client_id()).filter(|clid| *clid > 0),
            "channelId": (channel > 0).then_some(channel),
            "channelName": channel_name,
            "session": session,
            "music": music,
            "video": null,
            "lastMusicStop": null,
            "lastVideoStop": null,
            "automation": automation_json,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_read_like_the_old_program() {
        let address = |text| parse_external_host(text).unwrap().unwrap();
        assert_eq!(address("MyNick "), ExternalAddress { host: "mynick".to_owned(), port: None });
        assert_eq!(address("ts.example.com:9988"), ExternalAddress { host: "ts.example.com".to_owned(), port: Some(9988) });
        assert_eq!(address("203.0.113.5"), ExternalAddress { host: "203.0.113.5".to_owned(), port: None });
        assert_eq!(address("[2001:db8::1]:9987"), ExternalAddress { host: "2001:db8::1".to_owned(), port: Some(9987) });
        assert_eq!(address("2001:db8::1"), ExternalAddress { host: "2001:db8::1".to_owned(), port: None });
        assert_eq!(parse_external_host("  ").unwrap(), None);
        for bad in ["a b", "host:0", "host:99999", "host:abc", "-bad", "[nope]", "http://x", &"a".repeat(261)] {
            assert!(parse_external_host(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn addresses_save_in_one_form() {
        assert_eq!(normalize_external_host(" MyNick").unwrap().as_deref(), Some("mynick"));
        assert_eq!(normalize_external_host("TS.Example:9988").unwrap().as_deref(), Some("ts.example:9988"));
        assert_eq!(normalize_external_host("2001:db8::1").unwrap().as_deref(), Some("[2001:db8::1]"));
        assert_eq!(normalize_external_host("").unwrap(), None);
    }

    #[test]
    fn only_a_dotless_name_without_port_is_a_nickname() {
        let is = |text| is_server_nickname(&parse_external_host(text).unwrap().unwrap());
        assert!(is("mynick"));
        assert!(!is("mynick:9987"));
        assert!(!is("ts.example.com"));
        assert!(!is("1.2.3.4"));
    }

    #[tokio::test]
    async fn a_plain_address_needs_no_lookup() {
        assert_eq!(resolve_external("ts.example.com").await.unwrap(), ("ts.example.com".to_owned(), 9987));
        assert_eq!(resolve_external("1.2.3.4:9988").await.unwrap(), ("1.2.3.4".to_owned(), 9988));
    }

    #[test]
    fn reconnects_back_off_then_stop() {
        let delays: Vec<u64> = (0..10).map(|attempt| reconnect_delay(attempt).unwrap().as_secs()).collect();
        assert_eq!(delays, [1, 2, 4, 8, 16, 30, 30, 30, 30, 30]);
        assert_eq!(reconnect_delay(10), None);
    }

    #[test]
    fn a_refusal_is_not_retried() {
        assert!(is_refusal_text("TS3 error 1028: invalid server password"));
        assert!(!is_refusal_text("Connection timeout"));
    }

    #[test]
    fn stored_identities_survive_a_round_trip() {
        let identity = Identity::generate(4).unwrap();
        let back = identity_from_stored(&identity_to_stored(&identity)).unwrap();
        assert_eq!((back.uid, back.key_offset), (identity.uid.clone(), identity.key_offset));
        // The old program's form: extra fields and numbers as text.
        let old = format!(r#"{{"privateKey":{{}},"publicKey":{{}},"privateKeyBigInt":"{}","publicKeyString":"x","keyOffset":"{}","uid":"y"}}"#, identity.private_scalar(), identity.key_offset);
        assert_eq!(identity_from_stored(&old).unwrap().uid, identity.uid);
        assert!(identity_from_stored("nonsense").is_err());
        assert!(identity_from_stored("{}").is_err());
    }

    #[test]
    fn queue_items_read_like_before() {
        let item = QueueItem {
            id: "1".to_owned(),
            title: "T".to_owned(),
            artist: None,
            duration: Some(10.0),
            source: crate::bot::Source::Youtube,
            file_path: "/f".to_owned(),
            source_url: Some("u".to_owned()),
            stream_url: None,
        };
        assert_eq!(item_json(&item), json!({"id": "1", "title": "T", "duration": 10.0, "filePath": "/f", "source": "youtube", "sourceUrl": "u"}));
    }
}
