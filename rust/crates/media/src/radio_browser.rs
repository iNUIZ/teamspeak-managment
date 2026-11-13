//! Community Radio Browser: finding stations to import. Mirrors come from the service's own
//! list, tried in random order until one answers.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;

const DIRECTORY_HOST: &str = "all.api.radio-browser.info";
const MIRROR_TTL: Duration = Duration::from_secs(60 * 60);
const TIMEOUT: Duration = Duration::from_secs(12);
const MAX_LIMIT: i64 = 25;
const USER_AGENT: &str = concat!("teamhub/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Station {
    pub stationuuid: String,
    pub name: String,
    pub url: String,
    pub genre: String,
    pub image_url: Option<String>,
    pub countrycode: String,
    pub codec: String,
    pub bitrate: f64,
}

#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    pub name: String,
    pub tag: String,
    pub country: String,
    pub limit: Option<i64>,
}

fn is_web_address(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

fn text(raw: &Value, key: &str) -> String {
    match &raw[key] {
        Value::String(value) => value.trim().to_owned(),
        Value::Number(value) => value.to_string(),
        _ => String::new(),
    }
}

/// The first three different tags, which the panel shows as mood chips.
fn first_tags(raw: &str) -> String {
    let mut seen: Vec<String> = Vec::new();
    let mut tags: Vec<String> = Vec::new();
    for tag in raw.split(',') {
        let tag = tag.split_whitespace().collect::<Vec<_>>().join(" ");
        let key = tag.to_lowercase();
        if tag.is_empty() || seen.contains(&key) {
            continue;
        }
        seen.push(key);
        tags.push(tag);
        if tags.len() == 3 {
            break;
        }
    }
    tags.join(", ")
}

/// A station from the service, or nothing when it has no usable address.
pub fn map_station(raw: &Value) -> Option<Station> {
    let stationuuid = text(raw, "stationuuid");
    let name = text(raw, "name");
    let resolved = text(raw, "url_resolved");
    let url = if resolved.is_empty() { text(raw, "url") } else { resolved };
    if stationuuid.is_empty() || name.is_empty() || url.is_empty() || !is_web_address(&url) {
        return None;
    }
    let favicon = text(raw, "favicon");
    let bitrate = match &raw["bitrate"] {
        Value::Number(value) => value.as_f64().unwrap_or(0.0),
        Value::String(value) => value.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    };
    Some(Station {
        stationuuid,
        name,
        url,
        genre: first_tags(raw["tags"].as_str().unwrap_or_default()),
        image_url: is_web_address(&favicon).then_some(favicon),
        countrycode: text(raw, "countrycode").to_uppercase(),
        codec: text(raw, "codec"),
        bitrate,
    })
}

/// The search as the service expects it: working, non-HLS, most voted first.
pub fn search_params(query: &SearchQuery) -> Result<Vec<(&'static str, String)>, String> {
    let (name, tag, country) = (query.name.trim(), query.tag.trim(), query.country.trim().to_uppercase());
    if name.is_empty() && tag.is_empty() && country.is_empty() {
        return Err("Provide a search name, tag, or country code".to_owned());
    }
    let limit = query.limit.unwrap_or(MAX_LIMIT).clamp(1, MAX_LIMIT);
    let mut params = vec![
        ("lastcheckok", "1".to_owned()),
        ("hls", "0".to_owned()),
        ("order", "votes".to_owned()),
        ("reverse", "true".to_owned()),
        ("limit", limit.to_string()),
        ("hidebroken", "true".to_owned()),
    ];
    if !name.is_empty() {
        params.push(("name", name.to_owned()));
    }
    if !tag.is_empty() {
        params.push(("tag", tag.to_owned()));
    }
    if !country.is_empty() {
        params.push(("countrycode", country));
    }
    Ok(params)
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder().timeout(TIMEOUT).user_agent(USER_AGENT).build().map_err(|_| "Radio Browser search failed".to_owned())
}

static MIRRORS: Mutex<Option<(Vec<String>, Instant)>> = Mutex::new(None);

/// The mirror list; the shared address also answers when the list cannot be read.
async fn mirrors(client: &reqwest::Client) -> Vec<String> {
    if let Some((hosts, until)) = MIRRORS.lock().expect("mirrors lock").as_ref() {
        if *until > Instant::now() {
            return hosts.clone();
        }
    }
    let listed: Option<Value> = match client.get(format!("https://{DIRECTORY_HOST}/json/servers")).send().await {
        Ok(response) if response.status().is_success() => response.json().await.ok(),
        _ => None,
    };
    let mut hosts: Vec<String> = Vec::new();
    for server in listed.as_ref().and_then(Value::as_array).into_iter().flatten() {
        if let Some(name) = server["name"].as_str().filter(|name| !name.is_empty()) {
            if !hosts.iter().any(|host| host == name) {
                hosts.push(name.to_owned());
            }
        }
    }
    if hosts.is_empty() {
        return vec![DIRECTORY_HOST.to_owned()];
    }
    *MIRRORS.lock().expect("mirrors lock") = Some((hosts.clone(), Instant::now() + MIRROR_TTL));
    hosts
}

fn shuffled(mut hosts: Vec<String>) -> Vec<String> {
    fastrand::shuffle(&mut hosts);
    hosts
}

pub async fn search(query: &SearchQuery) -> Result<Vec<Station>, String> {
    let params = search_params(query)?;
    let client = client()?;
    let mut last_error = "Radio Browser search failed".to_owned();
    for host in shuffled(mirrors(&client).await) {
        let response = client.get(format!("https://{host}/json/stations/search")).query(&params).send().await;
        let response = match response {
            Ok(response) if response.status().is_success() => response,
            Ok(response) => {
                last_error = format!("Radio Browser HTTP {}", response.status().as_u16());
                continue;
            }
            Err(_) => continue,
        };
        match response.json::<Value>().await {
            Ok(Value::Array(stations)) => return Ok(stations.iter().filter_map(map_station).collect()),
            _ => last_error = "Unexpected Radio Browser response".to_owned(),
        }
    }
    Err(last_error)
}

fn is_station_id(id: &str) -> bool {
    id.len() == 36 && id.bytes().all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
}

/// Tell the service a station was chosen, which helps its ranking. Failure changes nothing.
pub async fn record_click(id: &str) {
    let id = id.trim();
    if !is_station_id(id) {
        return;
    }
    let Ok(client) = client() else { return };
    for host in shuffled(mirrors(&client).await) {
        if matches!(client.get(format!("https://{host}/json/url/{id}")).send().await, Ok(response) if response.status().is_success()) {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn stations_are_mapped_for_import() {
        let station = map_station(&json!({
            "stationuuid": "u", "name": " Lofi ", "url": "http://a", "url_resolved": "https://b",
            "tags": "Chill, chill,  lo  fi ,jazz,rock", "favicon": "ftp://x", "countrycode": "de", "codec": "MP3", "bitrate": 128,
        }))
        .unwrap();
        assert_eq!((station.name.as_str(), station.url.as_str(), station.genre.as_str()), ("Lofi", "https://b", "Chill, lo fi, jazz"));
        assert_eq!((station.image_url, station.countrycode.as_str(), station.bitrate), (None, "DE", 128.0));
        assert_eq!(serde_json::to_value(map_station(&json!({"stationuuid": "u", "name": "n", "url": "http://a", "favicon": "https://i"})).unwrap()).unwrap()["imageUrl"], "https://i");
    }

    #[test]
    fn unusable_stations_are_left_out() {
        assert_eq!(map_station(&json!({"name": "n", "url": "http://a"})), None);
        assert_eq!(map_station(&json!({"stationuuid": "u", "url": "http://a"})), None);
        assert_eq!(map_station(&json!({"stationuuid": "u", "name": "n", "url": "ftp://a"})), None);
        assert_eq!(map_station(&json!("x")), None);
    }

    #[test]
    fn a_search_needs_something_to_look_for() {
        assert!(search_params(&SearchQuery { name: "  ".to_owned(), ..SearchQuery::default() }).is_err());
        let params = search_params(&SearchQuery { country: "tr".to_owned(), limit: Some(500), ..SearchQuery::default() }).unwrap();
        assert!(params.contains(&("countrycode", "TR".to_owned())));
        assert!(params.contains(&("limit", "25".to_owned())));
        assert!(params.contains(&("hls", "0".to_owned())));
    }

    #[test]
    fn only_real_station_ids_are_sent() {
        assert!(is_station_id("ef52b56c-6830-4346-b4d3-e42e5ae5d928"));
        assert!(!is_station_id("../../etc"));
    }
}
