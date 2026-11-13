//! Every command against a fake bot and fake music sources, with a real database.

use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::automation::AutomationSettings;

#[derive(Default)]
struct FakeState {
    status: Option<Status>,
    now_playing: Option<QueueItem>,
    position: f64,
    volume: u8,
    can_seek: bool,
    token: u64,
    channel: u64,
    members: Vec<u64>,
    positions: HashMap<u64, u64>,
    said: Vec<String>,
    private: Vec<(u64, String)>,
    actions: Vec<String>,
    flood: bool,
    ignored: usize,
}

struct FakeBot {
    client_id: u64,
    queue: Arc<Mutex<PlayQueue<QueueItem>>>,
    state: Mutex<FakeState>,
}

impl FakeBot {
    fn new(client_id: u64, channel: u64) -> Arc<Self> {
        Arc::new(Self {
            client_id,
            queue: Arc::default(),
            state: Mutex::new(FakeState { status: Some(Status::Connected), volume: 100, channel, ..FakeState::default() }),
        })
    }

    fn with<T>(&self, work: impl FnOnce(&mut FakeState) -> T) -> T {
        work(&mut self.state.lock().unwrap())
    }

    fn said(&self) -> Vec<String> {
        self.with(|state| state.said.clone())
    }

    fn last(&self) -> String {
        self.said().last().cloned().unwrap_or_default()
    }

    fn actions(&self) -> Vec<String> {
        self.with(|state| state.actions.clone())
    }

    fn titles(&self) -> Vec<String> {
        self.queue.lock().unwrap().get_all().iter().map(|item| item.title.clone()).collect()
    }
}

impl BotControl for FakeBot {
    fn snapshot(&self) -> Snapshot {
        self.with(|state| Snapshot {
            status: state.status.unwrap_or(Status::Connected),
            now_playing: state.now_playing.clone(),
            position_seconds: state.position,
            volume: state.volume,
            queue_length: 0,
            frames_sent: 0,
            last_error: None,
            can_seek: state.can_seek,
            playback_token: state.token,
        })
    }
    fn client_id(&self) -> u64 {
        self.client_id
    }
    fn current_channel(&self) -> u64 {
        self.with(|state| state.channel)
    }
    fn client_channel(&self, clid: u64) -> Option<u64> {
        self.with(|state| state.positions.get(&clid).copied())
    }
    fn channel_members(&self) -> Vec<u64> {
        self.with(|state| state.members.clone())
    }
    fn queue(&self) -> Arc<Mutex<PlayQueue<QueueItem>>> {
        self.queue.clone()
    }
    fn play_item(&self, item: QueueItem) {
        self.with(|state| {
            state.actions.push(format!("play {}", item.title));
            state.now_playing = Some(item);
            state.status = Some(Status::Playing);
            state.token += 1;
        });
    }
    fn play_automation(&self, item: QueueItem) {
        self.with(|state| {
            state.actions.push(format!("autodj {}", item.title));
            state.now_playing = Some(item);
            state.status = Some(Status::Playing);
        });
    }
    fn clear_automation(&self) {
        self.with(|state| {
            state.actions.push("clear".to_owned());
            state.now_playing = None;
            state.status = Some(Status::Connected);
        });
    }
    fn stop_audio(&self) {
        self.with(|state| {
            state.actions.push("stop".to_owned());
            state.now_playing = None;
            state.status = Some(Status::Connected);
        });
    }
    fn pause(&self) {
        self.with(|state| state.status = Some(Status::Paused));
    }
    fn resume(&self) {
        self.with(|state| state.status = Some(Status::Playing));
    }
    fn seek(&self, seconds: f64) {
        self.with(|state| state.actions.push(format!("seek {seconds}")));
    }
    fn set_volume(&self, volume: u8) {
        self.with(|state| state.volume = volume);
    }
    fn join_channel(&self, channel: u64) {
        self.with(|state| {
            state.actions.push(format!("join {channel}"));
            state.channel = channel;
        });
    }
    fn send_channel(&self, message: &str) {
        self.with(|state| {
            if !state.flood {
                state.said.push(message.to_owned());
            }
        });
    }
    fn send_private(&self, clid: u64, message: &str) {
        self.with(|state| state.private.push((clid, message.to_owned())));
    }
    fn flood_hold_active(&self) -> bool {
        self.with(|state| state.flood)
    }
    fn note_ignored_command(&self) {
        self.with(|state| state.ignored += 1);
    }
    fn settle(&self) -> BoxFuture<'_, ()> {
        Box::pin(async {})
    }
}

#[derive(Default)]
struct FakeTracks {
    cookies: bool,
    results: Vec<RankedResult>,
    downloads: AtomicUsize,
    hang_downloads: bool,
    fail: Option<String>,
    expanded: Mutex<Option<Expanded>>,
    shared: Mutex<Option<Collection>>,
    spotify: bool,
}

fn track(url: &str) -> QueueItem {
    QueueItem {
        id: format!("yt_{url}"),
        title: format!("Title of {}", url.rsplit('=').next().unwrap_or(url)),
        artist: Some("Artist".to_owned()),
        duration: Some(100.0),
        source: Source::Youtube,
        file_path: "/f".to_owned(),
        source_url: Some(url.to_owned()),
        stream_url: None,
    }
}

impl Tracks for Arc<FakeTracks> {
    fn has_cookies(&self) -> bool {
        self.cookies
    }
    fn has_spotify(&self) -> bool {
        self.spotify
    }
    fn resolve_share_link(&self, _url: String) -> BoxFuture<'static, Result<Collection>> {
        let shared = self.shared.lock().unwrap().clone();
        Box::pin(async move { shared.ok_or_else(|| "Could not read the tracks".to_owned()) })
    }
    fn search(&self, _query: String) -> BoxFuture<'static, Vec<RankedResult>> {
        let results = self.results.clone();
        Box::pin(async move { results })
    }
    fn find_song(&self, query: String) -> BoxFuture<'static, Result<Option<SearchResult>>> {
        Box::pin(async move {
            if query == "nothing" {
                return Ok(None);
            }
            Ok(Some(SearchResult { id: "aaaaaaaaaaa".to_owned(), title: query, artist: "A".to_owned(), duration: 1.0, thumbnail: String::new() }))
        })
    }
    fn expand(&self, _url: String, _cap: usize) -> BoxFuture<'static, Result<Expanded>> {
        let expanded = self.expanded.lock().unwrap().clone();
        Box::pin(async move { expanded.ok_or_else(|| "no".to_owned()) })
    }
    fn download(&self, url: String) -> BoxFuture<'static, Result<QueueItem>> {
        self.downloads.fetch_add(1, Ordering::SeqCst);
        let (hang, fail) = (self.hang_downloads, self.fail.clone());
        Box::pin(async move {
            if hang {
                std::future::pending::<()>().await;
            }
            match fail {
                Some(error) => Err(error),
                None => Ok(track(&url)),
            }
        })
    }
    fn lyrics(&self, input: LyricsQuery) -> BoxFuture<'static, Option<LyricsResult>> {
        Box::pin(async move {
            input.query.filter(|query| query == "found").map(|_| LyricsResult {
                artist: "A".to_owned(),
                title: "T".to_owned(),
                lyrics: "la la".to_owned(),
                instrumental: false,
            })
        })
    }
}

