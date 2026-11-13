//! Music bots through the API, without any TeamSpeak server: everything but the connection.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use teamhub_core::{Config, FieldCipher};
use teamhub_db::Db;
use teamhub_media::cache::MediaCache;
use teamhub_media::youtube::YtDlpOptions;
use teamhub_server::auth::sign_access_token;
use teamhub_server::client_ip::TrustProxy;
use teamhub_server::pool::ConnectionPool;
use teamhub_server::state::{AppState, Limiters};
use teamhub_voice::activity::ActivityLog;
use teamhub_voice::chat::service::{ChatService, RealTracks};
use teamhub_voice::manager::BotManager;
use tower::ServiceExt;

const SECRET: &str = "test-jwt-secret";

struct Api {
    app: Router,
    manager: Arc<BotManager>,
    db: Db,
    admin: String,
    viewer: String,
}

async fn api() -> Api {
    let db = Db::open_in_memory().await.unwrap();
    let config = Config::from_lookup(|name| match name {
        "JWT_SECRET" => Some(SECRET.to_owned()),
        "ENCRYPTION_KEY" => Some("test-encryption-key".to_owned()),
        _ => None,
    })
    .unwrap();
    let cipher = Arc::new(FieldCipher::new(&config.encryption_source).unwrap());
    let admin = db.create_user("admin", "x", "Admin", "admin").await.unwrap();
    let viewer = db.create_user("viewer", "x", "Viewer", "viewer").await.unwrap();
    let music_dir = std::env::temp_dir().join(format!("teamhub-bots-test-{}", std::process::id()));
    let cache = MediaCache::new(&music_dir);
    let tracks = Arc::new(RealTracks { cache: cache.clone(), yt_dlp: YtDlpOptions::default(), spotify: None });
    let chat = ChatService::new(db.clone(), tracks, music_dir);
    let bots = BotManager::new(db.clone(), cipher.clone(), cache, YtDlpOptions::default(), chat);
    bots.set_final_identity_level(8);
    let manager = bots.clone();
    let state = AppState {
        db: db.clone(),
        config: Arc::new(config),
        cipher,
        trust_proxy: Arc::new(TrustProxy::None),
        limiters: Arc::new(Limiters::default()),
        pool: Arc::new(ConnectionPool::new(false)),
        bots: Some(bots),
        activity: Arc::new(ActivityLog::new(db.clone())),
    };
    sqlx::query(r#"INSERT INTO "TsServerConfig" ("id", "name", "host", "webqueryPort", "apiKey", "useHttps", "createdAt", "updatedAt") VALUES (1, 'S', 'h', 1, 'k', 0, 0, 0)"#)
        .execute(db.pool())
        .await
        .unwrap();
    Api {
        app: teamhub_server::router(state),
        manager,
        db,
        admin: sign_access_token(SECRET, "15m", admin, "admin", "admin").unwrap(),
        viewer: sign_access_token(SECRET, "15m", viewer, "viewer", "viewer").unwrap(),
    }
}

impl Api {
    async fn call(&self, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        self.call_as(&self.admin, method, path, body).await
    }

    async fn call_as(&self, token: &str, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        let request = Request::builder().method(method).uri(path).header(header::CONTENT_TYPE, "application/json").header(header::AUTHORIZATION, format!("Bearer {token}"));
        let mut request = request.body(body.map_or_else(Body::empty, |body| Body::from(body.to_string()))).unwrap();
        request.extensions_mut().insert(ConnectInfo("9.9.9.9:1".parse::<SocketAddr>().unwrap()));
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    /// A bot that stays stopped.
    async fn add_bot(&self) -> i64 {
        let (status, body) = self
            .call(Method::POST, "/api/music-bots", Some(json!({"name": "Müzik", "serverConfigId": 1, "nickname": "Muzik Botu", "connectAfterCreate": false, "externalHost": "MyNick "})))
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        body["id"].as_i64().unwrap()
    }
}

#[tokio::test]
async fn only_admins_reach_music_bots() {
    let api = api().await;
    let (status, body) = api.call_as(&api.viewer, Method::GET, "/api/music-bots", None).await;
    assert_eq!((status, body), (StatusCode::FORBIDDEN, json!({"error": "Insufficient permissions"})));
}

#[tokio::test]
async fn a_new_bot_is_saved_with_a_private_identity() {
    let api = api().await;
    let id = api.add_bot().await;
    let row = api.db.music_bot(id).await.unwrap().unwrap();
    assert!(row.identity_data.unwrap().starts_with("enc:"), "stored encrypted");
    assert_eq!(row.external_host.as_deref(), Some("mynick"), "saved in one form");
    assert_eq!(row.volume, 50);

    let (_, list) = api.call(Method::GET, "/api/music-bots", None).await;
    assert_eq!(list[0]["status"], "stopped");
    assert_eq!(list[0]["serverConfig"]["name"], "S");
    assert_eq!(list[0]["nowPlaying"], Value::Null);
    assert_eq!(list[0]["automationSettings"]["enabled"], false);
    assert_eq!(list[0]["commandChannelIds"], json!([]));

    let (status, details) = api.call(Method::GET, &format!("/api/music-bots/{id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(details.get("identityData").is_none() && details.get("automationState").is_none(), "{details}");
    assert_eq!(details["playbackProgress"], Value::Null);
}

#[tokio::test]
async fn bad_bots_are_refused() {
    let api = api().await;
    for (body, problem) in [
        (json!({"name": "x"}), "name and serverConfigId are required"),
        (json!({"name": "x", "serverConfigId": 1, "nickname": "ab"}), "Bot nickname must be 3-30 characters (TeamSpeak limit)"),
        (json!({"name": "x", "serverConfigId": 1, "externalHost": "a b"}), "Enter a TeamSpeak address such as mynick, ts.example.com or 203.0.113.5:9987"),
        (json!({"name": "x", "serverConfigId": 99, "connectAfterCreate": false}), "Server config not found"),
    ] {
        let (status, answer) = api.call(Method::POST, "/api/music-bots", Some(body)).await;
        assert_eq!((status, answer["error"].as_str()), (StatusCode::BAD_REQUEST, Some(problem)));
    }
}

#[tokio::test]
async fn the_bot_limit_holds() {
    let api = api().await;
    api.db.set_setting("max_music_bots", "1").await.unwrap();
    api.add_bot().await;
    let (status, answer) = api.call(Method::POST, "/api/music-bots", Some(json!({"name": "x", "serverConfigId": 1, "connectAfterCreate": false}))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(answer["error"], "Music bot limit reached (1). Adjust the limit in Settings.");
}

#[tokio::test]
async fn settings_change_and_an_external_server_loses_command_channels() {
    let api = api().await;
    let id = api.add_bot().await;
    let path = format!("/api/music-bots/{id}");
    assert_eq!(api.call(Method::PUT, &path, Some(json!({"name": "Yeni", "volume": 20, "externalHost": null, "commandChannelIds": ["5", "x", "6"], "autoStart": true}))).await.0, StatusCode::OK);
    let row = api.db.music_bot(id).await.unwrap().unwrap();
    assert_eq!((row.name.as_str(), row.volume, row.external_host, row.auto_start), ("Yeni", 20, None, true));
    assert_eq!(row.command_channel_ids.as_deref(), Some(r#"["5","6"]"#));

    api.call(Method::PUT, &path, Some(json!({"externalHost": "ts.example.com:9988"}))).await;
    let row = api.db.music_bot(id).await.unwrap().unwrap();
    assert_eq!(row.external_host.as_deref(), Some("ts.example.com:9988"));
    assert_eq!(row.command_channel_ids, None);
    assert_eq!(api.call(Method::PUT, "/api/music-bots/999", Some(json!({"name": "x"}))).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_stopped_bot_refuses_to_play_and_answers_controls_quietly() {
    let api = api().await;
    let id = api.add_bot().await;
    let bot = format!("/api/music-bots/{id}");
    sqlx::query(r#"INSERT INTO "Song" ("id", "title", "filePath", "source", "serverConfigId", "createdAt") VALUES (1, 'One', '/f', 'local', 1, 0)"#).execute(api.db.pool()).await.unwrap();
    let (status, answer) = api.call(Method::POST, &format!("{bot}/play"), Some(json!({"songId": 1}))).await;
    assert_eq!((status, answer["error"].as_str()), (StatusCode::BAD_REQUEST, Some("Bot is not connected")));
    assert_eq!(api.call(Method::POST, &format!("{bot}/play"), Some(json!({}))).await.1["error"], "songId is required");
    assert_eq!(api.call(Method::POST, &format!("{bot}/play-url"), Some(json!({}))).await.1["error"], "url is required");
    assert_eq!(api.call(Method::POST, &format!("{bot}/play-radio"), Some(json!({}))).await.1["error"], "stationId is required");
    for control in ["pause", "resume", "skip", "previous", "seek", "stop-playback"] {
        assert_eq!(api.call(Method::POST, &format!("{bot}/{control}"), None).await.0, StatusCode::OK, "{control}");
    }
    let (status, _) = api.call(Method::POST, &format!("{bot}/join"), Some(json!({"cid": 5}))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(api.call(Method::POST, &format!("{bot}/join"), Some(json!({}))).await.1["error"], "Choose a channel");
    assert_eq!(api.call(Method::GET, &format!("{bot}/channels"), None).await.1, json!({"current": 0, "channels": []}));
    assert_eq!(api.call(Method::POST, &format!("{bot}/home"), None).await.0, StatusCode::CONFLICT);
    assert_eq!(api.call(Method::POST, "/api/music-bots/999/pause", None).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn volume_is_saved_while_stopped() {
    let api = api().await;
    let id = api.add_bot().await;
    let path = format!("/api/music-bots/{id}/volume");
    assert_eq!(api.call(Method::POST, &path, Some(json!({"volume": 0}))).await.1, json!({"success": true, "volume": 0}));
    assert_eq!(api.db.music_bot(id).await.unwrap().unwrap().volume, 0);
    assert_eq!(api.call(Method::POST, &path, Some(json!({"volume": "abc"}))).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(api.call(Method::GET, &format!("/api/music-bots/{id}/state"), None).await.1["volume"], 0);
}

#[tokio::test]
async fn the_queue_works_while_stopped_and_refuses_stale_positions() {
    let api = api().await;
    let id = api.add_bot().await;
    let queue = format!("/api/music-bots/{id}/queue");
    sqlx::query(r#"INSERT INTO "Song" ("id", "title", "filePath", "source", "serverConfigId", "createdAt") VALUES (1, 'One', '/1', 'local', 1, 0), (2, 'Two', '/2', 'local', 1, 0)"#).execute(api.db.pool()).await.unwrap();
    assert_eq!(api.call(Method::POST, &queue, Some(json!({"songId": 9}))).await.0, StatusCode::NOT_FOUND);
    assert_eq!(api.call(Method::POST, &queue, Some(json!({"songId": 1}))).await.1["queueLength"], 1);
    assert_eq!(api.call(Method::POST, &queue, Some(json!({"songId": 2}))).await.1["queueLength"], 2);

    let (_, listed) = api.call(Method::GET, &queue, None).await;
    assert_eq!(listed["items"][0]["title"], "One");
    assert_eq!(listed["items"][1]["filePath"], "/2");
    assert_eq!((listed["shuffle"].as_bool(), listed["repeat"].as_str()), (Some(false), Some("off")));

    assert_eq!(api.call(Method::PUT, &format!("{queue}/move"), Some(json!({"from": 0, "to": 1, "id": "2"}))).await.1["reason"], "queue_changed");
    assert_eq!(api.call(Method::PUT, &format!("{queue}/move"), Some(json!({"from": 0, "to": 1, "id": "1"}))).await.0, StatusCode::OK);
    assert_eq!(api.call(Method::GET, &queue, None).await.1["items"][0]["title"], "Two");
    assert_eq!(api.call(Method::PUT, &format!("{queue}/move"), Some(json!({"from": 0, "to": 9}))).await.1["error"], "Invalid indices");

    assert_eq!(api.call(Method::POST, &format!("{queue}/repeat"), Some(json!({"mode": "track"}))).await.1["repeat"], "track");
    assert_eq!(api.call(Method::POST, &format!("{queue}/repeat"), Some(json!({"mode": "x"}))).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(api.call(Method::POST, &format!("{queue}/shuffle"), Some(json!({"enabled": true}))).await.1["shuffle"], true);
    assert_eq!(api.call(Method::POST, &format!("{queue}/shuffle"), Some(json!({"enabled": false}))).await.1["shuffle"], false);

    assert_eq!(api.call(Method::DELETE, &format!("{queue}/0?id=wrong"), None).await.0, StatusCode::CONFLICT);
    assert_eq!(api.call(Method::DELETE, &format!("{queue}/0"), None).await.0, StatusCode::OK);
    assert_eq!(api.call(Method::GET, &queue, None).await.1["items"].as_array().unwrap().len(), 1);
    assert_eq!(api.call(Method::DELETE, &queue, None).await.0, StatusCode::OK);
    assert!(api.call(Method::GET, &queue, None).await.1["items"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn a_playlist_loads_whole() {
    let api = api().await;
    let id = api.add_bot().await;
    let path = format!("/api/music-bots/{id}/queue/playlist");
    sqlx::raw_sql(
        r#"INSERT INTO "Song" ("id", "title", "filePath", "source", "serverConfigId", "createdAt") VALUES (1, 'One', '', 'youtube', 1, 0), (2, 'Two', '', 'youtube', 1, 0);
           INSERT INTO "Playlist" ("id", "name", "mode", "serverConfigId", "createdAt") VALUES (1, 'P', 'x', 1, 0);
           INSERT INTO "PlaylistSong" ("playlistId", "songId", "position") VALUES (1, 2, 0), (1, 1, 1);"#,
    )
    .execute(api.db.pool())
    .await
    .unwrap();
    assert_eq!(api.call(Method::POST, &path, Some(json!({"playlistId": 9}))).await.0, StatusCode::NOT_FOUND);
    let (_, answer) = api.call(Method::POST, &path, Some(json!({"playlistId": 1}))).await;
    assert_eq!((answer["queueLength"].as_i64(), &answer["playError"]), (Some(2), &Value::Null));
    let (_, again) = api.call(Method::POST, &path, Some(json!({"playlistId": 1, "clearFirst": true}))).await;
    assert_eq!(again["queueLength"], 2, "cleared first");
    assert_eq!(again["playError"], "Bot is not connected", "autoplay follows clearFirst");
    assert_eq!(api.call(Method::GET, &format!("/api/music-bots/{id}/queue"), None).await.1["items"][0]["title"], "Two");
}

#[tokio::test]
async fn auto_dj_settings_are_checked_and_audited() {
    let api = api().await;
    let id = api.add_bot().await;
    let path = format!("/api/music-bots/{id}/automation");
    let settings = |source: Value, enabled: bool| json!({"enabled": enabled, "source": source, "requireListeners": false, "quietPeriodSeconds": 60, "retryBaseSeconds": 30, "retryMaxSeconds": 60, "stayInChannel": true});
    assert_eq!(api.call(Method::PUT, &path, Some(json!({"enabled": true}))).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(api.call(Method::PUT, &path, Some(settings(Value::Null, true))).await.0, StatusCode::BAD_REQUEST, "enabled needs a source");
    let missing = api.call(Method::PUT, &path, Some(settings(json!({"kind": "radio", "id": 5}), true))).await;
    assert_eq!(missing.1["error"], "Choose a radio station from this bot’s server");
    sqlx::query(r#"INSERT INTO "RadioStation" ("id", "name", "url", "serverConfigId", "createdAt") VALUES (5, 'R', 'http://r', 1, 0)"#).execute(api.db.pool()).await.unwrap();
    let (status, saved) = api.call(Method::PUT, &path, Some(settings(json!({"kind": "radio", "id": 5}), true))).await;
    assert_eq!((status, saved["source"]["id"].as_i64()), (StatusCode::OK, Some(5)));
    let (_, list) = api.call(Method::GET, "/api/music-bots", None).await;
    assert_eq!(list[0]["automationSettings"]["enabled"], true);
    let events = api.db.audit_events(10).await.unwrap();
    assert!(events.iter().any(|event| event.action == "music_bot.automation_update" && event.outcome == "success"));
    let (_, media) = api.call(Method::GET, "/api/music-bots/media", None).await;
    assert_eq!(media[0]["automation"]["sourceKind"], "radio");
    assert_eq!(media[0]["automation"]["stayInChannel"], true);
    assert_eq!(media[0]["serverName"], "S");
}

#[tokio::test]
async fn commands_can_be_switched_off_per_bot() {
    let api = api().await;
    let id = api.add_bot().await;
    let path = format!("/api/music-bots/{id}/extensions");
    assert_eq!(api.call(Method::PUT, &path, Some(json!({"disabledCommands": ["next", "skip", "ara"]}))).await.1, json!({"disabledCommands": ["skip", "ara"]}));
    assert_eq!(api.call(Method::PUT, &path, Some(json!({"disabledCommands": ["Bad Name"]}))).await.0, StatusCode::BAD_REQUEST);
    let (_, shown) = api.call(Method::GET, &path, None).await;
    assert_eq!(shown["disabledCommands"], json!(["skip", "ara"]));
    let builtin = shown["builtin"].as_array().unwrap();
    assert_eq!(builtin.iter().find(|line| line["name"] == "next").unwrap()["canonical"], "skip");
}

#[tokio::test]
async fn panel_actions_are_in_the_activity_list() {
    let api = api().await;
    let id = api.add_bot().await;
    api.call(Method::POST, &format!("/api/music-bots/{id}/pause"), None).await;
    api.call(Method::POST, &format!("/api/music-bots/{id}/volume"), Some(json!({"volume": 30}))).await;
    let (_, entries) = api.call(Method::GET, &format!("/api/music-bots/{id}/activity"), None).await;
    assert_eq!(entries[0]["action"], "!vol 30");
    assert_eq!(entries[1]["action"], "!pause");
    assert_eq!((entries[0]["via"].as_str(), entries[0]["who"].as_str()), (Some("panel"), Some("admin")));
}

#[tokio::test]
async fn a_bot_can_be_deleted() {
    let api = api().await;
    let id = api.add_bot().await;
    assert_eq!(api.call(Method::DELETE, &format!("/api/music-bots/{id}"), None).await.0, StatusCode::OK);
    assert!(api.db.music_bot(id).await.unwrap().is_none());
    assert_eq!(api.call(Method::GET, &format!("/api/music-bots/{id}"), None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(api.call(Method::GET, &format!("/api/music-bots/{id}/queue"), None).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn video_says_it_is_not_streaming() {
    let api = api().await;
    let id = api.add_bot().await;
    assert_eq!(api.call(Method::GET, &format!("/api/music-bots/{id}/stream/status"), None).await.1["streaming"], false);
}

// ─── Radio stations ────────────────────────────────────────────

const RADIO: &str = "/api/servers/1/radio-stations";

#[tokio::test]
async fn radio_stations_can_be_managed() {
    let api = api().await;
    let (status, body) = api.call_as(&api.viewer, Method::GET, RADIO, None).await;
    assert_eq!((status, body["error"].as_str()), (StatusCode::FORBIDDEN, Some("Insufficient permissions")));
    assert_eq!(api.call(Method::POST, RADIO, Some(json!({"name": "x"}))).await.1["error"], "name and url are required");
    let (status, body) = api.call(Method::POST, RADIO, Some(json!({"name": "x", "url": "http://127.0.0.1/s"}))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().starts_with("Invalid URL: "), "{body}");
    assert_eq!(api.call(Method::POST, "/api/servers/9/radio-stations", Some(json!({"name": "x", "url": "https://1.1.1.1/s"}))).await.0, StatusCode::NOT_FOUND);

    let (status, made) = api.call(Method::POST, RADIO, Some(json!({"name": "B", "url": "https://1.1.1.1/s", "genre": "", "imageUrl": "https://1.1.1.1/i"}))).await;
    assert_eq!((status, &made["genre"], made["imageUrl"].as_str()), (StatusCode::CREATED, &Value::Null, Some("https://1.1.1.1/i")));
    let id = made["id"].as_i64().unwrap();
    api.call(Method::POST, RADIO, Some(json!({"name": "A", "url": "https://1.1.1.1/a"}))).await;
    let (_, listed) = api.call(Method::GET, RADIO, None).await;
    assert_eq!((listed[0]["name"].as_str(), listed[1]["name"].as_str()), (Some("A"), Some("B")), "by name");

    let path = format!("{RADIO}/{id}");
    let (_, changed) = api.call(Method::PUT, &path, Some(json!({"name": " C ", "genre": "Pop"}))).await;
    assert_eq!((changed["name"].as_str(), changed["genre"].as_str(), changed["url"].as_str()), (Some("C"), Some("Pop"), Some("https://1.1.1.1/s")));
    assert_eq!(api.call(Method::PUT, &path, Some(json!({"name": " "}))).await.1["error"], "name is required");
    assert_eq!(api.call(Method::PUT, &path, Some(json!({"genre": 1}))).await.1["error"], "genre must be a string or null");
    assert_eq!(api.call(Method::PUT, &path, Some(json!({"genre": null}))).await.1["genre"], Value::Null);
    assert_eq!(api.call(Method::PUT, &format!("{RADIO}/999"), Some(json!({}))).await.0, StatusCode::NOT_FOUND);

    assert_eq!(api.call(Method::DELETE, &format!("/api/servers/9/radio-stations/{id}"), None).await.0, StatusCode::NOT_FOUND, "another connection's station stays");
    assert_eq!(api.call(Method::DELETE, &path, None).await.1, json!({"success": true}));
    assert_eq!(api.call(Method::DELETE, &path, None).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn radio_ids_really_get_compacted() {
    let api = api().await;
    let mut ids = Vec::new();
    for name in ["A", "B", "C"] {
        ids.push(api.call(Method::POST, RADIO, Some(json!({"name": name, "url": format!("https://1.1.1.1/{name}")}))).await.1["id"].as_i64().unwrap());
    }
    api.call(Method::DELETE, &format!("{RADIO}/{}", ids[0]), None).await;
    let (_, answer) = api.call(Method::POST, &format!("{RADIO}/reset-ids"), None).await;
    let stations = answer["stations"].as_array().unwrap();
    assert_eq!(stations.len(), 2);
    assert_eq!((stations[0]["id"].as_i64(), stations[0]["name"].as_str()), (Some(1), Some("B")));
    assert_eq!(stations[1]["id"], 2);
}

#[tokio::test]
async fn radio_presets_and_search_rules() {
    let api = api().await;
    let (_, presets) = api.call(Method::GET, &format!("{RADIO}/presets"), None).await;
    let presets = presets.as_array().unwrap();
    assert_eq!(presets.len(), 25, "the shared station is listed once");
    assert_eq!(presets[0]["name"], "Kral FM");
    assert_eq!(api.call(Method::GET, &format!("{RADIO}/browse?q=%20"), None).await.1["error"], "Provide a search query, tag, or country code");
}

#[tokio::test]
async fn a_new_connection_gets_stations_and_two_bots_once() {
    let api = api().await;
    teamhub_server::bootstrap::seed_server(&api.db, Some(&api.manager), 1).await;
    let stations = api.db.radio_stations(1).await.unwrap();
    assert_eq!(stations.len(), 10);
    let bots = api.db.music_bots().await.unwrap();
    assert_eq!(bots.iter().map(|bot| bot.name.as_str()).collect::<Vec<_>>(), ["Müzik Botu", "Radyo Botu"]);
    assert!(bots.iter().all(|bot| bot.auto_start));
    let radio: Value = serde_json::from_str(bots[1].automation_settings.as_deref().unwrap()).unwrap();
    assert_eq!((radio["enabled"].as_bool(), radio["source"]["kind"].as_str(), radio["quietPeriodSeconds"].as_i64()), (Some(true), Some("radio"), Some(30)));
    let music: Value = serde_json::from_str(bots[0].automation_settings.as_deref().unwrap()).unwrap();
    assert_eq!((music["enabled"].as_bool(), music["stayInChannel"].as_bool()), (Some(false), Some(true)));

    // Deleted on purpose: they do not come back.
    let first = stations[0].id;
    api.call(Method::DELETE, &format!("{RADIO}/{first}"), None).await;
    api.call(Method::DELETE, &format!("/api/music-bots/{}", bots[0].id), None).await;
    teamhub_server::bootstrap::seed_server(&api.db, Some(&api.manager), 1).await;
    assert_eq!(api.db.radio_stations(1).await.unwrap().len(), 9);
    assert_eq!(api.db.music_bots().await.unwrap().len(), 1);
    api.manager.stop_all().await;
}

// ─── Chat commands ─────────────────────────────────────────────

const COMMANDS: &str = "/api/servers/1/chat-commands";

#[tokio::test]
async fn custom_commands_can_be_managed() {
    let api = api().await;
    assert_eq!(api.call_as(&api.viewer, Method::GET, COMMANDS, None).await.0, StatusCode::FORBIDDEN);
    assert_eq!(api.call(Method::POST, COMMANDS, Some(json!({"response": "x"}))).await.1["error"], "name is required (letters, numbers, hyphens, underscores; no \"!\")");
    assert_eq!(api.call(Method::POST, COMMANDS, Some(json!({"name": "!Play", "response": "x"}))).await.1["error"], "\"!play\" is a built-in command and cannot be overridden");
    assert_eq!(api.call(Method::POST, COMMANDS, Some(json!({"name": "x"}))).await.1["error"], "response is required");
    assert_eq!(api.call(Method::POST, COMMANDS, Some(json!({"name": "x", "response": "y".repeat(901)}))).await.1["error"], "response must be at most 900 characters");

    let (status, made) = api.call(Method::POST, COMMANDS, Some(json!({"name": "!Kural", "response": " kurallar ", "description": "d".repeat(200)}))).await;
    assert_eq!((status, made["name"].as_str(), made["response"].as_str(), made["enabled"].as_bool()), (StatusCode::CREATED, Some("kural"), Some("kurallar"), Some(true)));
    assert_eq!(made["description"].as_str().unwrap().chars().count(), 120);
    assert_eq!(api.call(Method::POST, COMMANDS, Some(json!({"name": "kural", "response": "x"}))).await, (StatusCode::CONFLICT, json!({"error": "Command \"!kural\" already exists"})));
    let path = format!("{COMMANDS}/{}", made["id"]);

    let (_, off) = api.call(Method::PUT, &path, Some(json!({"enabled": false, "description": null}))).await;
    assert_eq!((off["enabled"].as_bool(), &off["description"], off["response"].as_str()), (Some(false), &Value::Null, Some("kurallar")));
    api.call(Method::POST, COMMANDS, Some(json!({"name": "other", "response": "x"}))).await;
    assert_eq!(api.call(Method::PUT, &path, Some(json!({"name": "other"}))).await.0, StatusCode::CONFLICT);
    assert_eq!(api.call(Method::PUT, "/api/servers/9/chat-commands/1", Some(json!({}))).await.1["error"], "Chat command not found");
    assert_eq!(api.call(Method::GET, COMMANDS, None).await.1.as_array().unwrap().len(), 2);
    assert_eq!(api.call(Method::DELETE, &path, None).await.1, json!({"success": true}));
    assert_eq!(api.call(Method::DELETE, &path, None).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn command_templates_are_added_switched_off() {
    let api = api().await;
    api.call(Method::POST, COMMANDS, Some(json!({"name": "rules", "response": "mine"}))).await;
    assert_eq!(api.call(Method::GET, &format!("{COMMANDS}/presets"), None).await.1.as_array().unwrap().len(), 5);
    let (_, seeded) = api.call(Method::POST, &format!("{COMMANDS}/seed-presets"), None).await;
    assert_eq!(seeded["created"], 4);
    assert_eq!(seeded["createdNames"], json!(["links", "discord", "info", "about"]));
    let commands = seeded["commands"].as_array().unwrap();
    assert_eq!(commands.iter().find(|command| command["name"] == "rules").unwrap()["response"], "mine", "an existing one is kept");
    assert_eq!(commands.iter().find(|command| command["name"] == "info").unwrap()["enabled"], false);
    assert_eq!(api.call(Method::POST, &format!("{COMMANDS}/seed-presets"), None).await.1["created"], 0);
}

#[tokio::test]
async fn media_command_access_is_per_virtual_server() {
    let api = api().await;
    let path = format!("{COMMANDS}/permissions/1");
    let open = json!({"playback": {"mode": "everyone"}, "queue": {"mode": "everyone"}, "video": {"mode": "everyone"}});
    assert_eq!(api.call(Method::GET, &path, None).await.1, open);
    assert_eq!(api.call(Method::GET, &format!("{COMMANDS}/permissions/0"), None).await.1["error"], "A valid server connection and virtual server are required");
    assert_eq!(api.call(Method::PUT, &path, Some(json!({"playback": {"mode": "everyone"}}))).await.0, StatusCode::BAD_REQUEST);
    let policy = json!({"playback": {"mode": "server_groups", "serverGroupIds": [6, 6, 7]}, "queue": {"mode": "everyone"}, "video": {"mode": "server_groups", "serverGroupIds": []}});
    let (_, saved) = api.call(Method::PUT, &path, Some(policy)).await;
    assert_eq!(saved["playback"]["serverGroupIds"], json!([6, 7]));
    assert_eq!(api.call(Method::GET, &path, None).await.1, saved);
    assert_eq!(api.call(Method::GET, &format!("{COMMANDS}/permissions/2"), None).await.1, open, "another virtual server has its own");
    // A damaged saved policy closes the commands rather than opening them.
    api.db.set_setting("media_command_permissions:1:3", "bozuk").await.unwrap();
    assert_eq!(api.call(Method::GET, &format!("{COMMANDS}/permissions/3"), None).await.1["playback"]["serverGroupIds"], json!([]));
}

// ─── Request history ───────────────────────────────────────────

const REQUESTS: &str = "/api/servers/1/music-requests";

async fn add_request(api: &Api, id: i64, bot: i64, url: &str, source: &str) {
    sqlx::query(r#"INSERT INTO "MusicRequest" ("id", "title", "url", "source", "status", "serverConfigId", "musicBotId", "requestedAt") VALUES (?, 'T', ?, ?, 'available', 1, ?, ?)"#)
        .bind(id)
        .bind(url)
        .bind(source)
        .bind(bot)
        .bind(id * 1000)
        .execute(api.db.pool())
        .await
        .unwrap();
}

#[tokio::test]
async fn the_request_list_shows_no_addresses_and_loses_gone_bots() {
    let api = api().await;
    let bot = api.add_bot().await;
    add_request(&api, 1, 0, "https://secret.example/old", "url").await;
    add_request(&api, 2, bot, "https://secret.example/mine", "url").await;
    add_request(&api, 3, 77, "https://secret.example/gone", "url").await;
    assert_eq!(api.call_as(&api.viewer, Method::GET, REQUESTS, None).await.1["error"], "No access to this server");
    assert_eq!(api.call(Method::GET, &format!("{REQUESTS}?botId=x"), None).await.1["error"], "Invalid bot id");
    let (status, rows) = api.call(Method::GET, REQUESTS, None).await;
    assert_eq!(status, StatusCode::OK);
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.iter().map(|row| row["id"].as_i64().unwrap()).collect::<Vec<_>>(), [2, 1], "newest first, the deleted bot's row is gone");
    assert!(!Value::Array(rows.clone()).to_string().contains("secret"));
    assert_eq!(rows[0]["requestedAt"], "1970-01-01T00:00:02.000Z");
    assert_eq!(api.call(Method::GET, &format!("{REQUESTS}?botId={bot}"), None).await.1.as_array().unwrap().len(), 2, "the bot's and the old rows");
}

#[tokio::test]
async fn replays_are_checked() {
    let api = api().await;
    let bot = api.add_bot().await;
    add_request(&api, 1, bot, "not-an-address", "url").await;
    add_request(&api, 2, bot, "https://1.1.1.1/radio", "radio").await;
    let replay = |id: i64| format!("{REQUESTS}/{id}/replay");
    assert_eq!(api.call_as(&api.viewer, Method::POST, &replay(1), Some(json!({"botId": bot}))).await.0, StatusCode::FORBIDDEN);
    for body in [json!({}), json!({"botId": "1"}), json!({"botId": bot, "enqueue": "yes"})] {
        assert_eq!(api.call(Method::POST, &replay(1), Some(body)).await.1["error"], "Invalid replay request");
    }
    assert_eq!(api.call(Method::POST, &replay(9), Some(json!({"botId": bot}))).await.1["error"], "Request not found");
    assert_eq!(api.call(Method::POST, &replay(1), Some(json!({"botId": 999}))).await.1["error"], "Music bot not found");
    assert_eq!(api.call(Method::POST, &replay(2), Some(json!({"botId": bot}))).await.1["error"], "Bot is not connected");

    let (status, answer) = api.call(Method::POST, &replay(1), Some(json!({"botId": bot}))).await;
    assert_eq!((status, answer["error"].as_str()), (StatusCode::GONE, Some("This media is no longer available.")));
    let (_, rows) = api.call(Method::GET, REQUESTS, None).await;
    assert_eq!(rows.as_array().unwrap().iter().find(|row| row["id"] == 1).unwrap()["status"], "unavailable");
}
