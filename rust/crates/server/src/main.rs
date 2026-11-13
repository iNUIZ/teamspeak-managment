//! `teamhub`: the single program that replaces Node, nginx, supervisor and the Go sidecar.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use teamhub_core::{Config, FieldCipher};
use teamhub_db::Db;
use teamhub_media::cache::MediaCache;
use teamhub_media::links::SpotifyCatalog;
use teamhub_server::routes::music_bots::{yt_dlp_options, PoolPorts};
use teamhub_voice::activity::ActivityLog;
use teamhub_voice::chat::service::{ChatService, RealTracks};
use teamhub_voice::manager::BotManager;
use teamhub_server::client_ip::parse_trust_proxy;
use teamhub_server::pool::ConnectionPool;
use teamhub_server::state::{AppState, Limiters};

const DEV_JWT_SECRET: &str = "dev-secret-change-me-in-production";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let config = Config::from_env()?;
    if config.jwt_secret == DEV_JWT_SECRET {
        if config.production {
            return Err("JWT_SECRET is set to the default value. Set a secure JWT_SECRET before running in production.".into());
        }
        tracing::warn!("JWT_SECRET is using the default development value. Set JWT_SECRET in production!");
    }
    let trust_proxy = parse_trust_proxy(config.trust_proxy.as_deref())?;
    let db = Db::open(&config.database_url).await?;
    let cipher = FieldCipher::new(&config.encryption_source)?;
    let port = config.port;
    let pool = ConnectionPool::new(config.ts_allow_self_signed);
    if let Err(error) = pool.initialize(&db, &cipher).await {
        tracing::warn!(error = %error.message, "Server connections could not be loaded");
    }
    let pool = Arc::new(pool);
    let cipher = Arc::new(cipher);

    let music_dir = PathBuf::from(std::env::var("MUSIC_DIR").unwrap_or_else(|_| "/data/music".to_owned()));
    if let Err(error) = std::fs::create_dir_all(&music_dir) {
        tracing::warn!(%error, "The music folder could not be created");
    }
    let cache = MediaCache::new(&music_dir);
    let yt_dlp = yt_dlp_options();
    let spotify = SpotifyCatalog::from_env().map(Arc::new);
    let tracks = Arc::new(RealTracks { cache: cache.clone(), yt_dlp: yt_dlp.clone(), spotify });
    let chat = ChatService::new(db.clone(), tracks, music_dir);
    let bots = BotManager::new(db.clone(), cipher.clone(), cache.clone(), yt_dlp, chat);
    bots.set_port_lookup(Arc::new(PoolPorts(pool.clone())));
    // Leftovers of interrupted downloads: now and every hour.
    tokio::spawn(async move {
        let mut hourly = tokio::time::interval(std::time::Duration::from_secs(60 * 60));
        loop {
            hourly.tick().await;
            if let Err(error) = cache.sweep() {
                tracing::warn!(%error, "The download cache could not be swept");
            }
        }
    });
    bots.load_all().await.map_err(|error| error.to_string())?;
    teamhub_server::bootstrap::seed_all(&db, &bots).await;
    let closing_db = db.clone();

    let activity = Arc::new(ActivityLog::new(db.clone()));
    let state = AppState {
        db,
        config: Arc::new(config),
        cipher,
        trust_proxy: Arc::new(trust_proxy),
        limiters: Arc::new(Limiters::default()),
        pool,
        bots: Some(bots.clone()),
        activity,
    };
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!("teamhub listening on {}", listener.local_addr()?);
    axum::serve(listener, teamhub_server::router(state).into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    // Leave TeamSpeak properly, so the bots do not linger there as ghosts.
    bots.stop_all().await;
    closing_db.close().await;
    // A new bot's identity may still be calculating, and it is not worth waiting for: the bot
    // keeps the one it has and the stronger one is made again when needed.
    std::process::exit(0);
}

/// Ctrl-C, or the stop signal Docker and systemd send.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        match signal(SignalKind::terminate()) {
            Ok(mut terminate) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = terminate.recv() => {}
                }
                return;
            }
            Err(error) => tracing::warn!(%error, "The stop signal cannot be watched"),
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}
