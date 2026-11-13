//! A connection's radio stations. Admins only.

use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde_json::{json, Value};
use teamhub_db::library::NewStation;
use teamhub_db::models::RadioStation;
use teamhub_media::address::validate_url;
use teamhub_media::radio_browser::{self, SearchQuery};

use super::text_field;
use super::ts::FlexJson;
use crate::auth::AdminUser;
use crate::bootstrap::DEFAULT_RADIO_STATIONS;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

type Ids = Path<HashMap<String, String>>;

/// More to choose from beyond the stations every connection starts with.
const MORE_PRESETS: [(&str, &str, &str); 16] = [
    ("1LIVE", "https://wdr-1live-live.icecast.wdr.de/wdr/1live/live/mp3/128/stream.mp3", "Pop/Rock"),
    ("WDR 2", "https://wdr-wdr2-rheinland.icecast.wdr.de/wdr/wdr2/rheinland/mp3/128/stream.mp3", "Pop"),
    ("SWR3", "https://liveradio.swr.de/sw282p3/swr3/play.mp3", "Pop"),
    ("BigFM", "https://streams.bigfm.de/bigfm-deutschland-128-mp3", "Pop/Dance"),
    ("Technobase.FM", "https://listen.technobase.fm/tunein-mp3-pls", "Techno/Dance"),
    ("HardBase.FM", "https://listen.hardbase.fm/tunein-mp3-pls", "Hardstyle"),
    ("HouseTime.FM", "https://listen.housetime.fm/tunein-mp3-pls", "House"),
    ("TranceBase.FM", "https://listen.trancebase.fm/tunein-mp3-pls", "Trance"),
    ("Lofi Hip Hop", "https://play.streamafrica.net/lofiradio", "Lofi/Chill"),
    ("BBC Radio 1", "http://stream.live.vc.bbcmedia.co.uk/bbc_radio_one", "Pop/Chart"),
    ("Classic FM", "https://media-ice.musicradio.com/ClassicFMMP3", "Classical"),
    ("Absolute Radio", "https://ais-sa5.cdnstream1.com/b75154_128mp3", "Rock"),
    ("Jazz Radio", "http://jazz-wr04.ice.infomaniak.ch/jazz-wr04-128.mp3", "Jazz"),
    ("Sunshine Live", "https://stream.sunshine-live.de/live/mp3-192/stream.sunshine-live.de/", "Electronic"),
    ("Radio BOB!", "https://streams.radiobob.de/bob-live/mp3-192/mediaplayer", "Rock"),
    ("Deutschlandfunk", "https://st01.dlf.de/dlf/01/128/mp3/stream.mp3", "News/Culture"),
];

pub(crate) fn config_id(path: &HashMap<String, String>) -> ApiResult<i64> {
    path.get("configId").and_then(|id| id.parse().ok()).filter(|id| *id > 0).ok_or_else(|| ApiError::new(400, "Invalid server config ID"))
}

fn row_id(path: &HashMap<String, String>, missing: &'static str) -> ApiResult<i64> {
    path.get("id").and_then(|id| id.parse().ok()).ok_or_else(|| ApiError::new(404, missing))
}

fn station_json(station: &RadioStation) -> Value {
    json!({
        "id": station.id,
        "name": station.name,
        "url": station.url,
        "genre": station.genre,
        "imageUrl": station.image_url,
        "serverConfigId": station.server_config_id,
        "createdAt": station.created_at.to_iso(),
    })
}

async fn check_address(url: &str) -> ApiResult<()> {
    validate_url(url).await.map(|_| ()).map_err(|error| ApiError::new(400, format!("Invalid URL: {}", error.0)))
}

async fn presets(_admin: AdminUser) -> Json<Value> {
    let builtin = DEFAULT_RADIO_STATIONS.iter();
    let more = MORE_PRESETS.iter().filter(|preset| !DEFAULT_RADIO_STATIONS.iter().any(|station| station.0 == preset.0));
    Json(builtin.chain(more).map(|(name, url, genre)| json!({"name": name, "url": url, "genre": genre})).collect())
}

