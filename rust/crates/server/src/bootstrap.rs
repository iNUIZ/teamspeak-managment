//! What a new connection comes with: Turkish and English radio stations and two ready bots.
//! Both run once per connection, so nothing deleted on purpose comes back.

use std::sync::Arc;

use teamhub_db::Db;
use teamhub_voice::manager::BotManager;

/// Name, address, kind.
pub const DEFAULT_RADIO_STATIONS: [(&str, &str, &str); 10] = [
    ("Kral FM", "https://dygedge2.radyotvonline.net/kralfm/playlist.m3u8", "TR · Arabesk"),
    ("Kral Pop", "https://dygedge.radyotvonline.net/kralpop/playlist.m3u8", "TR · Pop"),
    ("PowerTürk", "https://listen.powerapp.com.tr/powerturk/mpeg/icecast.audio", "TR · Pop"),
    ("SlowTürk", "https://radyo.duhnet.tv/slowturk", "TR · Slow"),
    ("Radyo D", "https://radyo.duhnet.tv/radyod", "TR · Pop"),
    ("Heart UK", "https://media-ice.musicradio.com/HeartUKMP3", "EN · Pop"),
    ("Capital UK", "https://media-ice.musicradio.com/CapitalUKMP3", "EN · Hits"),
    ("Smooth UK", "https://media-ice.musicradio.com/SmoothUKMP3", "EN · Easy listening"),
    ("Classic FM", "https://media-ice.musicradio.com/ClassicFMMP3", "EN · Classical"),
    ("LBC UK", "https://media-ice.musicradio.com/LBCUKMP3", "EN · Talk"),
];

/// A failure here never stops the program or a server being added.
pub async fn seed_server(db: &Db, bots: Option<&Arc<BotManager>>, server_config_id: i64) {
    if let Err(error) = db.seed_radio_stations(server_config_id, &DEFAULT_RADIO_STATIONS).await {
        tracing::warn!(server_config_id, %error, "Default radio stations could not be added");
    }
    if let Some(bots) = bots {
        if let Err(error) = bots.seed_default_bots(server_config_id).await {
            tracing::warn!(server_config_id, %error, "Default bots could not be added");
        }
    }
}

/// Every real connection at startup.
pub async fn seed_all(db: &Db, bots: &Arc<BotManager>) {
    let servers = match db.servers().await {
        Ok(servers) => servers,
        Err(error) => {
            tracing::warn!(%error, "Connections could not be read for default content");
            return;
        }
    };
    for server in servers.iter().filter(|server| !server.is_demo) {
        seed_server(db, Some(bots), server.id).await;
    }
}
