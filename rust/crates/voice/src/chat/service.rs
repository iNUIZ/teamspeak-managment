//! Answering chat commands. Each line runs in its own task, so `!stop` reaches a bot while a
//! `!play` is still downloading, and the bot's voice clock is never involved.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use chrono::Utc;
use teamhub_db::{Db, NewMusicRequest};
use teamhub_media::address::validate_url;
use teamhub_media::cache::MediaCache;
use teamhub_media::links::{self, is_apple_music_share_url, is_spotify_share_url, Collection, LinkTrack, SpotifyCatalog};
use teamhub_media::pcm::is_remote_input;
use teamhub_media::youtube::{self, is_youtube_url, Expanded, SearchResult, YtDlpOptions};
use teamhub_ts_proto::commands::Params;
use tokio::sync::{broadcast, watch};

use super::claims::{Claims, CHANNEL_COMMAND_COOLDOWN, CHAT_REPLY_COOLDOWN, HELP_COOLDOWN, HERE_ACTION_COOLDOWN, HERE_LIST_COOLDOWN};
use super::commands::{canonical_command, command_group, is_builtin, suggest_builtin, CommandGroup, COMMAND_HELP};
use super::format::{self, AutomationSource, AutomationSummary, CustomLine, HereEntry, NowPlaying, TrackLine};
use super::lyrics::{self, chunk_lyrics, lyrics_input_from_track, LyricsQuery, LyricsResult};
use super::permissions::{self, authorize, permissions_key, Permissions};
use super::search::{format_results, merge_and_rank, Candidate, RankedResult, SearchSource};
use super::targeting::{choose_command_bot, parse_bot_target, ChannelBot};
use crate::automation::{AutomationController, AutomationTarget, Snapshot as AutomationSnapshot, Step};
use crate::bot::{BotEvent, QueueItem, Snapshot, Source, Status, VoiceBot};
use crate::queue::{PlayQueue, QueueFull, RepeatMode};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
type Error = String;
type Result<T> = std::result::Result<T, Error>;

const SEARCH_SESSION: Duration = Duration::from_secs(5 * 60);
const MAX_SEARCH_QUERY_CHARS: usize = 160;
const MAX_SONG_QUERY_CHARS: usize = 200;
const PLAYLIST_CAP: usize = 25;
const QUEUE_LINES: usize = 15;
const LYRICS_CHUNK_CHARS: usize = 900;
pub const MEDIA_UNAVAILABLE: &str = "This media is no longer available.";
pub const REPLAY_STOPPED: &str = "Playback was stopped before the replay started.";

/// Newest requests kept per bot: `MUSIC_REQUEST_HISTORY_LIMIT`, at most 1000.
pub fn history_limit() -> i64 {
    static LIMIT: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *LIMIT.get_or_init(|| std::env::var("MUSIC_REQUEST_HISTORY_LIMIT").ok().and_then(|text| text.parse::<i64>().ok()).filter(|limit| *limit > 0).map_or(100, |limit| limit.min(1000)))
}

/// What commands need from a running bot.
pub trait BotControl: Send + Sync {
    fn snapshot(&self) -> Snapshot;
    fn client_id(&self) -> u64;
    fn current_channel(&self) -> u64;
    fn client_channel(&self, clid: u64) -> Option<u64>;
    /// Other non-query clients in the bot's channel, other music bots included.
    fn channel_members(&self) -> Vec<u64>;
    fn queue(&self) -> Arc<Mutex<PlayQueue<QueueItem>>>;
    fn play_item(&self, item: QueueItem);
    fn play_automation(&self, item: QueueItem);
    fn clear_automation(&self);
    fn stop_audio(&self);
    fn pause(&self);
    fn resume(&self);
    fn seek(&self, seconds: f64);
    fn set_volume(&self, volume: u8);
    fn join_channel(&self, channel: u64);
    fn send_channel(&self, message: &str);
    fn send_private(&self, clid: u64, message: &str);
    fn flood_hold_active(&self) -> bool;
    fn note_ignored_command(&self);
    fn settle(&self) -> BoxFuture<'_, ()>;
}

impl BotControl for VoiceBot {
    fn snapshot(&self) -> Snapshot {
        VoiceBot::snapshot(self)
    }
    fn client_id(&self) -> u64 {
        self.client().client_id()
    }
    fn current_channel(&self) -> u64 {
        self.client().current_channel()
    }
    fn client_channel(&self, clid: u64) -> Option<u64> {
        self.client().client_channel(clid)
    }
    fn channel_members(&self) -> Vec<u64> {
        self.client().channel_members()
    }
    fn queue(&self) -> Arc<Mutex<PlayQueue<QueueItem>>> {
        VoiceBot::queue(self)
    }
    fn play_item(&self, item: QueueItem) {
        VoiceBot::play_item(self, item);
    }
    fn play_automation(&self, item: QueueItem) {
        VoiceBot::play_automation(self, item);
    }
    fn clear_automation(&self) {
        VoiceBot::clear_automation(self);
    }
    fn stop_audio(&self) {
        VoiceBot::stop_audio(self);
    }
    fn pause(&self) {
        VoiceBot::pause(self);
    }
    fn resume(&self) {
        VoiceBot::resume(self);
    }
    fn seek(&self, seconds: f64) {
        VoiceBot::seek(self, seconds);
    }
    fn set_volume(&self, volume: u8) {
        VoiceBot::set_volume(self, volume);
    }
    fn join_channel(&self, channel: u64) {
        VoiceBot::join_channel(self, channel);
    }
    fn send_channel(&self, message: &str) {
        self.send_channel_message(message);
    }
    fn send_private(&self, clid: u64, message: &str) {
        self.send_private_message(clid, message);
    }
    fn flood_hold_active(&self) -> bool {
        VoiceBot::flood_hold_active(self)
    }
    fn note_ignored_command(&self) {
        VoiceBot::note_ignored_command(self);
    }
    fn settle(&self) -> BoxFuture<'_, ()> {
        Box::pin(VoiceBot::settle(self))
    }
}

/// Finding and fetching music. Dropping a returned future stops its processes.
pub trait Tracks: Send + Sync {
    fn has_cookies(&self) -> bool;
    fn has_spotify(&self) -> bool;
    /// The tracks behind a Spotify or Apple Music link.
    fn resolve_share_link(&self, url: String) -> BoxFuture<'static, Result<Collection>>;
    fn search(&self, query: String) -> BoxFuture<'static, Vec<RankedResult>>;
    fn find_song(&self, query: String) -> BoxFuture<'static, Result<Option<SearchResult>>>;
    fn expand(&self, url: String, cap: usize) -> BoxFuture<'static, Result<Expanded>>;
    fn download(&self, url: String) -> BoxFuture<'static, Result<QueueItem>>;
    fn lyrics(&self, input: LyricsQuery) -> BoxFuture<'static, Option<LyricsResult>>;
}

/// The real ones: yt-dlp, the download cache and the lyrics sites.
pub struct RealTracks {
    pub cache: Arc<MediaCache>,
    pub yt_dlp: YtDlpOptions,
    /// Spotify's catalogue in `!ara`, when an app is set up.
    pub spotify: Option<Arc<SpotifyCatalog>>,
}

impl Tracks for RealTracks {
    fn has_cookies(&self) -> bool {
        self.yt_dlp.cookie_file.is_some()
    }

    fn has_spotify(&self) -> bool {
        self.spotify.is_some()
    }

    fn resolve_share_link(&self, url: String) -> BoxFuture<'static, Result<Collection>> {
        Box::pin(async move {
            let found = if is_spotify_share_url(&url) { links::resolve_spotify(&url).await } else { links::resolve_apple_music(&url).await };
            found.map_err(|error| error.0)
        })
    }

    fn search(&self, query: String) -> BoxFuture<'static, Vec<RankedResult>> {
        let (options, spotify) = (self.yt_dlp.clone(), self.spotify.clone());
        Box::pin(async move {
            let query: String = query.trim().chars().take(MAX_SEARCH_QUERY_CHARS).collect();
            // A plain search, plus the official upload that a plain search often buries.
            let audio = format!("{query} official audio");
            let catalog = async {
                match &spotify {
                    Some(catalog) => catalog.search(&query, 5).await,
                    None => Ok(Vec::new()),
                }
            };
            let (plain, official, catalog) = tokio::join!(youtube::search_youtube(&query, 10, &options), youtube::search_youtube(&audio, 5, &options), catalog);
            let items = plain.unwrap_or_default().into_iter().chain(official.unwrap_or_default());
            let candidates: Vec<Candidate> = items.map(|result| Candidate { result, url: None }).collect();
            let tracks = catalog.unwrap_or_else(|error| {
                tracing::warn!(%error, "Spotify search failed");
                Vec::new()
            });
            // Spotify ids cannot collide with YouTube ids in the merged list.
            let spotify_candidates: Vec<Candidate> = tracks
                .into_iter()
                .map(|track| Candidate {
                    result: SearchResult { id: format!("spotify:{}", track.id), title: track.title, artist: track.artist, duration: track.duration, thumbnail: String::new() },
                    url: Some(track.url),
                })
                .collect();
            merge_and_rank(&query, &[(SearchSource::Youtube, candidates), (SearchSource::Spotify, spotify_candidates)])
        })
    }

    fn find_song(&self, query: String) -> BoxFuture<'static, Result<Option<SearchResult>>> {
        let options = self.yt_dlp.clone();
        Box::pin(async move { youtube::find_song(&query, &options).await.map_err(|error| error.0) })
    }

    fn expand(&self, url: String, cap: usize) -> BoxFuture<'static, Result<Expanded>> {
        let options = self.yt_dlp.clone();
        Box::pin(async move { youtube::expand_to_watch_urls(&url, cap, &options).await.map_err(|error| error.0) })
    }

    fn download(&self, url: String) -> BoxFuture<'static, Result<QueueItem>> {
        let (cache, options) = (self.cache.clone(), self.yt_dlp.clone());
        Box::pin(async move {
            let track = youtube::download(&url, &cache, &options).await.map_err(|error| error.0)?;
            Ok(QueueItem {
                id: format!("yt_{}", track.id),
                title: track.title,
                artist: Some(track.artist),
                duration: (track.duration > 0.0).then_some(track.duration),
                source: Source::Youtube,
                file_path: track.file_path.display().to_string(),
                source_url: Some(url),
                stream_url: None,
            })
        })
    }

    fn lyrics(&self, input: LyricsQuery) -> BoxFuture<'static, Option<LyricsResult>> {
        Box::pin(async move { lyrics::fetch_lyrics(&input).await })
    }
}