const USER: u64 = 50;
const CHANNEL: u64 = 7;
const BOT: i64 = 5;

struct Rig {
    service: Arc<ChatService>,
    db: Db,
    bot: Arc<FakeBot>,
    tracks: Arc<FakeTracks>,
}

fn entry_for(bot: Arc<FakeBot>, name: &str) -> BotEntry {
    BotEntry { bot, server_config_id: 1, virtual_server_id: 1, external: false, name: name.to_owned(), nickname: String::new(), automation: None }
}

async fn rig_with(tracks: FakeTracks) -> Rig {
    let db = Db::open_in_memory().await.unwrap();
    sqlx::raw_sql(
        r#"INSERT INTO "TsServerConfig" ("id", "name", "host", "webqueryPort", "apiKey", "useHttps", "createdAt", "updatedAt") VALUES (1, 's', 'h', 1, 'k', 0, 0, 0);
           INSERT INTO "MusicBot" ("id", "name", "serverConfigId", "createdAt", "updatedAt") VALUES (5, 'Müzik', 1, 0, 0), (6, 'Radyo', 1, 0, 0);"#,
    )
    .execute(db.pool())
    .await
    .unwrap();
    let tracks = Arc::new(tracks);
    let service = ChatService::new(db.clone(), Arc::new(tracks.clone()), std::env::temp_dir());
    let bot = FakeBot::new(1, CHANNEL);
    service.register(BOT, entry_for(bot.clone(), "Müzik"));
    Rig { service, db, bot, tracks }
}

async fn rig() -> Rig {
    rig_with(FakeTracks::default()).await
}

fn line(text: &str) -> Params {
    Params::from([("invokerid".to_owned(), USER.to_string()), ("msg".to_owned(), text.to_owned()), ("invokername".to_owned(), "Ali".to_owned())])
}

impl Rig {
    async fn say(&self, text: &str) {
        self.service.on_text_message(BOT, line(text), None).await;
    }

    async fn say_to(&self, bot_id: i64, text: &str) {
        self.service.on_text_message(bot_id, line(text), None).await;
    }

    async fn sql(&self, sql: &'static str) {
        sqlx::raw_sql(sql).execute(self.db.pool()).await.unwrap();
    }

    fn queue(&self, titles: &[&str]) {
        let mut queue = self.bot.queue.lock().unwrap();
        for title in titles {
            let mut item = track(title);
            item.title = (*title).to_owned();
            item.id = (*title).to_owned();
            queue.add(item).unwrap();
        }
    }