async fn browse(_admin: AdminUser, Query(q): Query<HashMap<String, String>>) -> ApiResult<Json<Value>> {
    let field = |name: &str| q.get(name).cloned().unwrap_or_default();
    let query = SearchQuery { name: field("q"), tag: field("tag"), country: field("country"), limit: q.get("limit").and_then(|limit| limit.parse().ok()) };
    if query.name.trim().is_empty() && query.tag.trim().is_empty() && query.country.trim().is_empty() {
        return Err(ApiError::new(400, "Provide a search query, tag, or country code"));
    }
    let stations = radio_browser::search(&query).await.map_err(|message| ApiError::new(502, message))?;
    Ok(Json(json!(stations)))
}

async fn list(State(state): State<AppState>, _admin: AdminUser, Path(path): Ids) -> ApiResult<Json<Value>> {
    let stations = state.db.radio_stations(config_id(&path)?).await?;
    Ok(Json(stations.iter().map(station_json).collect()))
}

async fn create(State(state): State<AppState>, _admin: AdminUser, Path(path): Ids, FlexJson(body): FlexJson) -> ApiResult<(StatusCode, Json<Value>)> {
    let config = config_id(&path)?;
    let (Some(name), Some(url)) = (text_field(&body, "name"), text_field(&body, "url")) else {
        return Err(ApiError::new(400, "name and url are required"));
    };
    check_address(url).await?;
    if state.db.server(config).await?.is_none() {
        return Err(ApiError::new(404, "Server not found"));
    }
    let new = NewStation { name, url, genre: text_field(&body, "genre"), image_url: text_field(&body, "imageUrl") };
    let station = state.db.create_radio_station(config, &new).await?;
    if let Some(id) = text_field(&body, "stationuuid") {
        let id = id.to_owned();
        tokio::spawn(async move { radio_browser::record_click(&id).await });
    }
    Ok((StatusCode::CREATED, Json(station_json(&station))))
}

async fn update(State(state): State<AppState>, _admin: AdminUser, Path(path): Ids, FlexJson(body): FlexJson) -> ApiResult<Json<Value>> {
    let config = config_id(&path)?;
    let id = row_id(&path, "Station not found")?;
    let existing = state.db.radio_station(config, id).await?.ok_or_else(|| ApiError::new(404, "Station not found"))?;
    let name = match body.get("name") {
        None => existing.name,
        Some(value) => value.as_str().map(str::trim).filter(|name| !name.is_empty()).ok_or_else(|| ApiError::new(400, "name is required"))?.to_owned(),
    };
    let url = match body.get("url") {
        None => existing.url,
        Some(value) => {
            let url = value.as_str().ok_or_else(|| ApiError::new(400, "url must be a string"))?;
            check_address(url).await?;
            url.to_owned()
        }
    };
    let genre = match body.get("genre") {
        None => existing.genre,
        Some(Value::Null) => None,
        Some(Value::String(genre)) => Some(genre.clone()).filter(|genre| !genre.is_empty()),
        Some(_) => return Err(ApiError::new(400, "genre must be a string or null")),
    };
    let station = state.db.update_radio_station(config, id, &name, &url, genre.as_deref()).await?;
    Ok(Json(station_json(&station)))
}

/// Only this connection's stations: an id from another connection is simply not found.
async fn remove(State(state): State<AppState>, _admin: AdminUser, Path(path): Ids) -> ApiResult<Json<Value>> {
    if !state.db.delete_radio_station(config_id(&path)?, row_id(&path, "Station not found")?).await? {
        return Err(ApiError::new(404, "Station not found"));
    }
    Ok(Json(json!({"success": true})))
}

async fn reset_ids(State(state): State<AppState>, _admin: AdminUser, Path(path): Ids) -> ApiResult<Json<Value>> {
    let stations = state.db.compact_radio_ids(config_id(&path)?).await?;
    Ok(Json(json!({"stations": stations.iter().map(station_json).collect::<Vec<_>>()})))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/presets", get(presets))
        .route("/browse", get(browse))
        .route("/reset-ids", post(reset_ids))
        .route("/{id}", put(update).delete(remove))
}