/// Asking the TeamSpeak server itself: who a client is and who is listening. It arrives with
/// the query connection; without it, restricted commands stay closed and votes are refused.
pub trait ServerQuery: Send + Sync {
    fn execute(&self, server_config_id: i64, virtual_server_id: i64, command: String) -> BoxFuture<'static, Result<String>>;
}

/// One registered bot.
#[derive(Clone)]
pub struct BotEntry {
    pub bot: Arc<dyn BotControl>,
    pub server_config_id: i64,
    pub virtual_server_id: i64,
    /// On another server, where this program has no query rights.
    pub external: bool,
    pub name: String,
    pub nickname: String,
    pub automation: Option<Arc<Mutex<AutomationController>>>,
}

struct SearchSession {
    results: Vec<RankedResult>,
    expires_at: Instant,
}

/// Cancels pending media work: a new `!play`, `!stop` or `!queue clear` replaces it.
#[derive(Clone)]
struct MediaRequest {
    generation: u64,
    changes: watch::Receiver<u64>,
}

impl MediaRequest {
    fn is_cancelled(&self) -> bool {
        *self.changes.borrow() != self.generation
    }

    /// Run `work` until the request is replaced; dropping it stops its processes.
    async fn run<T>(&self, work: impl Future<Output = T>) -> Option<T> {
        let mut changes = self.changes.clone();
        let generation = self.generation;
        tokio::select! {
            value = work => Some(value),
            _ = changes.wait_for(|current| *current != generation) => None,
        }
    }
}

/// What the panel's "play this link" did.
pub struct PlayedLink {
    pub first: QueueItem,
    pub queued_in_background: usize,
    pub playlist_title: Option<String>,
}

/// A track of a link still to fetch.
enum Pending {
    Url(String),
    /// Spotify and Apple Music tracks are matched on YouTube first.
    Track(LinkTrack),
}

/// One vote per listener per track. More than half of the current listeners skip it.
#[derive(Default)]
struct VoteBook {
    tracks: HashMap<i64, (u64, HashSet<String>, bool)>,
}

impl VoteBook {
    fn vote(&mut self, bot_id: i64, token: u64, uid: &str, listeners: &[String]) -> (usize, usize, bool) {
        let track = self.tracks.entry(bot_id).or_default();
        if track.0 != token {
            *track = (token, HashSet::new(), false);
        }
        track.1.retain(|voter| listeners.contains(voter));
        track.1.insert(uid.to_owned());
        let distinct: HashSet<&String> = listeners.iter().collect();
        let required = distinct.len() / 2 + 1;
        let passed = !track.2 && track.1.len() >= required;
        if passed {
            track.2 = true;
        }
        (track.1.len(), required, passed)
    }
}

pub struct ChatService {
    db: Db,
    tracks: Arc<dyn Tracks>,
    query: RwLock<Option<Arc<dyn ServerQuery>>>,
    music_dir: PathBuf,
    bots: RwLock<HashMap<i64, BotEntry>>,
    claims: Claims,
    sessions: Mutex<HashMap<(i64, u64), SearchSession>>,
    requests: Mutex<HashMap<i64, watch::Sender<u64>>>,
    votes: Mutex<VoteBook>,
    /// Whether a running flow answers this line, so it is not reported as unknown.
    #[allow(clippy::type_complexity)]
    flow_lookup: RwLock<Option<Arc<dyn Fn(i64, i64, &str) -> bool + Send + Sync>>>,
    activity: crate::activity::ActivityLog,
}

/// One command being answered.
struct Call {
    bot_id: i64,
    entry: BotEntry,
    clid: u64,
    uid: Option<String>,
    channel: u64,
}

impl Call {
    fn reply(&self, message: &str) {
        self.entry.bot.send_channel(message);
    }
}

fn stored_error(error: teamhub_db::Error) -> Error {
    tracing::warn!(%error, "Chat command database error");
    "The database could not be read.".to_owned()
}

fn is_search_choice(message: &str) -> Option<usize> {
    message.parse::<usize>().ok().filter(|choice| (1..=10).contains(choice) && !message.starts_with('0') && !message.starts_with('+'))
}

/// TS clients wrap links in BBCode: `[URL]https://...[/URL]`.
pub fn strip_url_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let lower = rest.to_ascii_lowercase();
        let Some(open) = lower.find("[url") else { break };
        let after_name = open + 4;
        let opened = lower[after_name..].starts_with(']') || lower[after_name..].starts_with('=');
        let body_start = opened.then(|| lower[after_name..].find(']').map(|at| after_name + at + 1)).flatten();
        let closing = body_start.and_then(|start| lower[start..].find("[/url]").map(|at| (start, start + at)));
        let Some((start, end)) = closing else {
            out.push_str(&rest[..after_name]);
            rest = &rest[after_name..];
            continue;
        };
        out.push_str(&rest[..open]);
        out.push_str(&rest[start..end]);
        rest = &rest[end + "[/url]".len()..];
    }
    out.push_str(rest);
    out.trim().to_owned()
}

/// "Artist - Title", without repeating an artist the upload title already starts with.
fn track_label(artist: Option<&str>, title: &str) -> String {
    let artist = artist.unwrap_or("Unknown");
    if title.to_lowercase().starts_with(&artist.to_lowercase()) {
        return title.to_owned();
    }
    format!("{artist} - {title}")
}

fn is_command_like(command: &str) -> bool {
    let mut chars = command.chars();
    chars.next().is_some_and(char::is_alphanumeric) && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

/// A failed yt-dlp message that means the media itself is gone, not that a tool failed.
pub fn is_unavailable_error(message: &str) -> bool {
    let lower = message.to_lowercase();
    if lower.contains("enoent") || ["yt-dlp not found", "ffmpeg not found", "ffprobe not found"].iter().any(|text| lower.starts_with(text)) {
        return false;
    }
    let subject = ["video", "media", "track", "song", "file", "source"];
    let state = ["unavailable", "removed", "deleted", "not found"];
    subject.iter().any(|word| {
        lower.find(word).is_some_and(|at| state.iter().any(|gone| lower[at..].contains(gone)))
    }) || lower.contains("http error 404")
        || lower.contains("http 404")
        || lower.contains("http error 410")
        || lower.contains("http 410")
        || lower.contains("private video")
}

/// A local path from history, only when it is still under the music folder.
fn resolve_under_music_dir(music_dir: &Path, stored: &str) -> Option<PathBuf> {
    let root = std::fs::canonicalize(music_dir).ok()?;
    let candidate = if Path::new(stored).is_absolute() { PathBuf::from(stored) } else { music_dir.join(stored) };
    let resolved = std::fs::canonicalize(candidate).ok()?;
    (resolved.starts_with(&root) && resolved.is_file()).then_some(resolved)
}

/// What playing a history row again means.
pub enum ReplaySource {
    Link(String),
    Item(QueueItem),
    Gone,
}

enum Replay {
    Url(String),
    Radio(String),
    Local(PathBuf),
    Gone,
}

/// Parse a query response (`key=value key=value|key=value`) into its entries. Lines starting with
/// `error` other than id 0 mean refusal.
fn query_entries(raw: &str) -> Option<Vec<Params>> {
    let mut entries = Vec::new();
    for line in raw.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if let Some(status) = line.strip_prefix("error ") {
            if !status.starts_with("id=0") {
                return None;
            }
            continue;
        }
        for group in line.split('|') {
            let mut params = Params::new();
            for pair in group.split(' ') {
                if let Some((key, value)) = pair.split_once('=') {
                    params.insert(key.to_owned(), teamhub_ts_proto::commands::ts_unescape(value));
                }
            }
            entries.push(params);
        }
    }
    Some(entries)
}

/// Who a client is, from the server. A reused client id never inherits an old identity.
pub fn parse_client_identity(raw: &str, expected_uid: Option<&str>) -> Option<permissions::Identity> {
    let entries = query_entries(raw)?;
    let entry = entries.first()?;
    let uid = entry.get("client_unique_identifier")?;
    if uid.is_empty() || expected_uid.is_some_and(|expected| expected != uid) || entry.get("client_type").is_some_and(|kind| kind == "1") {
        return None;
    }
    let groups = entry.get("client_servergroups")?;
    let mut server_group_ids = Vec::new();
    for group in groups.split(',') {
        server_group_ids.push(group.parse::<i64>().ok().filter(|id| *id > 0)?);
    }
    Some(permissions::Identity { uid: uid.clone(), server_group_ids })
}

/// The channel's voters: non-query clients with identities, without the managed bots. A missing
/// identity gives `None`, because it must not shrink the electorate and lower the majority.
pub fn parse_listeners(raw: &str, channel: u64, bots: &HashSet<u64>) -> Option<Vec<(u64, String)>> {
    let mut listeners = Vec::new();
    for entry in query_entries(raw)? {
        let Some(clid) = entry.get("clid").and_then(|clid| clid.parse::<u64>().ok()).filter(|clid| *clid > 0) else { continue };
        let in_channel = entry.get("cid").and_then(|cid| cid.parse::<u64>().ok()) == Some(channel);
        if !in_channel || entry.get("client_type").map(String::as_str) != Some("0") || bots.contains(&clid) {
            continue;
        }
        listeners.push((clid, entry.get("client_unique_identifier").filter(|uid| !uid.is_empty())?.clone()));
    }
    Some(listeners)
}

impl ChatService {
    pub fn new(db: Db, tracks: Arc<dyn Tracks>, music_dir: PathBuf) -> Arc<Self> {
        let db_for_log = db.clone();
        Arc::new(Self {
            db,
            tracks,
            query: RwLock::new(None),
            music_dir,
            bots: RwLock::default(),
            claims: Claims::default(),
            sessions: Mutex::default(),
            requests: Mutex::default(),
            votes: Mutex::default(),
            flow_lookup: RwLock::new(None),
            activity: crate::activity::ActivityLog::new(db_for_log),
        })
    }

    pub fn set_query(&self, query: Arc<dyn ServerQuery>) {
        *self.query.write().expect("query lock") = Some(query);
    }

    pub fn set_flow_lookup(&self, lookup: Arc<dyn Fn(i64, i64, &str) -> bool + Send + Sync>) {
        *self.flow_lookup.write().expect("flow lock") = Some(lookup);
    }

