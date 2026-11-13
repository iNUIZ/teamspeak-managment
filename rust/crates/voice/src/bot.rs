//! One voice bot: a task that owns the TeamSpeak connection, the queue and the voice clock.
//! Slow work (yt-dlp, opening ffmpeg, ICY) runs in other tasks, so it never delays a frame.

use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use teamhub_media::cache::MediaCache;
use teamhub_media::pcm::is_remote_input;
use teamhub_media::youtube::{cached_video, download, is_youtube_url, resolve_audio_stream, YtDlpOptions};
use teamhub_media::{icy, OpusEncoder, PcmEvent, PcmStream, FRAME_MS};
use teamhub_ts_proto::commands::{build_command, Params};
use teamhub_ts_proto::{ClientOptions, Event, Identity, TsClient};
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio::task::JoinHandle;

use crate::clock::FrameClock;
use crate::nickname::now_playing_nickname;
use crate::queue::{PlayQueue, RepeatMode};

const ICY_POLL: Duration = Duration::from_secs(15);
const DISCONNECT_WAIT: Duration = Duration::from_secs(1);
/// How often the published position moves, in frames.
const PUBLISH_EVERY_FRAMES: u64 = 25;
/// TeamSpeak's "client is flooding". Every action would be refused and extend the block.
const FLOOD_ERROR_ID: &str = "524";
/// How long chat stays quiet after a 524; each further 524 restarts it.
const FLOOD_HOLD: Duration = Duration::from_secs(30);
const EVENT_BUFFER: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Stopped,
    Connected,
    Playing,
    Paused,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Connected => "connected",
            Self::Playing => "playing",
            Self::Paused => "paused",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Local,
    Youtube,
    Url,
    Radio,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Youtube => "youtube",
            Self::Url => "url",
            Self::Radio => "radio",
        }
    }

    pub fn from_stored(text: &str) -> Self {
        match text {
            "local" => Self::Local,
            "youtube" => Self::Youtube,
            "radio" => Self::Radio,
            _ => Self::Url,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueueItem {
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    /// Seconds.
    pub duration: Option<f64>,
    pub source: Source,
    pub file_path: String,
    pub source_url: Option<String>,
    /// Set for a live stream (radio) instead of a file.
    pub stream_url: Option<String>,
}

pub struct BotConfig {
    pub host: String,
    pub port: u16,
    pub identity: Identity,
    pub nickname: String,
    pub server_password: Option<String>,
    pub default_channel: Option<String>,
    pub channel_password: Option<String>,
    pub volume: u8,
    pub media: Media,
    /// Kept by whoever owns the bot, so it survives a reconnect.
    pub queue: Arc<Mutex<PlayQueue<QueueItem>>>,
}

/// What the rest of the program can see of a bot.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub status: Status,
    pub now_playing: Option<QueueItem>,
    pub position_seconds: f64,
    pub volume: u8,
    pub queue_length: usize,
    pub frames_sent: u64,
    pub last_error: Option<String>,
    /// A file or resolved track is open, so `seek` can work.
    pub can_seek: bool,
    /// Changes with every started track, so a vote or a claim can tell tracks apart.
    pub playback_token: u64,
}

/// What other parts of the program react to.
#[derive(Debug, Clone, PartialEq)]
pub enum BotEvent {
    Text(Params),
    /// A request started, as opposed to Auto-DJ.
    RequestStarted,
    /// Someone stopped the audio on purpose.
    ManualStop,
    /// Auto-DJ's own track ended.
    AutomationEnded { failed: bool },
    /// A download finished, so the song row can remember the file.
    MediaCached { song_id: String, file_path: String },
    Disconnected,
}

enum Command {
    Play(QueueItem),
    PlayItem { item: QueueItem, automation: bool },
    Enqueue(QueueItem),
    Pause,
    Resume,
    Seek(f64),
    Skip,
    Previous,
    StopAudio,
    /// Stop without counting as a manual stop: Auto-DJ giving way.
    ClearAutomation,
    SetVolume(u8),
    SetRepeat(RepeatMode),
    SetShuffle(bool),
    JoinChannel(u64),
    /// Answered when every earlier command has been handled.
    Barrier(oneshot::Sender<()>),
    Disconnect,
}

#[derive(Default)]
struct FloodHold {
    until: Mutex<Option<Instant>>,
    ignored: AtomicU32,
}

impl FloodHold {
    fn active(&self) -> bool {
        self.until.lock().expect("flood lock").is_some_and(|until| Instant::now() < until)
    }
}

/// The handle to a running bot. Dropping it disconnects the bot.
pub struct VoiceBot {
    commands: mpsc::UnboundedSender<Command>,
    state: watch::Receiver<Snapshot>,
    events: broadcast::Sender<BotEvent>,
    client: Arc<TsClient>,
    queue: Arc<Mutex<PlayQueue<QueueItem>>>,
    flood: Arc<FloodHold>,
    task: Mutex<Option<JoinHandle<()>>>,
}

impl VoiceBot {
    /// Connect, then run until disconnected.
    pub async fn start(config: BotConfig) -> teamhub_ts_proto::Result<Self> {
        let BotConfig { host, port, identity, nickname, server_password, default_channel, channel_password, volume, media, queue } =
            config;
        let (client, events) = TsClient::connect(ClientOptions {
            host,
            port,
            identity,
            nickname: nickname.clone(),
            server_password,
            default_channel,
            channel_password,
        })
        .await?;
        let encoder = OpusEncoder::new().map_err(|error| teamhub_ts_proto::Error(error.to_string()))?;

        let volume = volume.min(100);
        let (commands, command_rx) = mpsc::unbounded_channel();
        let (state_tx, state) = watch::channel(Snapshot {
            status: Status::Connected,
            now_playing: None,
            position_seconds: 0.0,
            volume,
            queue_length: 0,
            frames_sent: 0,
            last_error: None,
            can_seek: false,
            playback_token: 0,
        });
        let (internal_tx, internal_rx) = mpsc::unbounded_channel();
        let client = Arc::new(client);
        let flood = Arc::new(FloodHold::default());
        let (events_tx, _) = broadcast::channel(EVENT_BUFFER);
        let worker = Worker {
            client: client.clone(),
            encoder,
            nickname,
            volume,
            media,
            status: Status::Connected,
            queue: queue.clone(),
            flood: flood.clone(),
            flood_end: None,
            events: events_tx.clone(),
            automation_owned: false,
            now_playing: None,
            playback: None,
            preparing: None,
            icy: None,
            generation: 0,
            last_title: String::new(),
            last_error: None,
            state: state_tx,
            internal: internal_tx,
        };
        let task = tokio::spawn(worker.run(command_rx, events, internal_rx));
        Ok(Self { commands, state, events: events_tx, client, queue, flood, task: Mutex::new(Some(task)) })
    }

    fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    /// Queue a track and start it now.
    pub fn play(&self, item: QueueItem) {
        self.send(Command::Play(item));
    }

    /// Start a track that is already in the queue, or not meant for it.
    pub fn play_item(&self, item: QueueItem) {
        self.send(Command::PlayItem { item, automation: false });
    }

    /// Start Auto-DJ's track: it does not touch the queue and gives way to any request.
    pub fn play_automation(&self, item: QueueItem) {
        self.send(Command::PlayItem { item, automation: true });
    }

    pub fn clear_automation(&self) {
        self.send(Command::ClearAutomation);
    }

    /// Wait until every command sent so far has been handled, so the snapshot shows its result.
    pub async fn settle(&self) {
        let (sender, receiver) = oneshot::channel();
        self.send(Command::Barrier(sender));
        let _ = receiver.await;
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<BotEvent> {
        self.events.subscribe()
    }

    pub fn queue(&self) -> Arc<Mutex<PlayQueue<QueueItem>>> {
        self.queue.clone()
    }

    pub fn client(&self) -> &TsClient {
        &self.client
    }

    pub fn flood_hold_active(&self) -> bool {
        self.flood.active()
    }

    /// A command arrived during the flood hold; the bot says so once afterwards.
    pub fn note_ignored_command(&self) {
        self.flood.ignored.fetch_add(1, Ordering::SeqCst);
    }

    /// Text to the bot's current channel. Dropped during a flood hold.
    pub fn send_channel_message(&self, message: &str) {
        if !self.flood.active() {
            self.client.send_text(message, 2, None);
        }
    }

    /// A private message. Dropped during a flood hold.
    pub fn send_private_message(&self, client_id: u64, message: &str) {
        if !self.flood.active() {
            self.client.send_text(message, 1, Some(client_id));
        }
    }

    /// Queue a track; it starts by itself when the bot is idle.
    pub fn enqueue(&self, item: QueueItem) {
        self.send(Command::Enqueue(item));
    }

    pub fn pause(&self) {
        self.send(Command::Pause);
    }

    pub fn resume(&self) {
        self.send(Command::Resume);
    }

    pub fn seek(&self, seconds: f64) {
        self.send(Command::Seek(seconds));
    }

    pub fn skip(&self) {
        self.send(Command::Skip);
    }

    pub fn previous(&self) {
        self.send(Command::Previous);
    }

    pub fn stop_audio(&self) {
        self.send(Command::StopAudio);
    }

    pub fn set_volume(&self, volume: u8) {
        self.send(Command::SetVolume(volume));
    }

    pub fn set_repeat(&self, mode: RepeatMode) {
        self.send(Command::SetRepeat(mode));
    }

    pub fn set_shuffle(&self, enabled: bool) {
        self.send(Command::SetShuffle(enabled));
    }

    pub fn join_channel(&self, channel: u64) {
        self.send(Command::JoinChannel(channel));
    }

    pub fn snapshot(&self) -> Snapshot {
        self.state.borrow().clone()
    }

    /// A receiver that wakes on each change, for the API and WebSocket.
    pub fn subscribe(&self) -> watch::Receiver<Snapshot> {
        self.state.clone()
    }

    /// Leave the server and wait until the bot has stopped.
    pub async fn disconnect(&self) {
        self.send(Command::Disconnect);
        let task = self.task.lock().expect("task lock").take();
        if let Some(task) = task {
            let _ = task.await;
        }
    }
}

enum PlaybackSource {
    /// A local file or a resolved track URL: seekable, pausable, has an end.
    File { input: String, base_seconds: f64 },
    Live,
}

struct Playback {
    source: PlaybackSource,
    pcm: PcmStream,
    clock: FrameClock,
    frames_sent: u64,
}

impl Playback {
    fn position(&self) -> f64 {
        let played = self.frames_sent as f64 * FRAME_MS as f64 / 1000.0;
        match self.source {
            PlaybackSource::File { base_seconds, .. } => base_seconds + played,
            PlaybackSource::Live => played,
        }
    }
}

struct Opened {
    source: PlaybackSource,
    pcm: PcmStream,
    duration: Option<f64>,
    /// The file a download produced, so the track need not be fetched again.
    cached_file: Option<String>,
}

/// What opening a track needs from the program.
#[derive(Clone)]
pub struct Media {
    pub cache: Arc<MediaCache>,
    pub yt_dlp: YtDlpOptions,
}

enum Internal {
    Opened { generation: u64, result: Result<Opened, String> },
    IcyTitle { generation: u64, title: String },
}

struct Worker {
    client: Arc<TsClient>,
    encoder: OpusEncoder,
    nickname: String,
    volume: u8,
    media: Media,
    status: Status,
    queue: Arc<Mutex<PlayQueue<QueueItem>>>,
    flood: Arc<FloodHold>,
    flood_end: Option<Instant>,
    events: broadcast::Sender<BotEvent>,
    /// The current track is Auto-DJ's, so its end does not advance the queue.
    automation_owned: bool,
    now_playing: Option<QueueItem>,
    playback: Option<Playback>,
    preparing: Option<JoinHandle<()>>,
    icy: Option<JoinHandle<()>>,
    /// Bumped by every start and stop, so late results of older requests are recognised.
    generation: u64,
    last_title: String,
    last_error: Option<String>,
    state: watch::Sender<Snapshot>,
    internal: mpsc::UnboundedSender<Internal>,
}

async fn open_file(input: String, start_seconds: f64, duration: Option<f64>, cached_file: Option<String>) -> Result<Opened, String> {
    let pcm = PcmStream::open_file(&input, start_seconds).await.map_err(|error| error.to_string())?;
    Ok(Opened { source: PlaybackSource::File { input, base_seconds: start_seconds }, pcm, duration, cached_file })
}

/// Download the track and open the file.
async fn open_download(url: String, media: Media, start_seconds: f64) -> Result<Opened, String> {
    let track = download(&url, &media.cache, &media.yt_dlp).await.map_err(|error| error.to_string())?;
    let path = track.file_path.display().to_string();
    let duration = (track.duration > 0.0).then_some(track.duration);
    open_file(path.clone(), start_seconds, duration, Some(path)).await
}

/// How a request finds its audio. Runs in its own task; aborting it stops yt-dlp and ffmpeg.
///
/// A YouTube track plays from the cache, else streams, else is downloaded. Streaming is tried
/// first because it starts sooner; YouTube often refuses the resolved address, which
/// [`Worker::source_ended`] answers with a download.
async fn open_item(item: QueueItem, media: Media) -> Result<Opened, String> {
    if let Some(url) = &item.stream_url {
        let pcm = PcmStream::open_live(url).await.map_err(|error| error.to_string())?;
        return Ok(Opened { source: PlaybackSource::Live, pcm, duration: None, cached_file: None });
    }
    if !item.file_path.is_empty() && Path::new(&item.file_path).exists() {
        return open_file(item.file_path, 0.0, None, None).await;
    }
    let Some(url) = item.source_url.filter(|url| is_remote_input(url)) else {
        return Err(format!("No media file for \"{}\" and no source URL to download", item.title));
    };
    if is_youtube_url(&url) {
        if let Some(path) = cached_video(&url, &media.cache).await {
            let path = path.display().to_string();
            return open_file(path.clone(), 0.0, None, Some(path)).await;
        }
        match resolve_audio_stream(&url, &media.yt_dlp).await {
            Ok(resolved) => {
                let duration = (resolved.duration > 0.0).then_some(resolved.duration);
                return open_file(resolved.stream_url, 0.0, duration, None).await;
            }
            Err(error) => tracing::warn!(%error, "Streaming failed, downloading instead"),
        }
    }
    open_download(url, media, 0.0).await
}

impl Worker {
    async fn run(
        mut self,
        mut commands: mpsc::UnboundedReceiver<Command>,
        mut events: mpsc::UnboundedReceiver<Event>,
        mut internal: mpsc::UnboundedReceiver<Internal>,
    ) {
        loop {
            let due = match (&self.playback, self.status) {
                (Some(playback), Status::Playing) => Some(playback.clock.next_due()),
                _ => None,
            };
            let flood_end = self.flood_end;
            tokio::select! {
                // The clock comes first: a burst of commands must not delay a frame.
                biased;
                _ = async { tokio::time::sleep_until(due.expect("checked").into()).await }, if due.is_some() => {
                    self.tick();
                }
                _ = async { tokio::time::sleep_until(flood_end.expect("checked").into()).await }, if flood_end.is_some() => {
                    self.end_flood_hold();
                }
                event = events.recv() => match event {
                    Some(Event::Disconnected) | None => break,
                    Some(Event::Error(message)) => self.note_error(message),
                    Some(Event::ServerError(params)) => {
                        if params.get("id").map(String::as_str) == Some(FLOOD_ERROR_ID) {
                            self.start_flood_hold();
                        }
                    }
                    Some(Event::TextMessage(params)) => self.emit(BotEvent::Text(params)),
                    Some(_) => {}
                },
                message = internal.recv() => {
                    if let Some(message) = message {
                        self.handle_internal(message);
                    }
                }
                command = commands.recv() => match command {
                    Some(Command::Disconnect) | None => {
                        self.leave(&mut events).await;
                        break;
                    }
                    Some(command) => self.handle_command(command),
                },
            }
        }
        self.stop_tasks();
        self.playback = None;
        self.now_playing = None;
        self.status = Status::Stopped;
        self.publish();
        self.emit(BotEvent::Disconnected);
    }

    fn emit(&self, event: BotEvent) {
        // No receiver is fine: nothing is listening yet.
        let _ = self.events.send(event);
    }

    fn start_flood_hold(&mut self) {
        tracing::warn!("TeamSpeak reported flooding (524); holding chat for 30 s");
        let until = Instant::now() + FLOOD_HOLD;
        *self.flood.until.lock().expect("flood lock") = Some(until);
        self.flood_end = Some(until);
    }

    fn end_flood_hold(&mut self) {
        self.flood_end = None;
        *self.flood.until.lock().expect("flood lock") = None;
        if self.flood.ignored.swap(0, Ordering::SeqCst) > 0 {
            self.client.send_text("Commands came in too fast for the TeamSpeak server and were ignored. You can try again now.", 2, None);
        }
    }

    fn queue(&self) -> std::sync::MutexGuard<'_, PlayQueue<QueueItem>> {
        self.queue.lock().expect("queue lock")
    }

    fn handle_command(&mut self, command: Command) {
        match command {
            Command::Play(item) => {
                let added = {
                    let mut queue = self.queue();
                    let added = queue.add(item.clone());
                    if added.is_ok() {
                        let last = queue.len() - 1;
                        queue.play_at(last);
                    }
                    added
                };
                match added {
                    Ok(()) => self.begin(item, false),
                    Err(error) => self.note_error(error.to_string()),
                }
            }
            Command::PlayItem { item, automation } => self.begin(item, automation),
            Command::Enqueue(item) => {
                let added = self.queue().add(item);
                if let Err(error) = added {
                    self.note_error(error.to_string());
                    return;
                }
                if self.status == Status::Connected {
                    self.play_next();
                } else {
                    self.publish();
                }
            }
            Command::Pause => self.pause(),
            Command::Resume => self.resume(),
            Command::Seek(seconds) => self.seek(seconds),
            Command::Skip => self.play_next(),
            Command::Previous => {
                let previous = self.queue().previous().cloned();
                match previous {
                    Some(item) => self.begin(item, false),
                    None => self.end_audio(),
                }
            }
            Command::StopAudio => {
                self.emit(BotEvent::ManualStop);
                self.end_audio();
            }
            Command::ClearAutomation => self.end_audio(),
            Command::SetVolume(volume) => {
                self.volume = volume.min(100);
                self.publish();
            }
            Command::SetRepeat(mode) => self.queue().set_repeat(mode),
            Command::SetShuffle(enabled) => {
                self.queue().set_shuffle(enabled);
                self.publish();
            }
            Command::JoinChannel(channel) => self.client.move_to_channel(channel),
            Command::Barrier(sender) => {
                self.publish();
                let _ = sender.send(());
            }
            Command::Disconnect => unreachable!("handled by the main loop"),
        }
    }

    fn handle_internal(&mut self, message: Internal) {
        match message {
            Internal::Opened { generation, result } => {
                if generation != self.generation {
                    return; // A newer request took over; dropping the stream kills its ffmpeg.
                }
                self.preparing = None;
                match result {
                    Ok(opened) => {
                        if let Some(item) = &mut self.now_playing {
                            if let Some(duration) = opened.duration {
                                item.duration.get_or_insert(duration);
                            }
                            if let Some(path) = opened.cached_file {
                                item.file_path.clone_from(&path);
                                let song_id = item.id.clone();
                                self.emit(BotEvent::MediaCached { song_id, file_path: path });
                            }
                        }
                        self.playback = Some(Playback {
                            source: opened.source,
                            pcm: opened.pcm,
                            clock: FrameClock::starting(Instant::now()),
                            frames_sent: 0,
                        });
                        self.publish();
                    }
                    Err(message) => self.fail(message),
                }
            }
            Internal::IcyTitle { generation, title } => {
                if generation == self.generation && title != self.last_title {
                    self.apply_stream_title(title);
                }
            }
        }
    }

    /// One frame slot.
    fn tick(&mut self) {
        let now = Instant::now();
        let Some(playback) = &mut self.playback else { return };
        match playback.pcm.try_next() {
            Some(PcmEvent::Frame(pcm)) => match self.encoder.encode(&pcm, self.volume) {
                Ok(packet) => {
                    self.client.send_voice(packet);
                    playback.frames_sent += 1;
                }
                Err(error) => {
                    self.fail(error.to_string());
                    return;
                }
            },
            Some(PcmEvent::End { success, message }) => {
                self.source_ended(success, message);
                return;
            }
            // The decoder is behind (a slow network): skip the slot, listeners hear a gap.
            None => {}
        }
        playback.clock.advance(now);
        if playback.frames_sent.is_multiple_of(PUBLISH_EVERY_FRAMES) {
            self.publish();
        }
    }

    fn source_ended(&mut self, success: bool, message: String) {
        let is_live = matches!(self.playback.as_ref().map(|playback| &playback.source), Some(PlaybackSource::Live));
        if !success {
            if let Some(start_seconds) = self.download_position() {
                self.start_download(start_seconds, &message);
                return;
            }
            let detail = if message.is_empty() { "ffmpeg stopped".to_owned() } else { message };
            self.fail(detail);
            return;
        }
        self.client.send_voice_stop();
        self.playback = None;
        if self.automation_owned {
            self.end_audio();
            self.emit(BotEvent::AutomationEnded { failed: false });
            return;
        }
        if is_live {
            self.end_audio();
            return;
        }
        if self.queue().repeat() == RepeatMode::Track {
            if let Some(item) = self.now_playing.clone() {
                self.begin(item, false);
                return;
            }
        }
        self.play_next();
    }

    /// Where to continue from a download, when a resolved address gave no audio. YouTube refuses
    /// many of them at once, and a resolved address can expire before a seek.
    fn download_position(&self) -> Option<f64> {
        let playback = self.playback.as_ref()?;
        let PlaybackSource::File { input, base_seconds } = &playback.source else { return None };
        let has_source = self.now_playing.as_ref().is_some_and(|item| item.source_url.is_some());
        (has_source && playback.frames_sent == 0 && is_remote_input(input)).then_some(*base_seconds)
    }

    fn start_download(&mut self, start_seconds: f64, reason: &str) {
        let Some(url) = self.now_playing.as_ref().and_then(|item| item.source_url.clone()) else { return };
        tracing::info!(reason, "The stream gave no audio, downloading the track");
        self.playback = None;
        self.generation += 1;
        let generation = self.generation;
        let internal = self.internal.clone();
        let media = self.media.clone();
        self.preparing = Some(tokio::spawn(async move {
            let result = open_download(url, media, start_seconds).await;
            let _ = internal.send(Internal::Opened { generation, result });
        }));
    }

    /// Start a track: stop what was playing, name the bot, open the source in the background.
    fn begin(&mut self, item: QueueItem, automation: bool) {
        self.stop_tasks();
        self.playback = None;
        self.generation += 1;
        self.automation_owned = automation;
        if !automation {
            self.emit(BotEvent::RequestStarted);
        }
        self.last_title.clear();
        self.last_error = None;
        self.status = Status::Playing;
        self.set_nickname(&now_playing_nickname(&item.title, &self.nickname));
        if let Some(url) = item.stream_url.clone() {
            self.start_icy_polling(url);
        }
        let generation = self.generation;
        let internal = self.internal.clone();
        let media = self.media.clone();
        let opening = item.clone();
        self.preparing = Some(tokio::spawn(async move {
            let result = open_item(opening, media).await;
            let _ = internal.send(Internal::Opened { generation, result });
        }));
        self.now_playing = Some(item);
        self.publish();
    }

    fn start_icy_polling(&mut self, url: String) {
        let generation = self.generation;
        let internal = self.internal.clone();
        self.icy = Some(tokio::spawn(async move {
            loop {
                if let Some(title) = icy::fetch_title(&url).await {
                    if internal.send(Internal::IcyTitle { generation, title }).is_err() {
                        return;
                    }
                }
                tokio::time::sleep(ICY_POLL).await;
            }
        }));
    }

    fn apply_stream_title(&mut self, title: String) {
        let Some(item) = &mut self.now_playing else { return };
        match title.split_once(" - ") {
            Some((artist, song)) if !artist.trim().is_empty() => {
                item.artist = Some(artist.trim().to_owned());
                item.title = song.trim().to_owned();
            }
            _ => item.title.clone_from(&title),
        }
        let nickname = now_playing_nickname(&title, &self.nickname);
        self.set_nickname(&nickname);
        self.last_title = title;
        self.publish();
    }

    fn pause(&mut self) {
        if self.status != Status::Playing {
            return;
        }
        self.client.send_voice_stop();
        self.status = Status::Paused;
        // A live stream cannot wait: close it and open it again on resume. A file waits, with
        // ffmpeg held by the full pipe.
        if matches!(self.playback.as_ref().map(|playback| &playback.source), Some(PlaybackSource::Live))
            || self.now_playing.as_ref().is_some_and(|item| item.stream_url.is_some())
        {
            self.stop_tasks();
            self.generation += 1;
            self.playback = None;
        }
        self.publish();
    }

    fn resume(&mut self) {
        if self.status != Status::Paused {
            return;
        }
        if let Some(playback) = &mut self.playback {
            playback.clock = FrameClock::resuming(Instant::now());
            self.status = Status::Playing;
            self.publish();
        } else if self.preparing.is_some() {
            self.status = Status::Playing;
            self.publish();
        } else if let Some(item) = self.now_playing.clone() {
            self.begin(item, self.automation_owned);
        } else {
            self.status = Status::Connected;
            self.publish();
        }
    }

    /// Reopen the decoder at another position. Only files can seek.
    fn seek(&mut self, seconds: f64) {
        let Some(Playback { source: PlaybackSource::File { input, .. }, .. }) = &self.playback else { return };
        let input = input.clone();
        let duration = self.now_playing.as_ref().and_then(|item| item.duration).unwrap_or(f64::INFINITY);
        let target = seconds.clamp(0.0, duration);
        self.generation += 1;
        self.playback = None;
        let generation = self.generation;
        let internal = self.internal.clone();
        self.preparing = Some(tokio::spawn(async move {
            let result = open_file(input, target, None, None).await;
            let _ = internal.send(Internal::Opened { generation, result });
        }));
        self.publish();
    }

    fn play_next(&mut self) {
        let next = self.queue().next().cloned();
        match next {
            Some(item) => self.begin(item, false),
            None => self.end_audio(),
        }
    }

    /// Stop the audio and stay connected.
    fn end_audio(&mut self) {
        self.stop_tasks();
        self.generation += 1;
        self.playback = None;
        self.client.send_voice_stop();
        if self.now_playing.take().is_some() {
            self.set_nickname(&self.nickname.clone());
        }
        self.status = Status::Connected;
        self.publish();
    }

    /// The track could not be played. The bot stays usable; the reason is kept for the panel,
    /// and the track leaves the queue so it does not block what follows.
    fn fail(&mut self, message: String) {
        tracing::warn!(error = %message, "Track could not be played");
        let automation = self.automation_owned;
        if !automation {
            if let Some(failed) = &self.now_playing {
                let mut queue = self.queue.lock().expect("queue lock");
                let current = queue.current().is_some_and(|item| item.id == failed.id);
                if let (true, Some(index)) = (current, queue.index()) {
                    queue.remove_at(index);
                }
            }
        }
        self.end_audio();
        self.last_error = Some(message);
        self.publish();
        if automation {
            self.emit(BotEvent::AutomationEnded { failed: true });
        }
    }

    fn note_error(&mut self, message: String) {
        tracing::warn!(error = %message, "Voice bot error");
        self.last_error = Some(message);
        self.publish();
    }

    fn set_nickname(&self, nickname: &str) {
        self.client.send_command(build_command("clientupdate", &[("client_nickname", nickname)]));
    }

    fn stop_tasks(&mut self) {
        if let Some(task) = self.preparing.take() {
            task.abort();
        }
        if let Some(task) = self.icy.take() {
            task.abort();
        }
    }

    /// Restore the name and leave, waiting briefly for the server to confirm.
    async fn leave(&mut self, events: &mut mpsc::UnboundedReceiver<Event>) {
        self.end_audio();
        self.client.disconnect();
        let wait = async {
            while let Some(event) = events.recv().await {
                if event == Event::Disconnected {
                    break;
                }
            }
        };
        let _ = tokio::time::timeout(DISCONNECT_WAIT, wait).await;
    }

    fn publish(&self) {
        let (position_seconds, frames_sent) =
            self.playback.as_ref().map_or((0.0, 0), |playback| (playback.position(), playback.frames_sent));
        let can_seek = self.status != Status::Connected
            && matches!(self.playback.as_ref().map(|playback| &playback.source), Some(PlaybackSource::File { .. }));
        let queue_length = self.queue().len();
        self.state.send_replace(Snapshot {
            status: self.status,
            now_playing: self.now_playing.clone(),
            position_seconds,
            volume: self.volume,
            queue_length,
            frames_sent,
            last_error: self.last_error.clone(),
            can_seek,
            playback_token: self.generation,
        });
    }
}