    /// Wait for what chat tasks write in the background.
    async fn requests(&self) -> Vec<teamhub_db::models::MusicRequest> {
        for _ in 0..50 {
            let rows = self.db.music_requests(1, BOT, 100).await.unwrap();
            if !rows.is_empty() {
                return rows;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Vec::new()
    }
}

#[tokio::test]
async fn lines_without_a_command_are_ignored() {
    let rig = rig().await;
    rig.say("hello").await;
    rig.say("7").await;
    assert!(rig.bot.said().is_empty(), "a number with no open search is just chat");
    let mut no_sender = line("!help");
    no_sender.remove("invokerid");
    rig.service.on_text_message(BOT, no_sender, None).await;
    assert!(rig.bot.said().is_empty());
}

#[tokio::test]
async fn the_bot_ignores_itself() {
    let rig = rig().await;
    let mut own = line("!np");
    own.insert("invokerid".to_owned(), "1".to_owned());
    rig.service.on_text_message(BOT, own, None).await;
    assert!(rig.bot.said().is_empty());
}

#[tokio::test]
async fn during_a_flood_hold_commands_are_set_aside() {
    let rig = rig().await;
    rig.bot.with(|state| state.flood = true);
    rig.say("!stop").await;
    assert_eq!(rig.bot.with(|state| state.ignored), 1);
    assert!(rig.bot.actions().is_empty());
}

#[tokio::test]
async fn help_lists_commands_once() {
    let rig = rig().await;
    rig.sql(r#"INSERT INTO "ChatCommand" ("serverConfigId", "name", "response", "description", "enabled", "createdAt", "updatedAt") VALUES (1, 'rules', 'r', 'Kurallar', 1, 0, 0)"#).await;
    rig.say("!help").await;
    let said = rig.bot.said();
    assert_eq!(said.len(), 1);
    assert!(said[0].contains("**!play <url|song name>**"));
    assert!(said[0].contains("- **!rules** — Kurallar"));
    rig.say("!help").await;
    assert_eq!(rig.bot.said().len(), 1, "a repeat right after is collapsed");
}

#[tokio::test]
async fn help_mentions_auto_dj_when_on() {
    let rig = rig().await;
    let mut entry = entry_for(rig.bot.clone(), "Müzik");
    let settings = AutomationSettings {
        enabled: true,
        source: Some(AutomationTarget { kind: AutomationSource::Radio, id: 1 }),
        ..AutomationSettings::default()
    };
    entry.automation = Some(Arc::new(Mutex::new(AutomationController::new(settings, None))));
    rig.service.register(BOT, entry);
    rig.say("!help").await;
    assert!(rig.bot.last().contains("Auto-DJ is on"));
}

#[tokio::test]
async fn custom_and_unknown_commands() {
    let rig = rig().await;
    rig.sql(
        r#"INSERT INTO "ChatCommand" ("serverConfigId", "name", "response", "enabled", "createdAt", "updatedAt") VALUES (1, 'rules', 'Be nice', 1, 0, 0), (1, 'off', 'Hidden', 0, 0, 0)"#,
    )
    .await;
    rig.say("!RULES").await;
    assert_eq!(rig.bot.last(), "Be nice");
    rig.say("!off").await;
    assert!(rig.bot.last().starts_with("Unknown command **!off**"));
    rig.say("!plya").await;
    assert!(rig.bot.last().contains("Did you mean **!play**?"));
    let count = rig.bot.said().len();
    rig.say("!!!").await;
    assert_eq!(rig.bot.said().len(), count, "not a command name");
    rig.say("!plya").await;
    assert_eq!(rig.bot.said().len(), count, "a repeat right after is collapsed");
    rig.say("!commands").await;
    assert!(rig.bot.last().contains("- **!rules** — Custom reply"));
}

#[tokio::test]
async fn a_flow_can_answer_instead_of_unknown() {
    let rig = rig().await;
    rig.service.set_flow_lookup(Arc::new(|_, _, message| message == "!flowcmd"));
    rig.say("!flowcmd").await;
    assert!(rig.bot.said().is_empty());
}

#[tokio::test]
async fn a_disabled_command_says_nothing() {
    let rig = rig().await;
    rig.db.set_setting("bot_extensions:5", r#"{"disabledCommands":["skip","ara"]}"#).await.unwrap();
    rig.say("!next").await;
    assert!(rig.bot.said().is_empty(), "the alias shares the setting");
    rig.say("!np").await;
    assert_eq!(rig.bot.last(), "_Nothing is playing._");
}

#[tokio::test]
async fn broken_extension_settings_refuse_quietly() {
    let rig = rig().await;
    rig.db.set_setting("bot_extensions:5", r#"{"disabledCommands":["Bad Name"]}"#).await.unwrap();
    rig.say("!np").await;
    assert!(rig.bot.said().is_empty());
}

#[tokio::test]
async fn restricted_commands_answer_privately_and_do_nothing() {
    let rig = rig().await;
    rig.db
        .set_setting(
            "media_command_permissions:1:1",
            r#"{"playback":{"mode":"server_groups","serverGroupIds":[6]},"queue":{"mode":"everyone"},"video":{"mode":"everyone"}}"#,
        )
        .await
        .unwrap();
    rig.queue(&["A"]);
    rig.say("!skip").await;
    assert_eq!(rig.bot.with(|state| state.private.clone()), [(USER, "You are not allowed to use this command.".to_owned())]);
    assert!(rig.bot.actions().is_empty());
    rig.say("!vol").await;
    assert_eq!(rig.bot.last(), "Volume: 100%", "the read-only form stays public");
}

struct FakeQuery(&'static str);

impl ServerQuery for FakeQuery {
    fn execute(&self, _server: i64, _sid: i64, _command: String) -> BoxFuture<'static, Result<String>> {
        let answer = self.0.to_owned();
        Box::pin(async move { Ok(answer) })
    }
}

#[tokio::test]
async fn a_server_group_from_the_server_opens_the_command() {
    let rig = rig().await;
    rig.db
        .set_setting(
            "media_command_permissions:1:1",
            r#"{"playback":{"mode":"server_groups","serverGroupIds":[6]},"queue":{"mode":"everyone"},"video":{"mode":"everyone"}}"#,
        )
        .await
        .unwrap();
    rig.queue(&["A"]);
    rig.service.set_query(Arc::new(FakeQuery("client_unique_identifier=u1 client_servergroups=6,8 client_type=0\nerror id=0 msg=ok")));
    rig.say("!skip").await;
    assert_eq!(rig.bot.actions(), ["play A"]);
}

#[test]
fn identities_come_from_the_server_and_must_match() {
    let raw = "client_unique_identifier=u1 client_servergroups=6,8 client_type=0\nerror id=0 msg=ok";
    assert_eq!(parse_client_identity(raw, None).unwrap().server_group_ids, [6, 8]);
    assert!(parse_client_identity(raw, Some("other")).is_none(), "a reused client id");
    assert!(parse_client_identity("error id=512 msg=x", None).is_none());
    assert!(parse_client_identity("client_unique_identifier=u client_servergroups=a client_type=0", None).is_none());
    assert!(parse_client_identity("client_unique_identifier=u client_servergroups=6 client_type=1", None).is_none(), "a query client");
}

#[test]
fn listeners_are_humans_in_the_channel() {
    let raw = "clid=1 cid=7 client_type=0 client_unique_identifier=a|clid=2 cid=7 client_type=1 client_unique_identifier=q|clid=3 cid=8 client_type=0 client_unique_identifier=b|clid=4 cid=7 client_type=0 client_unique_identifier=bot";
    let bots = HashSet::from([4]);
    assert_eq!(parse_listeners(raw, 7, &bots).unwrap(), [(1, "a".to_owned())]);
    assert_eq!(parse_listeners("clid=1 cid=7 client_type=0", 7, &bots), None, "a missing identity must not lower the majority");
    assert_eq!(parse_listeners("error id=5 msg=x", 7, &bots), None);
}

#[test]
fn votes_need_more_than_half_and_count_once() {
    let mut book = VoteBook::default();
    let all = ["a".to_owned(), "b".to_owned(), "c".to_owned()];
    assert_eq!(book.vote(1, 9, "a", &all), (1, 2, false));
    assert_eq!(book.vote(1, 9, "a", &all), (1, 2, false));
    assert_eq!(book.vote(1, 9, "b", &all), (2, 2, true));
    assert_eq!(book.vote(1, 9, "c", &all), (3, 2, false), "skipped only once");
    assert_eq!(book.vote(1, 10, "a", &all), (1, 2, false), "a new track starts over");
    let two = ["b".to_owned(), "c".to_owned()];
    book.vote(1, 11, "a", &all);
    assert_eq!(book.vote(1, 11, "b", &two), (1, 2, false), "who left no longer counts");
}

#[tokio::test]
async fn vote_skip_needs_a_verified_listener() {
    let rig = rig().await;
    rig.say("!voteskip").await;
    assert_eq!(rig.bot.with(|state| state.private.last().unwrap().1.clone()), "Nothing is playing.");

    let rig = self::rig().await;
    rig.bot.with(|state| state.now_playing = Some(track("u")));
    rig.say("!voteskip").await;
    assert_eq!(rig.bot.with(|state| state.private.last().unwrap().1.clone()), "Vote-skip needs a verified listener in the bot channel.");

    let rig = self::rig().await;
    rig.bot.with(|state| state.now_playing = Some(track("u")));
    rig.queue(&["Next"]);
    rig.service.set_query(Arc::new(FakeQuery("clid=50 cid=7 client_type=0 client_unique_identifier=me")));
    rig.say("!voteskip").await;
    assert_eq!(rig.bot.actions(), ["play Next"], "the only listener is a majority");
}

#[tokio::test]
async fn two_bots_in_a_channel_answer_once() {
    let rig = rig().await;
    let other = FakeBot::new(2, CHANNEL);
    rig.service.register(6, entry_for(other.clone(), "Radyo"));

    rig.say_to(BOT, "!np").await;
    rig.say_to(6, "!np").await;
    assert_eq!(rig.bot.said().len() + other.said().len(), 1);
    assert_eq!(rig.bot.said().len(), 1, "the lowest id when none plays");

    // The playing one takes it.
    other.with(|state| {
        state.status = Some(Status::Playing);
        state.now_playing = Some(track("u"));
    });
    rig.say_to(BOT, "!stop").await;
    rig.say_to(6, "!stop").await;
    assert_eq!(other.actions(), ["stop"]);
    assert!(rig.bot.actions().is_empty());
}

#[tokio::test]
async fn a_named_bot_answers_and_its_name_leaves_the_arguments() {
    let rig = rig().await;
    let other = FakeBot::new(2, CHANNEL);
    rig.service.register(6, entry_for(other.clone(), "Radyo"));
    rig.say_to(BOT, "!vol 30 @Radyo").await;
    rig.say_to(6, "!vol 30 @Radyo").await;
    assert_eq!(other.with(|state| state.volume), 30);
    assert_eq!(rig.bot.with(|state| state.volume), 100);
    assert_eq!(other.last(), "Volume set to 30%.");
}

#[tokio::test]
async fn a_bot_on_another_server_does_not_compete() {
    let rig = rig().await;
    let other = FakeBot::new(2, CHANNEL);
    let mut entry = entry_for(other.clone(), "Radyo");
    entry.server_config_id = 2;
    rig.service.register(6, entry);
    rig.say_to(BOT, "!np").await;
    rig.say_to(6, "!np").await;
    assert_eq!(rig.bot.said().len(), 1);
    assert_eq!(other.said().len(), 1);
}

#[tokio::test]
async fn play_downloads_starts_and_names_the_track() {
    let rig = rig().await;
    rig.say("!play [URL]https://www.youtube.com/watch?v=aaaaaaaaaaa[/URL]").await;
    assert_eq!(rig.bot.actions(), ["play Title of aaaaaaaaaaa"]);
    assert_eq!(rig.bot.said(), ["Loading...", "Now playing: Artist - Title of aaaaaaaaaaa"]);
    assert_eq!(rig.bot.titles(), ["Title of aaaaaaaaaaa"]);
    let rows = rig.requests().await;
    assert_eq!(rows[0].url, "https://www.youtube.com/watch?v=aaaaaaaaaaa");
    assert_eq!(rows[0].source, "youtube");
}

#[tokio::test]
async fn play_while_playing_queues_with_a_position() {
    let rig = rig().await;
    rig.say("!play https://youtu.be/aaaaaaaaaaa").await;
    rig.say("!play https://youtu.be/bbbbbbbbbbb").await;
    assert_eq!(rig.bot.last(), "Queued: Artist - Title of bbbbbbbbbbb (position #2)");
    assert_eq!(rig.bot.actions().len(), 1, "the first keeps playing");
}

#[tokio::test]
async fn play_with_a_song_name_searches() {
    let rig = rig().await;
    rig.say("!play tarkan").await;
    assert_eq!(rig.bot.said()[0], "Searching YouTube for \"tarkan\"...");
    assert_eq!(rig.tracks.downloads.load(Ordering::SeqCst), 1);
    rig.say("!play nothing").await;
    assert_eq!(rig.bot.last(), "No results for \"nothing\".");
}

#[tokio::test]
async fn play_failures_are_reported() {
    let rig = rig_with(FakeTracks { fail: Some("boom".to_owned()), ..FakeTracks::default() }).await;
    rig.say("!play https://youtu.be/aaaaaaaaaaa").await;
    assert_eq!(rig.bot.last(), "Failed to play: boom");
    assert!(rig.bot.titles().is_empty(), "nothing queued");
}

#[tokio::test]
async fn play_without_arguments_resumes() {
    let rig = rig().await;
    rig.say("!play").await;
    assert_eq!(rig.bot.last(), "Usage: !play <url|song name>");
    rig.bot.with(|state| state.status = Some(Status::Paused));
    rig.say("!play").await;
    assert_eq!(rig.bot.last(), "Resumed.");
}

#[tokio::test]
async fn a_playlist_link_starts_the_first_and_queues_the_rest() {
    let rig = rig().await;
    *rig.tracks.expanded.lock().unwrap() = Some(Expanded {
        title: Some("Liste".to_owned()),
        urls: vec!["https://y/?v=one".to_owned(), "https://y/?v=two".to_owned(), "https://y/?v=three".to_owned()],
    });
    rig.say("!play https://www.youtube.com/playlist?list=PLabc").await;
    assert_eq!(rig.bot.last(), "Now playing: Artist - Title of one (+2 more from \"Liste\")");
    for _ in 0..100 {
        if rig.bot.titles().len() == 3 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(rig.bot.titles(), ["Title of one", "Title of two", "Title of three"]);
    assert_eq!(rig.bot.actions().len(), 1);
}

fn link_track(artist: &str, title: &str) -> LinkTrack {
    LinkTrack { artist: artist.to_owned(), title: title.to_owned() }
}

#[tokio::test]
async fn a_spotify_playlist_is_matched_on_youtube() {
    let rig = rig().await;
    *rig.tracks.shared.lock().unwrap() = Some(Collection { title: Some("Liste".to_owned()), tracks: vec![link_track("A", "One"), link_track("B", "Two")] });
    rig.say("!play https://open.spotify.com/playlist/xxxxxxxxxxxxxxxxxxxxxx").await;
    assert_eq!(rig.bot.last(), "Now playing: Artist - Title of aaaaaaaaaaa (+1 more from \"Liste\")");
    for _ in 0..100 {
        if rig.bot.titles().len() == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(rig.bot.titles().len(), 2);
}

#[tokio::test]
async fn an_apple_music_song_plays_alone() {
    let rig = rig().await;
    *rig.tracks.shared.lock().unwrap() = Some(Collection { title: Some("A - One".to_owned()), tracks: vec![link_track("A", "One")] });
    rig.say("!play https://music.apple.com/tr/song/x/1").await;
    assert_eq!(rig.bot.last(), "Now playing: Artist - Title of aaaaaaaaaaa");
}

#[tokio::test]
async fn a_track_with_no_match_is_reported() {
    let rig = rig().await;
    *rig.tracks.shared.lock().unwrap() = Some(Collection { title: None, tracks: vec![link_track("", "nothing")] });
    rig.say("!play https://open.spotify.com/track/xxxxxxxxxxxxxxxxxxxxxx").await;
    assert_eq!(rig.bot.last(), "Failed to play: No YouTube match for Spotify track:  - nothing");
}

#[tokio::test]
async fn a_link_that_cannot_be_read_does_not_become_a_song() {
    let rig = rig().await;
    rig.say("!play https://open.spotify.com/playlist/xxxxxxxxxxxxxxxxxxxxxx").await;
    assert_eq!(rig.bot.last(), "Failed to play: Could not read the tracks");
    assert_eq!(rig.tracks.downloads.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn an_empty_playlist_link_is_an_error() {
    let rig = rig().await;
    rig.say("!play https://www.youtube.com/playlist?list=PLabc").await;
    assert_eq!(rig.bot.last(), "Failed to play: Could not resolve any videos from that playlist URL");
}

#[tokio::test]
async fn stop_cancels_a_download_in_progress() {
    let rig = rig_with(FakeTracks { hang_downloads: true, ..FakeTracks::default() }).await;
    let service = rig.service.clone();
    let waiting = tokio::spawn(async move { service.on_text_message(BOT, line("!play https://youtu.be/aaaaaaaaaaa"), None).await });
    tokio::time::sleep(Duration::from_millis(20)).await;
    rig.say("!stop").await;
    tokio::time::timeout(Duration::from_secs(2), waiting).await.expect("the request stopped").unwrap();
    assert!(rig.bot.titles().is_empty());
    assert_eq!(rig.bot.said(), ["Loading...", "Playback stopped."]);
}

#[tokio::test]
async fn stop_clears_music_but_keeps_the_queue_under_radio() {
    let rig = rig().await;
    rig.queue(&["A", "B"]);
    rig.bot.with(|state| state.now_playing = Some(track("u")));
    rig.say("!stop").await;
    assert!(rig.bot.titles().is_empty());

    let rig = self::rig().await;
    rig.queue(&["A", "B"]);
    let mut radio = track("u");
    radio.stream_url = Some("http://r".to_owned());
    rig.bot.with(|state| state.now_playing = Some(radio));
    rig.say("!stop").await;
    assert_eq!(rig.bot.titles().len(), 2);
    assert_eq!(rig.bot.last(), "Playback stopped.");
}

#[tokio::test]
async fn pause_toggles() {
    let rig = rig().await;
    rig.say("!pause").await;
    assert_eq!(rig.bot.last(), "Nothing is playing.");
    rig.bot.with(|state| state.status = Some(Status::Playing));
    rig.say("!pause").await;
    assert_eq!(rig.bot.last(), "Paused.");
    rig.say("!pause").await;
    assert_eq!(rig.bot.last(), "Resumed.");
}

#[tokio::test]
async fn skip_and_previous_walk_the_queue() {
    let rig = rig().await;
    rig.queue(&["A", "B"]);
    rig.say("!skip").await;
    assert_eq!(rig.bot.last(), "Skipped to: A");
    rig.say("!next").await;
    assert_eq!(rig.bot.last(), "Skipped to: B");
    rig.say("!prev").await;
    assert_eq!(rig.bot.last(), "Previous: A");
    rig.say("!skip").await;
    rig.say("!skip").await;
    assert_eq!(rig.bot.last(), "Queue empty — playback stopped.");
    assert_eq!(rig.bot.actions().last().unwrap(), "stop");
    let empty = self::rig().await;
    empty.say("!prev").await;
    assert_eq!(empty.bot.last(), "No previous track.");
}

#[tokio::test]
async fn volume_shows_sets_and_refuses() {
    let rig = rig().await;
    rig.say("!vol").await;
    assert_eq!(rig.bot.last(), "Volume: 100%");
    rig.say("!volume 40").await;
    assert_eq!(rig.bot.last(), "Volume set to 40%.");
    for bad in ["!vol 101", "!vol abc", "!vol -1"] {
        rig.say(bad).await;
        assert_eq!(rig.bot.last(), "Usage: !vol <0-100>", "{bad}");
    }
    assert_eq!(rig.bot.with(|state| state.volume), 40);
}

#[tokio::test]
async fn seek_needs_a_seekable_track_and_a_number() {
    let rig = rig().await;
    rig.say("!seek abc").await;
    assert_eq!(rig.bot.last(), "Usage: !seek <seconds|+seconds|-seconds>");
    rig.say("!seek 1e3").await;
    assert_eq!(rig.bot.last(), "Usage: !seek <seconds|+seconds|-seconds>");
    rig.say("!seek 10").await;
    assert_eq!(rig.bot.last(), "Current source cannot be seeked. Play a local/downloaded track first.");

    rig.bot.with(|state| {
        state.can_seek = true;
        state.position = 50.0;
        state.now_playing = Some(track("u"));
    });
    rig.say("!seek +10").await;
    assert_eq!(rig.bot.last(), "Seeked to 60 seconds.");
    rig.say("!seek -500").await;
    assert_eq!(rig.bot.last(), "Seeked to 0 seconds.");
    rig.say("!seek 9999").await;
    assert_eq!(rig.bot.last(), "Seeked to 100 seconds.", "not past the end");
}

#[tokio::test]
async fn now_playing_shows_the_track_and_what_follows() {
    let rig = rig().await;
    rig.say("!np").await;
    assert_eq!(rig.bot.last(), "_Nothing is playing._");
    rig.queue(&["A", "B"]);
    rig.say("!skip").await;
    rig.say("!nowplaying").await;
    let message = rig.bot.last();
    assert!(message.contains("**A**"));
    assert!(message.contains("### Up Next"));
    assert!(message.contains("1. B"));
}

#[tokio::test]
async fn radio_lists_and_plays() {
    let rig = rig().await;
    rig.say("!radio").await;
    assert_eq!(rig.bot.last(), "No radio stations configured.");
    rig.sql(r#"INSERT INTO "RadioStation" ("id", "name", "url", "genre", "serverConfigId", "createdAt") VALUES (3, 'Power', 'http://p', 'Pop', 1, 0)"#).await;
    rig.say("!radio").await;
    assert!(rig.bot.last().contains("- **3** — Power _(Pop)_"));
    rig.say("!radio abc").await;
    assert_eq!(rig.bot.last(), "Usage: !radio <id> — Use !radio to list stations.");
    rig.say("!radio 9").await;
    assert_eq!(rig.bot.last(), "Station #9 not found. Use !radio to list stations.");
    rig.say("!radio 3").await;
    assert_eq!(rig.bot.last(), "Now playing: Power");
    let playing = rig.bot.snapshot().now_playing.unwrap();
    assert_eq!(playing.stream_url.as_deref(), Some("http://p"));
    assert!(rig.bot.titles().is_empty(), "radio does not enter the queue");
    assert_eq!(rig.requests().await[0].source, "radio");
}

#[tokio::test]
async fn search_needs_cookies_and_a_query() {
    let rig = rig().await;
    rig.say("!ara a").await;
    assert_eq!(rig.bot.last(), "Kullanım: !ara <şarkı veya sanatçı>");
    rig.say("!ara tarkan").await;
    assert!(rig.bot.last().starts_with("YouTube çerezleri gerekli."));
}

fn ranked(id: &str) -> RankedResult {
    RankedResult {
        id: id.to_owned(),
        title: format!("Song {id}"),
        artist: "A".to_owned(),
        duration: 100.0,
        url: format!("https://www.youtube.com/watch?v={id}"),
        source: SearchSource::Youtube,
        score: 1.0,
    }
}

#[tokio::test]
async fn a_search_is_answered_with_a_number() {
    let rig = rig_with(FakeTracks { cookies: true, results: vec![ranked("aaaaaaaaaaa"), ranked("bbbbbbbbbbb")], ..FakeTracks::default() }).await;
    rig.say("!ara tarkan").await;
    assert!(rig.bot.last().ends_with("Çalmak için numarayı yaz: **1**–**2**"));
    rig.say("5").await;
    assert_eq!(rig.bot.last(), "Geçersiz seçim. Listede görünen bir numarayı yazın.");
    rig.say("2").await;
    assert_eq!(rig.bot.said().iter().filter(|text| text.starts_with("Hazırlanıyor: A - Song bbbbbbbbbbb")).count(), 1);
    assert_eq!(rig.bot.last(), "Now playing: Artist - Title of bbbbbbbbbbb");
    let count = rig.bot.said().len();
    rig.say("1").await;
    assert_eq!(rig.bot.said().len(), count, "the search is over");
}

#[tokio::test]
async fn another_users_number_does_not_answer_a_search() {
    let rig = rig_with(FakeTracks { cookies: true, results: vec![ranked("aaaaaaaaaaa")], ..FakeTracks::default() }).await;
    rig.say("!ara tarkan").await;
    let count = rig.bot.said().len();
    let mut other = line("1");
    other.insert("invokerid".to_owned(), "99".to_owned());
    rig.service.on_text_message(BOT, other, None).await;
    assert_eq!(rig.bot.said().len(), count);
}

#[tokio::test]
async fn a_blocked_download_gets_a_clear_message_without_details() {
    let rig = rig_with(FakeTracks {
        cookies: true,
        results: vec![ranked("aaaaaaaaaaa")],
        fail: Some("/secret/path: Sign in to confirm you're not a bot".to_owned()),
        ..FakeTracks::default()
    })
    .await;
    rig.say("!ara tarkan").await;
    rig.say("1").await;
    assert!(rig.bot.last().starts_with("Çalınamadı: YouTube sunucu bağlantısında doğrulama istiyor."));
    assert!(!rig.bot.last().contains("secret"));
}

#[tokio::test]
async fn history_and_again() {
    let rig = rig().await;
    rig.say("!history").await;
    assert_eq!(rig.bot.last(), "No request history for this bot yet.");
    rig.say("!history x").await;
    assert_eq!(rig.bot.last(), "Usage: !history");
    rig.say("!again").await;
    assert_eq!(rig.bot.last(), "Request not found. Use !history to list requests.");
    rig.say("!again abc").await;
    assert_eq!(rig.bot.last(), "Usage: !again [id]");

    rig.sql(r#"INSERT INTO "MusicRequest" ("id", "title", "url", "source", "status", "serverConfigId", "musicBotId", "requestedAt") VALUES (8, 'Eski [şarkı]', '/etc/passwd', 'local', 'available', 1, 5, 1793767111000), (9, 'Radyo', 'ftp://x', 'radio', 'available', 1, 5, 1793767112000)"#).await;
    rig.say("!history").await;
    let message = rig.bot.last();
    assert!(message.contains("#9 Radyo — radio — "));
    assert!(message.contains("#8 Eski &#91;şarkı&#93; — local — 2026-"), "{message}");
    assert!(message.ends_with("Use !again <id>, or !again for the latest request."));

    // Neither may be played: outside the music folder, and not an http address.
    rig.say("!again 8").await;
    assert_eq!(rig.bot.last(), MEDIA_UNAVAILABLE);
    assert_eq!(rig.db.music_request(1, BOT, 8).await.unwrap().unwrap().status, "unavailable");
    rig.say("!again").await;
    assert_eq!(rig.bot.last(), MEDIA_UNAVAILABLE);
    assert!(rig.bot.actions().is_empty());
}

#[test]
fn unavailable_means_the_media_is_gone() {
    for message in ["ERROR: Video unavailable", "HTTP Error 404: x", "This is a private video", "the file was not found"] {
        assert!(is_unavailable_error(message), "{message}");
    }
    for message in ["yt-dlp not found: x", "spawn ENOENT file not found", "HTTP Error 403", "network down"] {
        assert!(!is_unavailable_error(message), "{message}");
    }
}

#[tokio::test]
async fn queue_commands() {
    let rig = rig().await;
    rig.say("!queue").await;
    assert_eq!(rig.bot.last(), "_Queue is empty._");
    rig.say("!queue remove 1").await;
    assert_eq!(rig.bot.last(), "Invalid index. The queue is empty.");
    rig.queue(&["A", "B", "C"]);
    rig.say("!queue show").await;
    assert!(rig.bot.last().starts_with("## Queue (3 tracks)"));
    rig.say("!queue remove 9").await;
    assert_eq!(rig.bot.last(), "Invalid index. Use a track number from 1 to 3 (see !queue).");
    rig.say("!queue remove 2").await;
    assert_eq!(rig.bot.last(), "Removed #2: B");
    rig.say("!queue play 2").await;
    assert_eq!(rig.bot.last(), "Playing #2: C");
    assert_eq!(rig.bot.actions(), ["play C"]);
    rig.say("!queue bogus").await;
    assert_eq!(rig.bot.last(), "Usage: !queue [show|play <n>|remove <n>|clear|<url>]");
    rig.say("!add https://youtu.be/aaaaaaaaaaa").await;
    assert_eq!(rig.bot.titles().len(), 3);
    assert!(rig.bot.last().starts_with("Queued: "));
    rig.say("!queue clear").await;
    assert_eq!(rig.bot.last(), "Queue cleared.");
    assert!(rig.bot.titles().is_empty());
}

#[tokio::test]
async fn remove_finds_one_upcoming_track() {
    let rig = rig().await;
    rig.say("!remove").await;
    assert_eq!(rig.bot.last(), "Usage: !remove <text>");
    rig.queue(&["Played", "Kuzu Kuzu", "Kuzu Yeni"]);
    rig.say("!skip").await;
    rig.say("!remove played").await;
    assert_eq!(rig.bot.last(), "No upcoming track matches.");
    rig.say("!remove kuzu").await;
    assert_eq!(rig.bot.last(), "Multiple matches; use !queue remove <n>: #2 Kuzu Kuzu, #3 Kuzu Yeni");
    rig.say("!remove kuzu kuzu").await;
    assert_eq!(rig.bot.last(), "Removed: Kuzu Kuzu");
}

#[tokio::test]
async fn repeat_and_shuffle() {
    let rig = rig().await;
    rig.say("!repeat").await;
    assert_eq!(rig.bot.last(), "Repeat mode: off");
    rig.say("!repeat track").await;
    assert_eq!(rig.bot.last(), "Repeat mode: track");
    rig.say("!repeat nope").await;
    assert_eq!(rig.bot.last(), "Usage: !repeat [off|track|queue]");
    rig.say("!shuffle").await;
    assert_eq!(rig.bot.last(), "Shuffle on.");
    rig.say("!shuffle no").await;
    assert_eq!(rig.bot.last(), "Shuffle off.");
    rig.say("!shuffle maybe").await;
    assert_eq!(rig.bot.last(), "Usage: !shuffle [on|off]");
}

#[tokio::test]
async fn playlists_by_id_and_name() {
    let rig = rig().await;
    rig.say("!playlist").await;
    assert_eq!(rig.bot.last(), "No playlists configured for this bot/server.");
    rig.sql(
        r#"INSERT INTO "Song" ("id", "title", "filePath", "source", "sourceUrl", "serverConfigId", "createdAt") VALUES (1, 'One', '', 'youtube', 'https://y/1', 1, 0), (2, 'Two', '', 'youtube', 'https://y/2', 1, 0);
           INSERT INTO "Playlist" ("id", "name", "mode", "serverConfigId", "createdAt") VALUES (1, 'Rock', 'x', 1, 0), (2, 'Rock Klasik', 'x', 1, 0), (3, 'Boş', 'x', 1, 0), (4, 'Pop Hits', 'x', 1, 0), (5, 'Pop Yeni', 'x', 1, 0);
           INSERT INTO "PlaylistSong" ("playlistId", "songId", "position") VALUES (1, 2, 0), (1, 1, 1);"#,
    )
    .await;
    rig.say("!pl").await;
    assert!(rig.bot.last().starts_with("Playlists (first 10):\n[3] Boş"));
    rig.say("!pl yok").await;
    assert_eq!(rig.bot.last(), "Playlist not found. Use !playlist to list.");
    rig.say("!pl pop").await;
    assert_eq!(rig.bot.last(), "Ambiguous playlist; use an ID: [4] Pop Hits, [5] Pop Yeni");
    rig.say("!pl 3").await;
    assert_eq!(rig.bot.last(), "Playlist is empty.");
    rig.say("!pl rock").await;
    assert_eq!(rig.bot.last(), "Queued playlist \"Rock\" (2 tracks).", "an exact name wins");
    assert_eq!(rig.bot.titles(), ["Two", "One"]);
    assert_eq!(rig.bot.actions(), ["play Two"]);
}

#[tokio::test]
async fn lyrics_for_a_query_or_the_track() {
    let rig = rig().await;
    rig.say("!lyrics").await;
    assert_eq!(rig.bot.last(), "Nothing playing. Usage: !lyrics [artist - title]");
    rig.say("!lyrics found").await;
    assert_eq!(rig.bot.last(), "🎤 A — T\nla la");
    rig.say("!lyrics missing").await;
    assert_eq!(rig.bot.last(), "Lyrics not found for \"missing\".");
}

#[tokio::test]
async fn commands_not_ported_yet_say_so() {
    let rig = rig().await;
    rig.say("!stream u").await;
    assert_eq!(rig.bot.last(), "!stream is not available yet.");
}

async fn second_bot(rig: &Rig, channel: u64) -> Arc<FakeBot> {
    let other = FakeBot::new(2, channel);
    rig.service.register(6, entry_for(other.clone(), "Radyo"));
    other
}

#[tokio::test]
async fn here_brings_the_only_bot() {
    let rig = rig().await;
    rig.bot.with(|state| {
        state.channel = 3;
        state.positions.insert(USER, CHANNEL);
    });
    rig.say("!here").await;
    assert_eq!(rig.bot.said(), ["Müzik [#5] is joining."]);
    assert_eq!(rig.bot.actions(), ["join 7"]);
}

#[tokio::test]
async fn here_says_when_it_is_already_here() {
    let rig = rig().await;
    rig.say("!here").await;
    assert_eq!(rig.bot.last(), "Müzik [#5] is already here.");
    assert!(rig.bot.actions().is_empty());
}

#[tokio::test]
async fn here_lists_idle_bots_elsewhere_too() {
    let rig = rig().await;
    second_bot(&rig, 99).await;
    rig.say("!here").await;
    assert_eq!(rig.bot.last(), "Available bots:\n[5] Müzik (already here)\n[6] Radyo\nUse: !here <id>");
}

#[tokio::test]
async fn here_does_not_take_a_bot_from_people() {
    let rig = rig().await;
    let other = second_bot(&rig, 99).await;
    other.with(|state| state.members = vec![77]);
    rig.bot.with(|state| state.channel = 98);
    rig.bot.with(|state| {
        state.members = vec![78];
        state.positions.insert(USER, CHANNEL);
    });
    rig.say("!here").await;
    assert!(rig.bot.last().starts_with("All music bots are busy with other users:\n[5] Müzik\n[6] Radyo"));
    assert!(other.actions().is_empty());
}

#[tokio::test]
async fn here_with_an_id_is_intentional() {
    let rig = rig().await;
    let other = second_bot(&rig, 99).await;
    other.with(|state| state.members = vec![77]);
    rig.say("!here 6").await;
    assert_eq!(other.actions(), ["join 7"]);
    assert_eq!(rig.bot.last(), "Radyo [#6] is joining.");
    rig.say("!come 77").await;
    assert_eq!(rig.bot.last(), "Bot #77 is not available. Use !here to list bots.");
    rig.say("!here abc").await;
    assert_eq!(rig.bot.last(), "Usage: !here [id] — Use !here to list bots.");
}

#[tokio::test]
async fn here_picks_the_only_idle_bot() {
    let rig = rig().await;
    let other = second_bot(&rig, 99).await;
    rig.bot.with(|state| {
        state.channel = 98;
        state.members = vec![78];
        state.positions.insert(USER, CHANNEL);
    });
    rig.say("!here").await;
    assert_eq!(other.actions(), ["join 7"]);
}

#[tokio::test]
async fn here_counts_other_music_bots_as_nobody() {
    let rig = rig().await;
    let other = second_bot(&rig, 99).await;
    // The other music bot's client id is in this bot's channel: still idle.
    rig.bot.with(|state| {
        state.channel = 98;
        state.members = vec![2];
        state.positions.insert(USER, CHANNEL);
    });
    other.with(|state| state.members = vec![55]);
    rig.say("!here").await;
    assert_eq!(rig.bot.actions(), ["join 7"]);
}

#[tokio::test]
async fn a_24_7_bot_is_not_idle() {
    let rig = rig().await;
    let other = second_bot(&rig, 99).await;
    let settings = AutomationSettings { stay_in_channel: true, ..AutomationSettings::default() };
    let mut entry = entry_for(other.clone(), "Radyo");
    entry.automation = Some(Arc::new(Mutex::new(AutomationController::new(settings, None))));
    rig.service.register(6, entry);
    rig.bot.with(|state| {
        state.channel = 98;
        state.positions.insert(USER, CHANNEL);
    });
    rig.say("!here").await;
    assert_eq!(rig.bot.actions(), ["join 7"], "the idle one goes");
    assert!(other.actions().is_empty());
}

#[test]
fn the_artist_is_not_repeated() {
    assert_eq!(track_label(Some("Rick Astley"), "Rick Astley - Never Gonna"), "Rick Astley - Never Gonna");
    assert_eq!(track_label(Some("Tarkan"), "Kuzu Kuzu"), "Tarkan - Kuzu Kuzu");
    assert_eq!(track_label(None, "X"), "Unknown - X");
}

#[test]
fn links_lose_their_tags() {
    assert_eq!(strip_url_tags("[URL]https://a[/URL]"), "https://a");
    assert_eq!(strip_url_tags("[url=https://x]https://a[/url] b"), "https://a b");
    assert_eq!(strip_url_tags("[URL]unclosed"), "[URL]unclosed");
    assert_eq!(strip_url_tags("[urlish]x"), "[urlish]x");
    assert_eq!(strip_url_tags("  plain "), "plain");
}

#[test]
fn search_choices_are_one_to_ten() {
    for (text, expected) in [("1", Some(1)), ("10", Some(10)), ("0", None), ("11", None), ("01", None), ("+1", None), ("x", None)] {
        assert_eq!(is_search_choice(text), expected, "{text}");
    }
}

#[test]
fn history_paths_stay_in_the_music_folder() {
    let dir = std::env::temp_dir().join(format!("teamhub-history-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("music")).unwrap();
    std::fs::write(dir.join("music").join("a.mp3"), "x").unwrap();
    std::fs::write(dir.join("outside.mp3"), "x").unwrap();
    let music = dir.join("music");
    assert!(resolve_under_music_dir(&music, "a.mp3").is_some());
    assert!(resolve_under_music_dir(&music, "../outside.mp3").is_none());
    assert!(resolve_under_music_dir(&music, &dir.join("outside.mp3").display().to_string()).is_none());
    assert!(resolve_under_music_dir(&music, "missing.mp3").is_none());
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn auto_dj_plays_a_playlist_in_turn_and_gives_way() {
    let rig = rig().await;
    rig.sql(
        r#"INSERT INTO "Song" ("id", "title", "filePath", "source", "serverConfigId", "createdAt") VALUES (1, 'One', '/1', 'local', 1, 0), (2, 'Two', '/2', 'local', 1, 0);
           INSERT INTO "Playlist" ("id", "name", "mode", "serverConfigId", "createdAt") VALUES (1, 'P', 'x', 1, 0);
           INSERT INTO "PlaylistSong" ("playlistId", "songId", "position") VALUES (1, 1, 0), (1, 2, 1);"#,
    )
    .await;
    let settings = AutomationSettings {
        enabled: true,
        source: Some(AutomationTarget { kind: AutomationSource::Playlist, id: 1 }),
        ..AutomationSettings::default()
    };
    let automation = Arc::new(Mutex::new(AutomationController::new(settings, None)));
    let mut entry = entry_for(rig.bot.clone(), "Müzik");
    entry.automation = Some(automation.clone());

    rig.service.automation_step(BOT, &entry, &automation).await;
    assert_eq!(rig.bot.actions(), ["autodj One"]);
    assert!(rig.bot.titles().is_empty(), "the request queue stays out of it");
    let stored = rig.db.music_bot(BOT).await.unwrap().unwrap().automation_state.unwrap();
    assert!(stored.contains(r#""lastTrackId":"1""#), "{stored}");

    // The track ended by itself: the next one, wrapping around.
    for expected in ["autodj Two", "autodj One"] {
        rig.bot.clear_automation();
        automation.lock().unwrap().source_ended(false, Utc::now().timestamp_millis());
        rig.service.automation_step(BOT, &entry, &automation).await;
        assert_eq!(rig.bot.actions().last().unwrap(), expected);
    }

    // A request arrives.
    rig.queue(&["Request"]);
    rig.service.automation_step(BOT, &entry, &automation).await;
    assert_eq!(rig.bot.actions().last().unwrap(), "clear");
    assert!(!automation.lock().unwrap().active());
}

#[tokio::test]
async fn auto_dj_backs_off_when_the_source_is_gone() {
    let rig = rig().await;
    let settings = AutomationSettings {
        enabled: true,
        source: Some(AutomationTarget { kind: AutomationSource::Radio, id: 404 }),
        ..AutomationSettings::default()
    };
    let automation = Arc::new(Mutex::new(AutomationController::new(settings, None)));
    let mut entry = entry_for(rig.bot.clone(), "Müzik");
    entry.automation = Some(automation.clone());
    rig.service.automation_step(BOT, &entry, &automation).await;
    assert!(rig.bot.actions().is_empty());
    assert_eq!(automation.lock().unwrap().state().failures, 1);
}

#[tokio::test]
async fn auto_dj_requires_listeners_when_asked_and_ignores_other_bots() {
    let rig = rig().await;
    rig.sql(r#"INSERT INTO "RadioStation" ("id", "name", "url", "serverConfigId", "createdAt") VALUES (1, 'R', 'http://r', 1, 0)"#).await;
    let settings = AutomationSettings {
        enabled: true,
        source: Some(AutomationTarget { kind: AutomationSource::Radio, id: 1 }),
        require_listeners: true,
        ..AutomationSettings::default()
    };
    let automation = Arc::new(Mutex::new(AutomationController::new(settings, None)));
    let mut entry = entry_for(rig.bot.clone(), "Müzik");
    entry.automation = Some(automation.clone());
    second_bot(&rig, CHANNEL).await;
    rig.bot.with(|state| state.members = vec![2]);
    rig.service.automation_step(BOT, &entry, &automation).await;
    assert!(rig.bot.actions().is_empty(), "another music bot is not a listener");
    rig.bot.with(|state| state.members = vec![2, USER]);
    rig.service.automation_step(BOT, &entry, &automation).await;
    assert_eq!(rig.bot.actions(), ["autodj R"]);
}