    pub fn unregister(&self, bot_id: i64) {
        self.bots.write().expect("bots lock").remove(&bot_id);
        self.requests.lock().expect("requests lock").remove(&bot_id);
    }

    fn entry(&self, bot_id: i64) -> Option<BotEntry> {
        self.bots.read().expect("bots lock").get(&bot_id).cloned()
    }

    /// Record a bot for commands and summons, without listening to it.
    pub fn register(&self, bot_id: i64, entry: BotEntry) {
        self.bots.write().expect("bots lock").insert(bot_id, entry);
    }

    /// Register a running bot and answer what it hears until it disconnects.
    pub fn attach(self: &Arc<Self>, bot_id: i64, entry: BotEntry, mut events: broadcast::Receiver<BotEvent>) {
        self.register(bot_id, entry);
        let service = self.clone();
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(BotEvent::Text(params)) => {
                        let service = service.clone();
                        tokio::spawn(async move { service.on_text_message(bot_id, params, None).await });
                    }
                    Ok(BotEvent::MediaCached { song_id, file_path }) => {
                        if let Ok(song_id) = song_id.parse::<i64>() {
                            if let Err(error) = service.db.set_song_file(song_id, &file_path).await {
                                tracing::warn!(%error, "Could not save the cached file of a song");
                            }
                        }
                    }
                    Ok(BotEvent::Disconnected) | Err(broadcast::error::RecvError::Closed) => break,
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                }
            }
        });
    }

    fn begin_request(&self, bot_id: i64) -> MediaRequest {
        let mut requests = self.requests.lock().expect("requests lock");
        let sender = requests.entry(bot_id).or_insert_with(|| watch::channel(0).0);
        let generation = *sender.borrow() + 1;
        sender.send_replace(generation);
        MediaRequest { generation, changes: sender.subscribe() }
    }

    /// Stop whatever this bot is still fetching.
    pub fn cancel_requests(&self, bot_id: i64) {
        let requests = self.requests.lock().expect("requests lock");
        if let Some(sender) = requests.get(&bot_id) {
            sender.send_modify(|generation| *generation += 1);
        }
    }

    fn claim(&self, key: String, hold: Duration) -> bool {
        self.claims.claim(key, hold, Instant::now())
    }

    /// Running bots of a server that sit in a channel.
    fn channel_bots(&self, bot_id: i64, own: &BotEntry, channel: u64) -> Vec<ChannelBot> {
        let describe = |id: i64, entry: &BotEntry| ChannelBot {
            id,
            names: [entry.name.clone(), entry.nickname.clone()].into_iter().filter(|name| !name.is_empty()).collect(),
            active: matches!(entry.bot.snapshot().status, Status::Playing | Status::Paused),
        };
        let mut bots = vec![describe(bot_id, own)];
        for (id, entry) in self.bots.read().expect("bots lock").iter() {
            let same_server = entry.server_config_id == own.server_config_id && entry.virtual_server_id == own.virtual_server_id;
            let running = entry.bot.snapshot().status != Status::Stopped;
            if *id != bot_id && same_server && running && channel > 0 && entry.bot.current_channel() == channel {
                bots.push(describe(*id, entry));
            }
        }
        bots
    }

    /// The client ids of this server's connected music bots.
    fn managed_clids(&self, own: &BotEntry) -> HashSet<u64> {
        self.bots
            .read()
            .expect("bots lock")
            .values()
            .filter(|entry| {
                entry.server_config_id == own.server_config_id
                    && entry.virtual_server_id == own.virtual_server_id
                    && entry.bot.snapshot().status != Status::Stopped
            })
            .map(|entry| entry.bot.client_id())
            .collect()
    }

    fn human_listeners(&self, entry: &BotEntry) -> usize {
        let managed = self.managed_clids(entry);
        entry.bot.channel_members().iter().filter(|clid| !managed.contains(clid)).count()
    }

    /// Parse a chat line and answer it. Numbers answer an open search.
    pub async fn on_text_message(self: &Arc<Self>, bot_id: i64, data: Params, reply_channel: Option<u64>) {
        let Some(entry) = self.entry(bot_id) else { return };
        let bot = entry.bot.clone();
        let clid = data.get("invokerid").and_then(|id| id.parse::<u64>().ok()).unwrap_or(0);
        if clid == 0 {
            return;
        }
        let message = data.get("msg").map(|text| text.trim()).unwrap_or_default();
        let search_choice = is_search_choice(message);
        if !message.starts_with('!') && search_choice.is_none() {
            return;
        }
        // During the anti-flood block every action would be refused and extend it.
        if bot.flood_hold_active() {
            bot.note_ignored_command();
            return;
        }
        let (command, raw_args) = if search_choice.is_some() {
            ("play".to_owned(), String::new())
        } else {
            let mut parts = message[1..].split_whitespace();
            let command = parts.next().unwrap_or_default().to_lowercase();
            (command, parts.collect::<Vec<_>>().join(" "))
        };

        let parsed_channel = ["target", "invokerchannelid", "cid"]
            .iter()
            .find_map(|key| data.get(*key).and_then(|value| value.parse::<u64>().ok()).filter(|value| *value > 0));
        let home = bot.current_channel();
        let channel = reply_channel.or(Some(home).filter(|home| *home > 0)).or(parsed_channel).unwrap_or(0);

        // Every bot in the channel hears this line: exactly one answers.
        let mut rest = raw_args;
        let peers = self.channel_bots(bot_id, &entry, channel);
        if peers.len() > 1 {
            let (target, args) = parse_bot_target(&command, &rest, &peers);
            if choose_command_bot(&peers, target) != Some(bot_id) {
                return;
            }
            let key = format!("{}:{}:{channel}:{clid}:{command}:{message}", entry.server_config_id, entry.virtual_server_id);
            if !self.claim(key, CHANNEL_COMMAND_COOLDOWN) {
                return;
            }
            rest = args;
        }
        let args = strip_url_tags(&rest);

        if clid == bot.client_id() {
            return; // Its own line.
        }

        let disabled = match self.disabled_commands(bot_id).await {
            Ok(disabled) => disabled,
            Err(error) => {
                tracing::warn!(error, "Bot command settings could not be read");
                return;
            }
        };
        if disabled.contains(canonical_command(&command)) || (search_choice.is_some() && disabled.contains("ara")) {
            return;
        }

        let uid = data.get("invokeruid").cloned();
        let call = Call { bot_id, entry, clid, uid, channel };

        if is_builtin(&command) {
            if !self.check_media_command(&call, &command, &args).await || call.entry.bot.flood_hold_active() {
                return;
            }
            let who = data.get("invokername").map_or("", String::as_str);
            self.activity.record(bot_id, "teamspeak", who, &crate::activity::describe_chat_command(&command, &args, search_choice)).await;
            if let Err(error) = self.dispatch(&call, &command, &args, search_choice).await {
                tracing::warn!(error, command, "Chat command failed");
                call.reply(&format!("Error: {error}"));
            }
            return;
        }
        if let Err(error) = self.custom_command(&call, &command, message).await {
            tracing::warn!(error, command, "Custom chat command failed");
        }
    }

    async fn disabled_commands(&self, bot_id: i64) -> Result<HashSet<String>> {
        let key = format!("bot_extensions:{bot_id}");
        let Some(text) = self.db.setting(&key).await.map_err(stored_error)? else { return Ok(HashSet::new()) };
        let value: serde_json::Value = serde_json::from_str(&text).map_err(|_| "Invalid bot command settings".to_owned())?;
        let commands = value["disabledCommands"].as_array().ok_or("Invalid bot command settings")?;
        if commands.len() > 256 {
            return Err("Invalid bot command settings".to_owned());
        }
        let mut disabled = HashSet::new();
        for command in commands {
            let name = command.as_str().ok_or("Invalid bot command settings")?;
            let valid = (1..=32).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-');
            if !valid {
                return Err("Invalid bot command settings".to_owned());
            }
            disabled.insert(canonical_command(name).to_owned());
        }
        Ok(disabled)
    }

    /// The gate before any side effect. A failed lookup never grants rights.
    async fn check_media_command(&self, call: &Call, command: &str, args: &str) -> bool {
        if matches!(command_group(command, args), Some(CommandGroup::Info | CommandGroup::Listener)) {
            return true;
        }
        let key = permissions_key(call.entry.server_config_id, call.entry.virtual_server_id);
        if let Ok(stored) = self.db.setting(&key).await {
            let policy = Permissions::from_stored(stored.as_deref());
            if authorize(&policy, command, args, None) {
                return true;
            }
            if let Some(identity) = self.resolve_identity(call).await {
                if authorize(&policy, command, args, Some(&identity)) {
                    return true;
                }
            }
        }
        self.private_reply(call, "You are not allowed to use this command.");
        false
    }

    async fn resolve_identity(&self, call: &Call) -> Option<permissions::Identity> {
        // That client id belongs to another server: looking it up here could match someone else.
        if call.entry.external {
            return None;
        }
        let query = self.query.read().expect("query lock").clone()?;
        let raw = query.execute(call.entry.server_config_id, call.entry.virtual_server_id, format!("clientinfo clid={}", call.clid)).await.ok()?;
        parse_client_identity(&raw, call.uid.as_deref())
    }

    fn private_reply(&self, call: &Call, message: &str) {
        let key = format!("{}:{}:0:{}:media-private", call.entry.server_config_id, call.entry.virtual_server_id, call.clid);
        if self.claim(key, CHAT_REPLY_COOLDOWN) {
            call.entry.bot.send_private(call.clid, message);
        }
    }

    async fn dispatch(self: &Arc<Self>, call: &Call, command: &str, args: &str, search_choice: Option<usize>) -> Result<()> {
        if let Some(choice) = search_choice {
            return self.search_choice(call, choice).await;
        }
        match command {
            "help" => self.help(call).await,
            "commands" => self.custom_commands_list(call).await,
            "here" | "come" => self.here(call, args).await,
            "playlist" | "pl" => self.playlist(call, args).await,
            "repeat" => {
                self.repeat(call, args);
                Ok(())
            }
            "seek" => {
                self.seek(call, args).await;
                Ok(())
            }
            "remove" => {
                self.remove(call, args);
                Ok(())
            }
            "radio" => self.radio(call, args).await,
            "play" => self.play(call, args).await,
            "ara" => self.search(call, args).await,
            "history" => self.history(call, args).await,
            "again" => self.again(call, args).await,
            "stop" => {
                self.stop(call).await;
                Ok(())
            }
            "pause" => {
                self.pause(call).await;
                Ok(())
            }
            "skip" | "next" => {
                self.skip(call).await;
                Ok(())
            }
            "voteskip" => self.vote_skip(call).await,
            "prev" => {
                self.previous(call).await;
                Ok(())
            }
            "vol" | "volume" => {
                self.volume(call, args).await;
                Ok(())
            }
            "np" | "nowplaying" => {
                self.now_playing(call);
                Ok(())
            }
            "queue" | "add" => self.queue_command(call, args).await,
            "shuffle" => {
                self.shuffle(call, args).await;
                Ok(())
            }
            "lyrics" => {
                self.lyrics(call, args).await;
                Ok(())
            }
            // The rest arrive with what they need: video and IPTV with the stream step, the
            // listener remote with the API, speech with TTS.
            "stream" | "stopstream" | "viewers" | "channels" | "tv" | "iptv" | "remote" | "say" => {
                call.reply(&format!("!{command} is not available yet."));
                Ok(())
            }
            _ => Ok(()),
        }
    }

    // ─── Information ───────────────────────────────────────────

    async fn enabled_custom(&self, server_config_id: i64) -> Result<Vec<CustomLine>> {
        let commands = self.db.enabled_chat_commands(server_config_id).await.map_err(stored_error)?;
        Ok(commands.into_iter().map(|command| CustomLine { name: command.name, description: command.description }).collect())
    }

    fn automation_summary(entry: &BotEntry) -> Option<AutomationSummary> {
        entry.automation.as_ref().and_then(|automation| automation.lock().expect("automation lock").summary())
    }

    async fn help(&self, call: &Call) -> Result<()> {
        let channel = if call.channel > 0 { call.channel } else { 0 };
        let key = format!("{}:{}:{channel}:{}", call.entry.server_config_id, call.entry.virtual_server_id, call.clid);
        if !self.claim(format!("help:{key}"), HELP_COOLDOWN) {
            return Ok(());
        }
        let custom = self.enabled_custom(call.entry.server_config_id).await?;
        call.reply(&format::help_message(&COMMAND_HELP, &custom, Self::automation_summary(&call.entry)));
        Ok(())
    }

    async fn custom_commands_list(&self, call: &Call) -> Result<()> {
        if !self.claim_info(call, "commands") {
            return Ok(());
        }
        call.reply(&format::custom_commands_message(&self.enabled_custom(call.entry.server_config_id).await?));
        Ok(())
    }

    fn claim_info(&self, call: &Call, name: &str) -> bool {
        let key = format!("{}:{}:{}:{}:{name}", call.entry.server_config_id, call.entry.virtual_server_id, call.channel, call.clid);
        self.claim(key, CHAT_REPLY_COOLDOWN)
    }

    async fn custom_command(&self, call: &Call, command: &str, message: &str) -> Result<()> {
        let server = call.entry.server_config_id;
        let custom = self.db.chat_command(server, command).await.map_err(stored_error)?;
        let Some(custom) = custom.filter(|custom| custom.enabled) else {
            if !is_command_like(command) {
                return Ok(());
            }
            let lookup = self.flow_lookup.read().expect("flow lock").clone();
            if lookup.is_some_and(|lookup| lookup(server, call.entry.virtual_server_id, message)) {
                return Ok(());
            }
            if self.claim_info(call, &format!("unknown:{command}")) {
                call.reply(&format::unknown_command_message(command, suggest_builtin(command)));
            }
            return Ok(());
        };
        if self.claim_info(call, command) {
            call.reply(&custom.response);
        }
        Ok(())
    }

    fn now_playing(&self, call: &Call) {
        let state = call.entry.bot.snapshot();
        let Some(track) = state.now_playing else {
            call.reply("_Nothing is playing._");
            return;
        };
        let queue = call.entry.bot.queue();
        let queue = queue.lock().expect("queue lock");
        let upcoming = queue.upcoming().into_iter().take(5).map(|item| TrackLine { title: item.title.clone(), artist: item.artist.clone(), duration: item.duration }).collect();
        call.reply(&format::now_playing_message(&NowPlaying {
            title: track.title,
            artist: track.artist,
            position: Some(state.position_seconds),
            duration: track.duration,
            paused: state.status == Status::Paused,
            upcoming,
            total_queue_length: queue.len(),
            queue_index: queue.index(),
        }));
    }

    async fn lyrics(&self, call: &Call, args: &str) {
        let (input, label) = if args.trim().is_empty() {
            let Some(track) = call.entry.bot.snapshot().now_playing else {
                call.reply("Nothing playing. Usage: !lyrics [artist - title]");
                return;
            };
            lyrics_input_from_track(track.artist.as_deref(), &track.title)
        } else {
            (LyricsQuery { query: Some(args.trim().to_owned()), ..LyricsQuery::default() }, args.trim().to_owned())
        };
        call.reply("Looking up lyrics…");
        let Some(found) = self.tracks.lyrics(input).await else {
            call.reply(&format!("Lyrics not found for \"{label}\"."));
            return;
        };
        if found.instrumental {
            call.reply(&format!("♪ {} — {}: instrumental track.", found.artist, found.title));
            return;
        }
        let artist = if found.artist.is_empty() { String::new() } else { format!("{} — ", found.artist) };
        for chunk in chunk_lyrics(&format!("🎤 {artist}{}", found.title), &found.lyrics, LYRICS_CHUNK_CHARS) {
            call.reply(&chunk);
        }
    }

    // ─── Summoning ─────────────────────────────────────────────

    fn summonable(&self, own: &BotEntry) -> Vec<(i64, BotEntry)> {
        let bots = self.bots.read().expect("bots lock");
        let mut found: Vec<(i64, BotEntry)> = bots
            .iter()
            .filter(|(_, entry)| {
                entry.server_config_id == own.server_config_id
                    && entry.virtual_server_id == own.virtual_server_id
                    && entry.bot.snapshot().status != Status::Stopped
            })
            .map(|(id, entry)| (*id, entry.clone()))
            .collect();
        found.sort_by_key(|(id, _)| *id);
        found
    }

    fn display_name(id: i64, entry: &BotEntry) -> String {
        let name = [&entry.name, &entry.nickname].into_iter().find(|name| !name.is_empty()).cloned().unwrap_or_else(|| format!("Bot {id}"));
        name.chars().take(60).collect()
    }

    /// Idle: no other humans in its channel, and not a 24/7 bot.
    fn is_idle(&self, entry: &BotEntry) -> bool {
        let keeps = entry.automation.as_ref().is_some_and(|automation| automation.lock().expect("automation lock").keep_channel());
        !keeps && (entry.bot.current_channel() == 0 || self.human_listeners(entry) == 0)
    }

    async fn here(self: &Arc<Self>, call: &Call, args: &str) -> Result<()> {
        let bot = &call.entry.bot;
        // The sender's own channel first: a private `!here` comes from wherever they sit.
        let channel = bot.client_channel(call.clid).filter(|channel| *channel > 0).unwrap_or(call.channel);
        let server = (call.entry.server_config_id, call.entry.virtual_server_id);
        if channel == 0 {
            if self.claim(format!("{}:{}:0:{}:unknown:{args}", server.0, server.1, call.clid), HERE_ACTION_COOLDOWN) {
                bot.send_channel("Could not determine this channel yet. Wait a second and try !here again.");
            }
            return Ok(());
        }
        if !self.claim(format!("{}:{}:{channel}:{}:{args}", server.0, server.1, call.clid), HERE_ACTION_COOLDOWN) {
            return Ok(());
        }
        let candidates = self.summonable(&call.entry);
        if candidates.is_empty() {
            call.reply("No music bots are available right now.");
            return Ok(());
        }
        if !args.is_empty() {
            let Some(target_id) = args.trim().parse::<i64>().ok().filter(|id| id.to_string() == args.trim()) else {
                call.reply("Usage: !here [id] — Use !here to list bots.");
                return Ok(());
            };
            match candidates.iter().find(|(id, _)| *id == target_id) {
                // An explicit id is intentional, even when the bot is with other users.
                Some((id, target)) => self.summon(call, *id, target, channel),
                None => call.reply(&format!("Bot #{target_id} is not available. Use !here to list bots.")),
            }
            return Ok(());
        }
        if let [(id, target)] = candidates.as_slice() {
            self.summon(call, *id, target, channel);
            return Ok(());
        }

        let here: Vec<&(i64, BotEntry)> = candidates.iter().filter(|(_, entry)| entry.bot.current_channel() == channel).collect();
        let idle: Vec<&(i64, BotEntry)> = candidates.iter().filter(|(_, entry)| self.is_idle(entry)).collect();
        let idle_elsewhere: Vec<&&(i64, BotEntry)> = idle.iter().filter(|(_, entry)| entry.bot.current_channel() != channel).collect();
        let idle_ids: HashSet<i64> = idle.iter().map(|(id, _)| *id).collect();
        let entry_line = |id: i64, entry: &BotEntry| HereEntry {
            id,
            name: Self::display_name(id, entry),
            here: entry.bot.current_channel() == channel,
            idle: Some(idle_ids.contains(&id)),
        };
        // One bot here and nobody idle to offer: confirm it. Several choices: list them.
        let (lines, busy): (Vec<HereEntry>, bool) = if here.len() == 1 && idle_elsewhere.is_empty() {
            let (id, target) = here[0];
            self.summon(call, *id, target, channel);
            return Ok(());
        } else if !here.is_empty() && !idle_elsewhere.is_empty() {
            let mut lines: Vec<HereEntry> = here.iter().map(|(id, entry)| entry_line(*id, entry)).collect();
            lines.extend(idle_elsewhere.iter().map(|(id, entry)| entry_line(*id, entry)));
            (lines, false)
        } else if here.len() > 1 {
            (here.iter().map(|(id, entry)| entry_line(*id, entry)).collect(), false)
        } else if let [(id, target)] = idle.as_slice().iter().map(|pair| &**pair).collect::<Vec<_>>().as_slice() {
            self.summon(call, *id, target, channel);
            return Ok(());
        } else if idle.len() > 1 {
            (idle.iter().map(|(id, entry)| entry_line(*id, entry)).collect(), false)
        } else {
            // Do not take a bot away from other users unasked.
            (candidates.iter().map(|(id, entry)| HereEntry { id: *id, name: Self::display_name(*id, entry), here: false, idle: None }).collect(), true)
        };
        if self.claim(format!("list:{}:{}:{channel}:{}", server.0, server.1, call.clid), HERE_LIST_COOLDOWN) {
            call.reply(&format::here_list_message(&lines, busy));
        }
        Ok(())
    }

    fn summon(&self, call: &Call, id: i64, target: &BotEntry, channel: u64) {
        let name = Self::display_name(id, target);
        if target.bot.current_channel() == channel {
            call.reply(&format!("{name} [#{id}] is already here."));
            return;
        }
        if target.bot.client_id() == 0 {
            call.reply(&format!("Could not move {name} [#{id}]: bot is not fully connected."));
            return;
        }
        // Announced first: the user should see it where they typed, before the bot leaves.
        call.reply(&format!("{name} [#{id}] is joining."));
        target.bot.join_channel(channel);
    }

    /// Move to the channel the command came from, when it is another one.
    fn join_for_command(&self, call: &Call) {
        if call.channel > 0 && call.entry.bot.current_channel() != call.channel {
            call.entry.bot.join_channel(call.channel);
        }
    }

    // ─── Playing ───────────────────────────────────────────────

    async fn radio(&self, call: &Call, args: &str) -> Result<()> {
        let stations = self.db.radio_stations(call.entry.server_config_id).await.map_err(stored_error)?;
        if stations.is_empty() {
            call.reply("No radio stations configured.");
            return Ok(());
        }
        if args.is_empty() {
            let list: Vec<(i64, String, Option<String>)> = stations.into_iter().map(|station| (station.id, station.name, station.genre)).collect();
            call.reply(&format::radio_list_message(&list));
            return Ok(());
        }
        let Some(id) = args.parse::<i64>().ok().filter(|id| *id > 0 && !args.starts_with('+')) else {
            call.reply("Usage: !radio <id> — Use !radio to list stations.");
            return Ok(());
        };
        let Some(station) = stations.into_iter().find(|station| station.id == id) else {
            call.reply(&format!("Station #{id} not found. Use !radio to list stations."));
            return Ok(());
        };
        let request = self.begin_request(call.bot_id);
        self.join_for_command(call);
        if request.is_cancelled() {
            return Ok(());
        }
        let item = QueueItem {
            id: format!("radio_{}", station.id),
            title: station.name.clone(),
            artist: Some(station.genre.unwrap_or_else(|| "Radio".to_owned())),
            duration: None,
            source: Source::Radio,
            file_path: String::new(),
            source_url: None,
            stream_url: Some(station.url),
        };
        call.entry.bot.play_item(item.clone());
        call.entry.bot.settle().await;
        self.save_requests(&call.entry, call.bot_id, std::slice::from_ref(&item));
        call.reply(&format!("Now playing: {}", station.name));
        Ok(())
    }

    async fn play(self: &Arc<Self>, call: &Call, args: &str) -> Result<()> {
        if args.is_empty() {
            self.join_for_command(call);
            if call.entry.bot.snapshot().status == Status::Paused {
                call.entry.bot.resume();
                call.reply("Resumed.");
            } else {
                call.reply("Usage: !play <url|song name>");
            }
            return Ok(());
        }
        let request = self.begin_request(call.bot_id);
        self.join_for_command(call);

        let lower = args.to_ascii_lowercase();
        let url = if lower.starts_with("http://") || lower.starts_with("https://") {
            call.reply("Loading...");
            args.to_owned()
        } else {
            let query: String = args.trim().chars().take(MAX_SONG_QUERY_CHARS).collect();
            call.reply(&format!("Searching YouTube for \"{query}\"..."));
            match request.run(self.tracks.find_song(query.clone())).await {
                None => return Ok(()),
                Some(Ok(Some(song))) => youtube::watch_url(&song.id),
                Some(Ok(None)) => {
                    call.reply(&format!("No results for \"{query}\"."));
                    return Ok(());
                }
                Some(Err(error)) => {
                    call.reply(&format!("Search failed: {error}"));
                    return Ok(());
                }
            }
        };
        if let Err(error) = self.enqueue_media_url(call, &url, request.clone()).await {
            if !request.is_cancelled() {
                call.reply(&format!("Failed to play: {error}"));
            }
        }
        Ok(())
    }

    /// Start a track when the bot is idle, otherwise add it. True when it was added behind music.
    async fn start_or_append(&self, call: &Call, item: QueueItem) -> Result<bool> {
        self.place_track(call.bot_id, &call.entry, item, false).await
    }

    /// `queue_only` never starts playback.
    async fn place_track(&self, bot_id: i64, entry: &BotEntry, item: QueueItem, queue_only: bool) -> Result<bool> {
        let bot = &entry.bot;
        let status = bot.snapshot().status;
        if status == Status::Stopped {
            return Err("Bot is not connected".to_owned());
        }
        let queue = bot.queue();
        let queued = queue_only || matches!(status, Status::Playing | Status::Paused);
        {
            let mut queue = queue.lock().expect("queue lock");
            queue.add(item.clone()).map_err(|error: QueueFull| error.to_string())?;
            if !queued {
                let last = queue.len() - 1;
                queue.play_at(last);
            }
        }
        if !queued {
            bot.play_item(item.clone());
            bot.settle().await;
        }
        self.save_requests(entry, bot_id, std::slice::from_ref(&item));
        Ok(queued)
    }

    /// The panel's "play this link". `queue_only` adds without interrupting anything.
    pub async fn play_link(self: &Arc<Self>, bot_id: i64, url: &str, queue_only: bool) -> Result<PlayedLink> {
        let entry = self.entry(bot_id).ok_or("Bot is not connected")?;
        let request = self.begin_request(bot_id);
        let (first_url, rest, title) = self.resolve_link(url, &request).await?.ok_or("The request was replaced")?;
        let Some(first) = request.run(self.tracks.download(first_url)).await else { return Err("The request was replaced".to_owned()) };
        let first = first?;
        if request.is_cancelled() {
            return Err("The request was replaced".to_owned());
        }
        let room = entry.bot.queue().lock().expect("queue lock").room();
        if rest.len() + 1 > room {
            return Err(QueueFull { requested: rest.len() + 1, room }.to_string());
        }
        self.place_track(bot_id, &entry, first.clone(), queue_only).await?;
        let background = rest.len();
        if !rest.is_empty() {
            self.queue_in_background(bot_id, entry, rest, request, queue_only);
        }
        Ok(PlayedLink { first, queued_in_background: background, playlist_title: title })
    }

    /// Where a history row can be played from now. A stop while this looks answers
    /// [`REPLAY_STOPPED`]; any other error is a name that could not be resolved, worth a retry.
    pub async fn replay_source(&self, bot_id: i64, row: &teamhub_db::models::MusicRequest) -> Result<ReplaySource> {
        let request = self.begin_request(bot_id);
        let reference = self.replay_reference(&row.url, &row.source).await?;
        if request.is_cancelled() {
            return Err(REPLAY_STOPPED.to_owned());
        }
        let item = |file_path: String, stream_url: Option<String>, source: Source| QueueItem {
            id: format!("history_{}", row.id),
            title: row.title.clone(),
            artist: row.artist.clone(),
            duration: row.duration,
            source,
            file_path,
            source_url: None,
            stream_url,
        };
        Ok(match reference {
            Replay::Gone => ReplaySource::Gone,
            Replay::Url(url) => ReplaySource::Link(url),
            Replay::Local(path) => ReplaySource::Item(item(path.display().to_string(), None, Source::Local)),
            Replay::Radio(url) => ReplaySource::Item(item(String::new(), Some(url), Source::Radio)),
        })
    }

    /// The panel's search box: the same ranked results as `!ara`.
    pub async fn search_music(&self, query: &str) -> (bool, Vec<RankedResult>) {
        (self.tracks.has_cookies(), self.tracks.search(query.to_owned()).await)
    }

    /// Resolve a link, start or queue its first track, and fetch the rest in the background.
    async fn enqueue_media_url(self: &Arc<Self>, call: &Call, raw_url: &str, request: MediaRequest) -> Result<()> {
        self.join_for_command(call);
        let Some((first_url, rest, title)) = self.resolve_link(raw_url, &request).await? else { return Ok(()) };
        self.start_and_queue_rest(call, first_url, rest, title, request).await
    }

    /// What a link holds: the first address to fetch, the rest, and the collection's name.
    /// `None` when the request was replaced meanwhile.
    async fn resolve_link(&self, raw_url: &str, request: &MediaRequest) -> Result<Option<(String, Vec<Pending>, Option<String>)>> {
        let spotify = is_spotify_share_url(raw_url);
        if spotify || is_apple_music_share_url(raw_url) {
            let label = if spotify { "Spotify" } else { "Apple Music" };
            let Some(found) = request.run(self.tracks.resolve_share_link(raw_url.to_owned())).await else { return Ok(None) };
            let found = found?;
            let Some(first) = found.tracks.first() else {
                return Err(format!("Could not resolve any tracks from that {label} URL"));
            };
            let Some(song) = request.run(self.tracks.find_song(first.query())).await else { return Ok(None) };
            let Some(song) = song? else {
                return Err(format!("No YouTube match for {label} track: {} - {}", first.artist, first.title));
            };
            let rest: Vec<Pending> = found.tracks[1..].iter().take(PLAYLIST_CAP - 1).cloned().map(Pending::Track).collect();
            let title = found.title.filter(|_| found.tracks.len() > 1);
            return Ok(Some((youtube::watch_url(&song.id), rest, title)));
        }
        let (urls, playlist_title) = if is_youtube_url(raw_url) {
            let parsed = youtube::parse_youtube_url(raw_url);
            if parsed.list_id.is_some() && parsed.video_id.is_none() {
                let Some(expanded) = request.run(self.tracks.expand(raw_url.to_owned(), PLAYLIST_CAP)).await else { return Ok(None) };
                let expanded = expanded.map_err(|_| "Could not resolve any videos from that playlist URL".to_owned())?;
                if expanded.urls.is_empty() {
                    return Err("Could not resolve any videos from that playlist URL".to_owned());
                }
                (expanded.urls, expanded.title)
            } else if let Some(watch) = parsed.watch_url {
                (vec![watch], None)
            } else {
                return Err("Could not resolve that YouTube URL".to_owned());
            }
        } else {
            (vec![raw_url.to_owned()], None)
        };

        let rest: Vec<Pending> = urls[1..].iter().cloned().map(Pending::Url).collect();
        Ok(Some((urls[0].clone(), rest, playlist_title)))
    }

    /// Download and start the first track, announce it, and leave the rest to the background.
    async fn start_and_queue_rest(
        self: &Arc<Self>,
        call: &Call,
        first_url: String,
        rest: Vec<Pending>,
        playlist_title: Option<String>,
        request: MediaRequest,
    ) -> Result<()> {
        let room = call.entry.bot.queue().lock().expect("queue lock").room();
        if rest.len() + 1 > room {
            return Err(QueueFull { requested: rest.len() + 1, room }.to_string());
        }
        let Some(first) = request.run(self.tracks.download(first_url)).await else { return Ok(()) };
        let first = first?;
        if request.is_cancelled() {
            return Ok(());
        }
        let queued = self.start_or_append(call, first.clone()).await?;

        let note = if rest.is_empty() {
            String::new()
        } else {
            let from = playlist_title.map_or_else(|| " playlist".to_owned(), |title| format!(" \"{title}\""));
            format!(" (+{} more from{from})", rest.len())
        };
        let track = track_label(first.artist.as_deref(), &first.title);
        if queued {
            let number = {
                let queue = call.entry.bot.queue();
                let queue = queue.lock().expect("queue lock");
                queue.pruned() + queue.len()
            };
            call.reply(&format!("Queued: {track} (position #{number}){note}"));
        } else {
            call.reply(&format!("Now playing: {track}{note}"));
        }
        if !rest.is_empty() {
            self.queue_in_background(call.bot_id, call.entry.clone(), rest, request, false);
        }
        Ok(())
    }

    fn queue_in_background(self: &Arc<Self>, bot_id: i64, entry: BotEntry, pending: Vec<Pending>, request: MediaRequest, queue_only: bool) {
        let service = self.clone();
        tokio::spawn(async move {
            for track in pending {
                let url = match track {
                    Pending::Url(url) => url,
                    Pending::Track(track) => {
                        let label = format!("{} - {}", track.artist, track.title);
                        let Some(song) = request.run(service.tracks.find_song(track.query())).await else { return };
                        match song {
                            Ok(Some(song)) => youtube::watch_url(&song.id),
                            Ok(None) => {
                                tracing::warn!(label, "No YouTube match for a track of a link");
                                continue;
                            }
                            Err(error) => {
                                tracing::warn!(error, label, "A track of a link could not be matched");
                                continue;
                            }
                        }
                    }
                };
                let Some(found) = request.run(service.tracks.download(url)).await else { return };
                let item = match found {
                    Ok(item) => item,
                    Err(error) => {
                        tracing::warn!(error, "A playlist track could not be queued");
                        continue;
                    }
                };
                let idle = !queue_only && entry.bot.snapshot().status == Status::Connected;
                {
                    let queue = entry.bot.queue();
                    let mut queue = queue.lock().expect("queue lock");
                    if queue.add(item.clone()).is_err() {
                        return;
                    }
                    if idle {
                        let last = queue.len() - 1;
                        queue.play_at(last);
                    }
                }
                if idle {
                    entry.bot.play_item(item.clone());
                    entry.bot.settle().await;
                }
                service.save_requests(&entry, bot_id, std::slice::from_ref(&item));
            }
        });
    }

    fn save_requests(&self, entry: &BotEntry, bot_id: i64, items: &[QueueItem]) {
        self.save_requests_for(entry.server_config_id, bot_id, items);
    }

    /// Remember requests in the bot's history, without holding anything up.
    pub fn save_requests_for(&self, server: i64, bot_id: i64, items: &[QueueItem]) {
        let requests: Vec<NewMusicRequest> = items
            .iter()
            .filter_map(|item| {
                let local = (item.source == Source::Local).then_some(item.file_path.as_str());
                let url = item.source_url.as_deref().or(item.stream_url.as_deref()).or(local).map(str::trim).filter(|url| !url.is_empty())?;
                Some(NewMusicRequest {
                    title: item.title.clone(),
                    artist: item.artist.clone(),
                    duration: item.duration,
                    url: url.to_owned(),
                    source: item.source.as_str().to_owned(),
                })
            })
            .collect();
        let db = self.db.clone();
        // Never holds up playback; failing to record a request is not worth a word in chat.
        tokio::spawn(async move {
            if let Err(error) = db.record_music_requests(server, bot_id, &requests, history_limit()).await {
                tracing::warn!(%error, "Could not save music request history");
            }
        });
    }

    async fn seek(&self, call: &Call, args: &str) {
        let body = args.strip_prefix(['+', '-']).unwrap_or(args);
        let number_like = !body.is_empty()
            && body.split_once('.').map_or(body.bytes().all(|b| b.is_ascii_digit()), |(whole, part)| {
                !whole.is_empty() && !part.is_empty() && whole.bytes().chain(part.bytes()).all(|b| b.is_ascii_digit())
            });
        let Some(value) = args.parse::<f64>().ok().filter(|value| number_like && value.is_finite()) else {
            call.reply("Usage: !seek <seconds|+seconds|-seconds>");
            return;
        };
        let state = call.entry.bot.snapshot();
        if !state.can_seek {
            call.reply("Current source cannot be seeked. Play a local/downloaded track first.");
            return;
        }
        let relative = args.starts_with(['+', '-']);
        let limit = state.now_playing.as_ref().and_then(|item| item.duration).unwrap_or(f64::INFINITY);
        let target = ((if relative { state.position_seconds } else { 0.0 }) + value).clamp(0.0, limit);
        call.entry.bot.seek(target);
        call.reply(&format!("Seeked to {} seconds.", target.floor() as u64));
    }

    /// Cancel what is loading and stop. Music clears the queue; radio keeps it.
    async fn stop(&self, call: &Call) {
        self.cancel_requests(call.bot_id);
        let state = call.entry.bot.snapshot();
        let live = state.now_playing.as_ref().is_some_and(|item| item.stream_url.is_some());
        if !live {
            call.entry.bot.queue().lock().expect("queue lock").clear();
        }
        call.entry.bot.stop_audio();
        call.entry.bot.settle().await;
        call.reply("Playback stopped.");
    }

    async fn pause(&self, call: &Call) {
        match call.entry.bot.snapshot().status {
            Status::Paused => {
                call.entry.bot.resume();
                call.reply("Resumed.");
            }
            Status::Playing => {
                call.entry.bot.pause();
                call.reply("Paused.");
            }
            _ => call.reply("Nothing is playing."),
        }
        call.entry.bot.settle().await;
    }

    async fn skip(&self, call: &Call) {
        let next = call.entry.bot.queue().lock().expect("queue lock").next().cloned();
        match next {
            Some(item) => {
                let title = item.title.clone();
                call.entry.bot.play_item(item);
                call.reply(&format!("Skipped to: {title}"));
            }
            None => {
                // Unlike `!stop`, requests still loading are kept.
                call.entry.bot.stop_audio();
                call.reply("Queue empty — playback stopped.");
            }
        }
        call.entry.bot.settle().await;
    }

    async fn previous(&self, call: &Call) {
        let previous = call.entry.bot.queue().lock().expect("queue lock").previous().cloned();
        match previous {
            Some(item) => {
                let title = item.title.clone();
                call.entry.bot.play_item(item);
                call.entry.bot.settle().await;
                call.reply(&format!("Previous: {title}"));
            }
            None => call.reply("No previous track."),
        }
    }

    async fn volume(&self, call: &Call, args: &str) {
        if args.is_empty() {
            call.reply(&format!("Volume: {}%", call.entry.bot.snapshot().volume));
            return;
        }
        let Some(volume) = args.parse::<u8>().ok().filter(|volume| *volume <= 100) else {
            call.reply("Usage: !vol <0-100>");
            return;
        };
        call.entry.bot.set_volume(volume);
        call.entry.bot.settle().await;
        call.reply(&format!("Volume set to {}%.", call.entry.bot.snapshot().volume));
    }

    async fn vote_skip(&self, call: &Call) -> Result<()> {
        let state = call.entry.bot.snapshot();
        if state.now_playing.is_none() {
            self.private_reply(call, "Nothing is playing.");
            return Ok(());
        }
        let query = if call.entry.external { None } else { self.query.read().expect("query lock").clone() };
        let channel = call.entry.bot.current_channel();
        let listeners = match query {
            Some(query) => {
                let raw = query.execute(call.entry.server_config_id, call.entry.virtual_server_id, "clientlist -uid".to_owned()).await.ok();
                raw.and_then(|raw| parse_listeners(&raw, channel, &self.managed_clids(&call.entry)))
            }
            None => None,
        };
        let now = call.entry.bot.snapshot();
        if call.entry.bot.flood_hold_active() || now.playback_token != state.playback_token || call.entry.bot.current_channel() != channel || now.now_playing.is_none() {
            return Ok(());
        }
        let voter = listeners.as_ref().and_then(|listeners| {
            listeners.iter().find(|(clid, uid)| *clid == call.clid && call.uid.as_ref().is_none_or(|expected| expected == uid))
        });
        let Some((_, uid)) = voter else {
            self.private_reply(call, "Vote-skip needs a verified listener in the bot channel.");
            return Ok(());
        };
        let electorate: Vec<String> = listeners.as_ref().map(|listeners| listeners.iter().map(|(_, uid)| uid.clone()).collect()).unwrap_or_default();
        let (votes, required, passed) = self.votes.lock().expect("votes lock").vote(call.bot_id, state.playback_token, uid, &electorate);
        if passed {
            self.skip(call).await;
        } else {
            self.private_reply(call, &format!("Vote-skip: {votes}/{required} votes needed."));
        }
        Ok(())
    }

    // ─── Search ────────────────────────────────────────────────

    async fn search(&self, call: &Call, raw_query: &str) -> Result<()> {
        let query: String = raw_query.trim().chars().take(MAX_SEARCH_QUERY_CHARS).collect();
        if query.chars().count() < 2 {
            call.reply("Kullanım: !ara <şarkı veya sanatçı>");
            return Ok(());
        }
        let key = (call.bot_id, call.clid);
        if !self.tracks.has_cookies() {
            self.sessions.lock().expect("sessions lock").remove(&key);
            call.reply("YouTube çerezleri gerekli. Yönetici, panelde Kütüphane → YouTube erişimi bölümünden cookies.txt yüklemeli.");
            return Ok(());
        }
        call.reply(&if self.tracks.has_spotify() { format!("YouTube ve Spotify'da aranıyor: {query}") } else { format!("YouTube'da aranıyor: {query}") });
        let results = self.tracks.search(query.clone()).await;
        if results.is_empty() {
            call.reply(&format!("“{query}” için sonuç bulunamadı."));
            return Ok(());
        }
        call.reply(&format_results(&query, &results));
        self.sessions.lock().expect("sessions lock").insert(key, SearchSession { results, expires_at: Instant::now() + SEARCH_SESSION });
        Ok(())
    }

    /// A number with no open search is an ordinary chat line.
    async fn search_choice(self: &Arc<Self>, call: &Call, choice: usize) -> Result<()> {
        let key = (call.bot_id, call.clid);
        let selected = {
            let mut sessions = self.sessions.lock().expect("sessions lock");
            let Some(session) = sessions.get(&key) else { return Ok(()) };
            if session.expires_at <= Instant::now() {
                sessions.remove(&key);
                drop(sessions);
                call.reply("Arama süresi doldu. Tekrar !ara yazın.");
                return Ok(());
            }
            let Some(selected) = session.results.get(choice - 1).cloned() else {
                drop(sessions);
                call.reply("Geçersiz seçim. Listede görünen bir numarayı yazın.");
                return Ok(());
            };
            sessions.remove(&key);
            selected
        };
        call.reply(&format!("Hazırlanıyor: {} - {}", selected.artist, selected.title));
        let request = self.begin_request(call.bot_id);
        if let Err(error) = self.enqueue_media_url(call, &selected.url, request).await {
            // Downloader paths, proxy credentials and cookie locations stay out of chat.
            let lower = error.to_lowercase();
            let blocked = lower.contains("sign in to confirm") || lower.contains("not a bot") || lower.contains("bot check");
            call.reply(if blocked {
                "Çalınamadı: YouTube sunucu bağlantısında doğrulama istiyor. Arama sonucu bulunması, sesin indirilebildiği anlamına gelmez. Yönetici Medya tanılama bölümünü kontrol etmeli."
            } else {
                "Şarkı hazırlanamadı. Yönetici Medya tanılama bölümünü ve bot kayıtlarını kontrol etmeli. Tekrar !ara ile deneyebilirsiniz."
            });
        }
        Ok(())
    }

    // ─── History ───────────────────────────────────────────────

    async fn history(&self, call: &Call, args: &str) -> Result<()> {
        if !args.is_empty() {
            call.reply("Usage: !history");
            return Ok(());
        }
        let rows = self.db.music_requests(call.entry.server_config_id, call.bot_id, 10).await.map_err(stored_error)?;
        if rows.is_empty() {
            call.reply("No request history for this bot yet.");
            return Ok(());
        }
        let mut lines = vec!["Recent requests:".to_owned()];
        for row in rows {
            let at = row.requested_at.0.with_timezone(&Utc).format("%Y-%m-%d %H:%M");
            let gone = if row.status == "unavailable" { " (unavailable)" } else { "" };
            lines.push(format!("#{} {} — {} — {at} UTC{gone}", row.id, format::history_text(&row.title, 70), format::history_text(&row.source, 20)));
        }
        lines.push("Use !again <id>, or !again for the latest request.".to_owned());
        call.reply(&lines.join("\n"));
        Ok(())
    }

    async fn replay_reference(&self, url: &str, source: &str) -> Result<Replay> {
        if source == "local" {
            return Ok(resolve_under_music_dir(&self.music_dir, url).map_or(Replay::Gone, Replay::Local));
        }
        if !is_remote_input(url) {
            return Ok(Replay::Gone);
        }
        if let Err(error) = validate_url(url).await {
            if error.0.ends_with("could not be resolved") {
                return Err("Media source could not be resolved. Try again.".to_owned());
            }
            return Ok(Replay::Gone);
        }
        Ok(if source == "radio" { Replay::Radio(url.to_owned()) } else { Replay::Url(url.to_owned()) })
    }

    async fn again(self: &Arc<Self>, call: &Call, args: &str) -> Result<()> {
        let id = if args.is_empty() {
            None
        } else {
            match args.parse::<i64>().ok().filter(|id| *id > 0 && !args.starts_with('+')) {
                Some(id) => Some(id),
                None => {
                    call.reply("Usage: !again [id]");
                    return Ok(());
                }
            }
        };
        let request = self.begin_request(call.bot_id);
        let server = call.entry.server_config_id;
        let row = match id {
            Some(id) => self.db.music_request(server, call.bot_id, id).await.map_err(stored_error)?,
            None => self.db.music_requests(server, call.bot_id, 1).await.map_err(stored_error)?.into_iter().next(),
        };
        if request.is_cancelled() {
            return Ok(());
        }
        let Some(row) = row else {
            call.reply("Request not found. Use !history to list requests.");
            return Ok(());
        };
        let unavailable = |service: Arc<Self>, id: i64| async move {
            if let Err(error) = service.db.mark_music_request_unavailable(id).await {
                tracing::warn!(%error, "Could not mark a request unavailable");
            }
        };
        let reference = self.replay_reference(&row.url, &row.source).await?;
        if request.is_cancelled() {
            return Ok(());
        }
        let local_or_radio = |file_path: String, stream_url: Option<String>, source: Source| QueueItem {
            id: format!("history_{}", row.id),
            title: row.title.clone(),
            artist: row.artist.clone(),
            duration: row.duration,
            source,
            file_path,
            source_url: None,
            stream_url,
        };
        let item = match reference {
            Replay::Gone => {
                unavailable(self.clone(), row.id).await;
                call.reply(MEDIA_UNAVAILABLE);
                return Ok(());
            }
            Replay::Url(url) => {
                if let Err(error) = self.enqueue_media_url(call, &url, request.clone()).await {
                    if request.is_cancelled() {
                        return Ok(());
                    }
                    if !is_unavailable_error(&error) {
                        return Err(error);
                    }
                    unavailable(self.clone(), row.id).await;
                    call.reply(MEDIA_UNAVAILABLE);
                }
                return Ok(());
            }
            Replay::Local(path) => local_or_radio(path.display().to_string(), None, Source::Local),
            Replay::Radio(url) => local_or_radio(String::new(), Some(url), Source::Radio),
        };
        self.join_for_command(call);
        let queued = self.start_or_append(call, item).await?;
        let verb = if queued { "Queued" } else { "Now playing" };
        call.reply(&format!("{verb}: {}", format::history_text(&row.title, 70)));
        Ok(())
    }

    // ─── Queue ─────────────────────────────────────────────────

    async fn playlist(&self, call: &Call, args: &str) -> Result<()> {
        let server = call.entry.server_config_id;
        let playlists = self.db.playlists(server).await.map_err(stored_error)?;
        let short = |name: &str| name.chars().take(60).collect::<String>();
        let query = args.trim().to_lowercase();
        if query.is_empty() {
            if playlists.is_empty() {
                call.reply("No playlists configured for this bot/server.");
            } else {
                let lines: Vec<String> = playlists.iter().take(10).map(|playlist| format!("[{}] {}", playlist.id, short(&playlist.name))).collect();
                call.reply(&format!("Playlists (first 10):\n{}", lines.join("\n")));
            }
            return Ok(());
        }
        let by_id: Vec<_> = query.parse::<i64>().ok().map(|id| playlists.iter().filter(|playlist| playlist.id == id).collect()).unwrap_or_default();
        let exact: Vec<_> = playlists.iter().filter(|playlist| playlist.name.to_lowercase() == query).collect();
        let matches: Vec<_> = if !by_id.is_empty() {
            by_id
        } else if !exact.is_empty() {
            exact
        } else {
            playlists.iter().filter(|playlist| playlist.name.to_lowercase().contains(&query)).collect()
        };
        let [playlist] = matches.as_slice() else {
            if matches.is_empty() {
                call.reply("Playlist not found. Use !playlist to list.");
            } else {
                let names: Vec<String> = matches.iter().take(5).map(|playlist| format!("[{}] {}", playlist.id, short(&playlist.name))).collect();
                call.reply(&format!("Ambiguous playlist; use an ID: {}", names.join(", ")));
            }
            return Ok(());
        };
        let songs = self.db.playlist_songs(server, playlist.id).await.map_err(stored_error)?;
        if songs.is_empty() {
            call.reply("Playlist is empty.");
            return Ok(());
        }
        let items: Vec<QueueItem> = songs
            .into_iter()
            .map(|song| QueueItem {
                id: song.id.to_string(),
                title: song.title,
                artist: song.artist,
                duration: song.duration,
                source: Source::from_stored(&song.source),
                file_path: song.file_path,
                source_url: song.source_url,
                stream_url: None,
            })
            .collect();
        let count = items.len();
        let first = {
            let queue = call.entry.bot.queue();
            let mut queue = queue.lock().expect("queue lock");
            if let Err(error) = queue.add_many(items.clone()) {
                call.reply(&error.to_string());
                return Ok(());
            }
            let idle = call.entry.bot.snapshot().status == Status::Connected;
            idle.then(|| {
                let position = queue.index().map_or(0, |index| index + 1);
                queue.play_at(position).cloned()
            })
            .flatten()
        };
        self.save_requests(&call.entry, call.bot_id, &items);
        if let Some(first) = first {
            call.entry.bot.play_item(first);
            call.entry.bot.settle().await;
        }
        call.reply(&format!("Queued playlist \"{}\" ({count} tracks).", short(&playlist.name)));
        Ok(())
    }

    fn repeat(&self, call: &Call, args: &str) {
        let queue = call.entry.bot.queue();
        let mut queue = queue.lock().expect("queue lock");
        match args.trim().to_lowercase().as_str() {
            "" => {}
            "off" => queue.set_repeat(RepeatMode::Off),
            "track" => queue.set_repeat(RepeatMode::Track),
            "queue" => queue.set_repeat(RepeatMode::Queue),
            _ => {
                call.reply("Usage: !repeat [off|track|queue]");
                return;
            }
        }
        let mode = match queue.repeat() {
            RepeatMode::Off => "off",
            RepeatMode::Track => "track",
            RepeatMode::Queue => "queue",
        };
        call.reply(&format!("Repeat mode: {mode}"));
    }

    fn remove(&self, call: &Call, args: &str) {
        let query = args.trim().to_lowercase();
        if query.is_empty() {
            call.reply("Usage: !remove <text>");
            return;
        }
        let queue = call.entry.bot.queue();
        let mut queue = queue.lock().expect("queue lock");
        let start = queue.index().map_or(0, |index| index + 1);
        let upcoming: Vec<(usize, QueueItem)> = queue.get_all().into_iter().cloned().enumerate().filter(|(index, _)| *index >= start).collect();
        let exact: Vec<&(usize, QueueItem)> = upcoming
            .iter()
            .filter(|(_, item)| item.title.to_lowercase() == query || item.artist.as_deref().is_some_and(|artist| artist.to_lowercase() == query))
            .collect();
        let matches: Vec<&(usize, QueueItem)> = if exact.is_empty() {
            upcoming
                .iter()
                .filter(|(_, item)| item.title.to_lowercase().contains(&query) || item.artist.as_deref().is_some_and(|artist| artist.to_lowercase().contains(&query)))
                .collect()
        } else {
            exact
        };
        let [(index, item)] = matches.as_slice() else {
            if matches.is_empty() {
                call.reply("No upcoming track matches.");
            } else {
                let names: Vec<String> = matches
                    .iter()
                    .take(5)
                    .map(|(index, item)| format!("#{} {}", queue.pruned() + index + 1, item.title.chars().take(60).collect::<String>()))
                    .collect();
                call.reply(&format!("Multiple matches; use !queue remove <n>: {}", names.join(", ")));
            }
            return;
        };
        queue.remove_at(*index);
        call.reply(&format!("Removed: {}", item.title.chars().take(100).collect::<String>()));
    }

    fn show_queue(&self, call: &Call) {
        let queue = call.entry.bot.queue();
        let queue = queue.lock().expect("queue lock");
        let lines: Vec<TrackLine> = queue.get_all().into_iter().map(|item| TrackLine { title: item.title.clone(), artist: item.artist.clone(), duration: item.duration }).collect();
        // Around the playing track: the kept history would otherwise fill the list.
        let start = queue.index().map_or(0, |index| index.saturating_sub(2)).min(lines.len().saturating_sub(QUEUE_LINES));
        call.reply(&format::queue_message(&lines, queue.index(), QUEUE_LINES, start, queue.pruned()));
    }

    /// Chat numbers count from the first track kept since the last clear, so pruning never
    /// renumbers what is still queued.
    fn position_of_number(queue: &PlayQueue<QueueItem>, raw: &str) -> Option<usize> {
        let value = raw.trim();
        if value.is_empty() || value.starts_with('0') || !value.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        value.parse::<usize>().ok()?.checked_sub(1)?.checked_sub(queue.pruned())
    }

    fn invalid_number(queue: &PlayQueue<QueueItem>) -> String {
        if queue.is_empty() {
            return "Invalid index. The queue is empty.".to_owned();
        }
        format!("Invalid index. Use a track number from {} to {} (see !queue).", queue.pruned() + 1, queue.pruned() + queue.len())
    }

    async fn queue_command(self: &Arc<Self>, call: &Call, args: &str) -> Result<()> {
        let lower = args.to_lowercase();
        if args.is_empty() || lower == "show" {
            self.show_queue(call);
            return Ok(());
        }
        let queue = call.entry.bot.queue();
        if lower.starts_with("remove ") {
            let mut queue = queue.lock().expect("queue lock");
            let Some(position) = Self::position_of_number(&queue, &args[7..]).filter(|position| *position < queue.len()) else {
                call.reply(&Self::invalid_number(&queue));
                return Ok(());
            };
            let title = queue.get_all()[position].title.clone();
            let number = queue.pruned() + position + 1;
            queue.remove_at(position);
            call.reply(&format!("Removed #{number}: {title}"));
            return Ok(());
        }
        if lower.starts_with("play ") {
            let found = {
                let mut queue = queue.lock().expect("queue lock");
                let position = Self::position_of_number(&queue, &args[5..]);
                let item = position.and_then(|position| queue.play_at(position).cloned());
                match item {
                    Some(item) => Ok((item, queue.pruned() + position.unwrap_or(0) + 1)),
                    None => Err(Self::invalid_number(&queue)),
                }
            };
            match found {
                Ok((item, number)) => {
                    let title = item.title.clone();
                    call.entry.bot.play_item(item);
                    call.entry.bot.settle().await;
                    call.reply(&format!("Playing #{number}: {title}"));
                }
                Err(message) => call.reply(&message),
            }
            return Ok(());
        }
        if lower == "clear" {
            self.cancel_requests(call.bot_id);
            queue.lock().expect("queue lock").clear();
            call.entry.bot.stop_audio();
            call.entry.bot.settle().await;
            call.reply("Queue cleared.");
            return Ok(());
        }
        if !args.starts_with("http://") && !args.starts_with("https://") {
            call.reply("Usage: !queue [show|play <n>|remove <n>|clear|<url>]");
            return Ok(());
        }
        call.reply("Loading...");
        let request = self.begin_request(call.bot_id);
        if let Err(error) = self.enqueue_media_url(call, args, request.clone()).await {
            if !request.is_cancelled() {
                call.reply(&format!("Failed to queue: {error}"));
            }
        }
        Ok(())
    }

    async fn shuffle(&self, call: &Call, args: &str) {
        let queue = call.entry.bot.queue();
        let enabled = match args.trim().to_lowercase().as_str() {
            "" => !queue.lock().expect("queue lock").shuffle(),
            "on" | "1" | "true" | "yes" => true,
            "off" | "0" | "false" | "no" => false,
            _ => {
                call.reply("Usage: !shuffle [on|off]");
                return;
            }
        };
        queue.lock().expect("queue lock").set_shuffle(enabled);
        call.reply(if enabled { "Shuffle on." } else { "Shuffle off." });
    }

    // ─── Auto-DJ ───────────────────────────────────────────────

    /// Run a bot's Auto-DJ until it disconnects: on every event, and once a second for timers.
    pub fn run_automation(self: &Arc<Self>, bot_id: i64, entry: BotEntry, mut events: broadcast::Receiver<BotEvent>) {
        let Some(automation) = entry.automation.clone() else { return };
        let service = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(1));
            loop {
                let now = Utc::now().timestamp_millis();
                tokio::select! {
                    _ = tick.tick() => {}
                    event = events.recv() => {
                        let mut controller = automation.lock().expect("automation lock");
                        match event {
                            Ok(BotEvent::RequestStarted) => controller.request_queued(),
                            Ok(BotEvent::ManualStop) => controller.manual_stop(now),
                            Ok(BotEvent::AutomationEnded { failed }) => controller.source_ended(failed, now),
                            Ok(BotEvent::Disconnected) | Err(broadcast::error::RecvError::Closed) => {
                                controller.disconnected();
                                break;
                            }
                            Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                        }
                    }
                }
                service.automation_step(bot_id, &entry, &automation).await;
            }
        });
    }

    fn automation_snapshot(&self, entry: &BotEntry) -> AutomationSnapshot {
        let state = entry.bot.snapshot();
        AutomationSnapshot {
            connected: state.status != Status::Stopped && entry.bot.current_channel() > 0,
            busy: state.now_playing.is_some() || state.status == Status::Paused,
            has_requests: entry.bot.queue().lock().expect("queue lock").upcoming_count() > 0,
            human_listeners: self.human_listeners(entry),
        }
    }

    async fn automation_step(&self, bot_id: i64, entry: &BotEntry, automation: &Arc<Mutex<AutomationController>>) {
        let now = Utc::now().timestamp_millis();
        let snapshot = self.automation_snapshot(entry);
        let step = {
            let mut controller = automation.lock().expect("automation lock");
            controller.remember_channel(entry.bot.current_channel() as i64);
            controller.reconcile(snapshot, now)
        };
        if let Step::Load { token, source, last_track_id } = step {
            let loaded = self.load_automation_source(entry.server_config_id, source, last_track_id.as_deref()).await;
            let now = Utc::now().timestamp_millis();
            let snapshot = self.automation_snapshot(entry);
            match loaded {
                Ok(item) => {
                    let play = automation.lock().expect("automation lock").loaded(token, &item.id, snapshot, now);
                    if play {
                        entry.bot.play_automation(item);
                        entry.bot.settle().await;
                    }
                }
                Err(error) => {
                    tracing::warn!(error, bot_id, "Auto-DJ source failed");
                    automation.lock().expect("automation lock").load_failed(token, now);
                }
            }
        }
        let (stop, state) = {
            let mut controller = automation.lock().expect("automation lock");
            (controller.take_stop(), controller.take_state_to_save())
        };
        if stop {
            entry.bot.clear_automation();
            entry.bot.settle().await;
        }
        if let Some(state) = state {
            if let Err(error) = self.db.set_automation_state(bot_id, &state).await {
                tracing::warn!(%error, bot_id, "Auto-DJ state could not be saved");
            }
        }
    }

    /// The station, or the playlist's track after the last one played.
    async fn load_automation_source(&self, server: i64, source: AutomationTarget, last_track_id: Option<&str>) -> Result<QueueItem> {
        if source.kind == AutomationSource::Radio {
            let station = self.db.radio_station(server, source.id).await.map_err(stored_error)?.ok_or("Auto-DJ radio station no longer exists")?;
            return Ok(QueueItem {
                id: format!("radio_{}", station.id),
                title: station.name,
                artist: Some(station.genre.unwrap_or_else(|| "Radio".to_owned())),
                duration: None,
                source: Source::Radio,
                file_path: String::new(),
                source_url: None,
                stream_url: Some(station.url),
            });
        }
        let songs = self.db.automation_playlist_songs(server, source.id).await.map_err(stored_error)?;
        if songs.is_empty() {
            return Err("Auto-DJ playlist is missing or empty".to_owned());
        }
        let previous = songs.iter().position(|song| Some(song.id.to_string().as_str()) == last_track_id);
        let song = songs.into_iter().cycle().nth(previous.map_or(0, |index| index + 1)).expect("not empty");
        Ok(QueueItem {
            id: song.id.to_string(),
            title: song.title,
            artist: song.artist,
            duration: song.duration,
            source: Source::from_stored(&song.source),
            file_path: song.file_path,
            source_url: song.source_url,
            stream_url: None,
        })
    }
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
