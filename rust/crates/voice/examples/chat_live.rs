//! Try the chat commands on a real TeamSpeak server: a bot answers, and a second client types
//! into the channel and reads the answers.
//!
//! cargo run -p teamhub-voice --example chat_live -- <host> <port> <radio url> <music dir> [youtube url]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use teamhub_db::Db;
use teamhub_media::cache::MediaCache;
use teamhub_media::youtube::YtDlpOptions;
use teamhub_ts_proto::{ClientOptions, Event, Identity, TsClient};
use teamhub_voice::automation::{AutomationController, AutomationSettings, AutomationTarget};
use teamhub_voice::chat::format::AutomationSource;
use teamhub_voice::chat::service::{BotEntry, ChatService, RealTracks};
use teamhub_voice::{BotConfig, Media, VoiceBot};

const BOT_ID: i64 = 5;

async fn identity() -> Result<Identity, Box<dyn std::error::Error>> {
    Ok(tokio::task::spawn_blocking(|| Identity::generate(8)).await??)
}

/// Type a line and print what comes back within `wait`.
async fn say(user: &TsClient, events: &mut tokio::sync::mpsc::UnboundedReceiver<Event>, line: &str, wait: Duration) {
    println!("\n> {line}");
    user.send_text(line, 2, None);
    let deadline = tokio::time::sleep(wait);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            _ = &mut deadline => return,
            event = events.recv() => match event {
                Some(Event::TextMessage(params)) => {
                    if params.get("invokername").is_some_and(|name| name != "live-kullanici") {
                        let text = params.get("msg").cloned().unwrap_or_default();
                        let short: String = text.chars().take(240).collect();
                        println!("< [{}] {}", params.get("invokername").map_or("?", String::as_str), short.replace('\n', " / "));
                    }
                }
                Some(Event::Command(command)) if command.name == "notifyclientupdated" => {
                    if let Some(name) = command.params.get("client_nickname") {
                        println!("  (isim: {name})");
                    }
                }
                Some(Event::Disconnected) | None => return,
                _ => {}
            },
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [host, port, radio, music, ..] = args.as_slice() else {
        return Err("usage: chat_live <host> <port> <radio url> <music dir> [youtube url]".into());
    };
    let port: u16 = port.parse()?;
    let music_dir = std::path::PathBuf::from(music);
    std::fs::create_dir_all(&music_dir)?;

    let db = Db::open_in_memory().await?;
    sqlx::query(
        r#"INSERT INTO "TsServerConfig" ("id", "name", "host", "webqueryPort", "apiKey", "useHttps", "createdAt", "updatedAt") VALUES (1, 's', 'h', 1, 'k', 0, 0, 0)"#,
    )
    .execute(db.pool())
    .await?;
    sqlx::query(r#"INSERT INTO "MusicBot" ("id", "name", "serverConfigId", "createdAt", "updatedAt") VALUES (5, 'Müzik', 1, 0, 0)"#).execute(db.pool()).await?;
    sqlx::query(r#"INSERT INTO "RadioStation" ("id", "name", "url", "genre", "serverConfigId", "createdAt") VALUES (1, 'Deneme Radyo', ?, 'Ambient', 1, 0)"#)
        .bind(radio)
        .execute(db.pool())
        .await?;

    let cache = MediaCache::new(&music_dir);
    let yt_dlp = YtDlpOptions::default();
    let bot = Arc::new(
        VoiceBot::start(BotConfig {
            host: host.clone(),
            port,
            identity: identity().await?,
            nickname: "rust-chat-bot".to_owned(),
            server_password: None,
            default_channel: None,
            channel_password: None,
            volume: 100,
            media: Media { cache: cache.clone(), yt_dlp: yt_dlp.clone() },
            queue: Arc::default(),
        })
        .await?,
    );
    let service = ChatService::new(db.clone(), Arc::new(RealTracks { cache, yt_dlp, spotify: None }), music_dir);
    let settings = AutomationSettings {
        enabled: true,
        source: Some(AutomationTarget { kind: AutomationSource::Radio, id: 1 }),
        quiet_period_seconds: 3,
        ..AutomationSettings::default()
    };
    let automation = Arc::new(Mutex::new(AutomationController::new(settings, None)));
    let entry = BotEntry {
        bot: bot.clone(),
        server_config_id: 1,
        virtual_server_id: 1,
        external: false,
        name: "Müzik".to_owned(),
        nickname: "rust-chat-bot".to_owned(),
        automation: Some(automation.clone()),
    };
    service.attach(BOT_ID, entry.clone(), bot.subscribe_events());

    let (user, mut events) = TsClient::connect(ClientOptions {
        host: host.clone(),
        port,
        identity: identity().await?,
        nickname: "live-kullanici".to_owned(),
        server_password: None,
        default_channel: None,
        channel_password: None,
    })
    .await?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    println!("bot clid={} channel={}", bot.client().client_id(), bot.client().current_channel());
    println!("user clid={} channel={}", user.client_id(), user.current_channel());

    // Slow enough for the server's anti-flood; `!help` is many messages, so it goes last.
    let short = Duration::from_millis(3000);
    say(&user, &mut events, "!np", short).await;
    say(&user, &mut events, "!plya", short).await;
    say(&user, &mut events, "!radio", short).await;
    say(&user, &mut events, "!radio 1", Duration::from_secs(6)).await;
    say(&user, &mut events, "!np", short).await;
    say(&user, &mut events, "!vol 50", short).await;
    say(&user, &mut events, "!pause", short).await;
    say(&user, &mut events, "!pause", Duration::from_secs(3)).await;
    println!("  durum: {:?} kare={} ", bot.snapshot().status, bot.snapshot().frames_sent);
    say(&user, &mut events, "!stop", short).await;
    say(&user, &mut events, "!history", short).await;
    if let Some(url) = args.get(4) {
        say(&user, &mut events, &format!("!play {url}"), Duration::from_secs(40)).await;
        say(&user, &mut events, "!np", short).await;
        say(&user, &mut events, "!seek +30", Duration::from_secs(3)).await;
        println!("  durum: {:?} konum={:.0}", bot.snapshot().status, bot.snapshot().position_seconds);
        say(&user, &mut events, "!queue", short).await;
        say(&user, &mut events, "!lyrics", Duration::from_secs(6)).await;
        say(&user, &mut events, "!stop", short).await;
    }

    println!("\n--- Auto-DJ: sessizlik süresi 3 sn, sonra kendi radyoyu açmalı");
    service.run_automation(BOT_ID, entry, bot.subscribe_events());
    say(&user, &mut events, "!np", Duration::from_secs(8)).await;
    println!("  durum: {:?} autodj_aktif={} kare={}", bot.snapshot().status, automation.lock().expect("lock").active(), bot.snapshot().frames_sent);
    say(&user, &mut events, "!stop", Duration::from_secs(1)).await;
    println!("  durum: {:?} autodj_aktif={}", bot.snapshot().status, automation.lock().expect("lock").active());
    say(&user, &mut events, "!help", short).await;

    user.disconnect();
    let _ = tokio::time::timeout(Duration::from_secs(1), events.recv()).await;
    bot.disconnect().await;
    println!("\nbitti");
    Ok(())
}
