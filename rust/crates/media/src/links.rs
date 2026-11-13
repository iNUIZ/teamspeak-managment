//! Spotify and Apple Music links. Neither gives audio: these read the track names, which are
//! then matched on YouTube. Every request goes to an allowed host over https, rebuilt on each
//! redirect, so a redirect cannot lead this program anywhere else.

use std::collections::HashSet;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;
use url::Url;

use crate::{err, Result};

const TIMEOUT: Duration = Duration::from_secs(180);
const MAX_REDIRECTS: usize = 5;
const BOT_AGENT: &str = "Mozilla/5.0 (compatible; ts6-manager/1.0)";
const BROWSER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const SPOTIFY_HOSTS: [&str; 5] = ["open.spotify.com", "www.spotify.com", "spotify.com", "spotify.link", "play.spotify.com"];
const APPLE_HOSTS: [&str; 4] = ["music.apple.com", "itunes.apple.com", "geo.itunes.apple.com", "apple.co"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkTrack {
    pub artist: String,
    pub title: String,
}

impl LinkTrack {
    /// What to search on YouTube.
    pub fn query(&self) -> String {
        [self.artist.as_str(), self.title.as_str()].iter().filter(|part| !part.is_empty()).copied().collect::<Vec<_>>().join(" ")
    }
}

/// The tracks of a share link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collection {
    pub title: Option<String>,
    pub tracks: Vec<LinkTrack>,
}

fn normalize_host(host: &str) -> String {
    host.trim_end_matches('.').to_ascii_lowercase()
}

fn host_of(url: &str) -> Option<String> {
    Url::parse(url.trim()).ok()?.host_str().map(normalize_host)
}

/// A link that must be resolved here, never handed to yt-dlp.
pub fn is_spotify_share_url(url: &str) -> bool {
    host_of(url).is_some_and(|host| host == "spotify.com" || host.ends_with(".spotify.com") || host == "spotify.link")
}

pub fn is_apple_music_share_url(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url.trim()) else { return false };
    matches!(parsed.scheme(), "http" | "https") && parsed.host_str().is_some_and(|host| APPLE_HOSTS.contains(&normalize_host(host).as_str()))
}

/// Fetch on an allowed host, checking again on every redirect. Returns the body and final URL.
async fn fetch_allowed(start: Url, hosts: &[&str], agent: &str, accept_http: bool) -> Result<(reqwest::StatusCode, String, Url)> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(TIMEOUT)
        .user_agent(agent)
        .build()
        .or_else(|error| err(format!("Could not start a web request: {error}")))?;
    let mut current = start;
    for _ in 0..=MAX_REDIRECTS {
        if !current.username().is_empty() || current.password().is_some() {
            return err("URL credentials are not allowed");
        }
        let scheme_allowed = current.scheme() == "https" || (accept_http && current.scheme() == "http");
        if !scheme_allowed {
            return err("Invalid URL protocol: only https is allowed");
        }
        let host = current.host_str().map(normalize_host).unwrap_or_default();
        let Some(allowed) = hosts.iter().find(|allowed| **allowed == host) else {
            return err(format!("Refusing fetch to disallowed host: {host}"));
        };
        let mut safe = Url::parse(&format!("https://{allowed}")).or_else(|_| err("Invalid URL"))?;
        safe.set_path(current.path());
        safe.set_query(current.query());
        let response = client.get(safe.clone()).send().await.or_else(|error| err(format!("Web request failed: {}", error.without_url())))?;
        if response.status().is_redirection() {
            let Some(location) = response.headers().get(reqwest::header::LOCATION).and_then(|value| value.to_str().ok()) else {
                return err("Redirect missing Location header");
            };
            current = safe.join(location).or_else(|_| err("Invalid redirect"))?;
            continue;
        }
        let status = response.status();
        let text = response.text().await.or_else(|error| err(format!("Web request failed: {}", error.without_url())))?;
        return Ok((status, text, safe));
    }
    err("Too many redirects while fetching allowlisted URL")
}

fn decode_entities(text: &str) -> String {
    // `&amp;` last, so `&amp;#39;` cannot be unescaped twice into a quote.
    let mut out = text.to_owned();
    for (from, to) in [("&lt;", "<"), ("&LT;", "<"), ("&gt;", ">"), ("&GT;", ">"), ("&quot;", "\""), ("&QUOT;", "\""), ("&#39;", "'"), ("&#039;", "'"), ("&#x27;", "'"), ("&#X27;", "'")] {
        out = out.replace(from, to);
    }
    out.replace("&amp;", "&").replace("&AMP;", "&")
}

/// One Open Graph `content`, attributes in any order.
fn og_content(html: &str, property: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let wanted = format!("property=\"og:{property}\"");
    let mut from = 0;
    while let Some(start) = lower[from..].find("<meta") {
        let start = from + start;
        let Some(length) = lower[start..].find('>') else { break };
        let tag = &lower[start..start + length];
        if tag.contains(&wanted) {
            if let Some(at) = tag.find("content=\"") {
                let value_start = start + at + "content=\"".len();
                if let Some(end) = html[value_start..].find('"') {
                    return decode_entities(&html[value_start..value_start + end]).trim().to_owned();
                }
            }
        }
        from = start + length;
    }
    String::new()
}

/// The text of `<script id="...">`.
fn script_body<'a>(html: &'a str, id: &str) -> Option<&'a str> {
    let lower = html.to_ascii_lowercase();
    let wanted = format!("id=\"{}\"", id.to_ascii_lowercase());
    let mut from = 0;
    while let Some(start) = lower[from..].find("<script") {
        let start = from + start;
        let open_end = start + lower[start..].find('>')?;
        if lower[start..open_end].contains(&wanted) {
            let close = open_end + 1 + lower[open_end + 1..].find("</script>")?;
            return Some(&html[open_end + 1..close]);
        }
        from = open_end;
    }
    None
}

// ─── Spotify ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpotifyKind {
    Track,
    Album,
    Playlist,
    Other,
}

fn is_spotify_id(id: &str) -> bool {
    id.len() == 22 && id.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// `<kind>/<id>` from an open.spotify.com path, ignoring `/intl-xx/` and `/embed/`.
pub fn parse_spotify_path(path: &str) -> (SpotifyKind, Option<String>) {
    let mut parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    while parts.first().is_some_and(|part| *part == "embed" || (part.len() > 5 && part[..5].eq_ignore_ascii_case("intl-"))) {
        parts.remove(0);
    }
    if let [kind, id, ..] = parts.as_slice() {
        if is_spotify_id(id) {
            let kind = match *kind {
                "track" => SpotifyKind::Track,
                "album" => SpotifyKind::Album,
                "playlist" => SpotifyKind::Playlist,
                _ => return (SpotifyKind::Other, None),
            };
            return (kind, Some((*id).to_owned()));
        }
    }
    (SpotifyKind::Other, None)
}

fn find_track_list(node: &Value, depth: usize) -> Option<(&Vec<Value>, &Value)> {
    if depth > 12 {
        return None;
    }
    match node {
        Value::Object(object) => {
            if let Some(Value::Array(list)) = object.get("trackList") {
                if list.iter().any(|track| track["title"].is_string()) {
                    return Some((list, node));
                }
            }
            object.values().find_map(|value| find_track_list(value, depth + 1))
        }
        Value::Array(items) => items.iter().find_map(|value| find_track_list(value, depth + 1)),
        _ => None,
    }
}

/// The tracks on a Spotify embed page, from its `__NEXT_DATA__` script.
pub fn parse_spotify_embed(html: &str) -> Collection {
    let empty = Collection { title: None, tracks: Vec::new() };
    let Some(data) = script_body(html, "__NEXT_DATA__").and_then(|body| serde_json::from_str::<Value>(body).ok()) else {
        return empty;
    };
    let Some((list, owner)) = find_track_list(&data, 0) else { return empty };
    let tracks = list
        .iter()
        .filter_map(|entry| {
            let title = entry["title"].as_str()?.trim();
            if title.is_empty() {
                return None;
            }
            let artist = entry["subtitle"].as_str().unwrap_or_default().replace('\u{a0}', " ");
            Some(LinkTrack { artist: artist.split_whitespace().collect::<Vec<_>>().join(" "), title: title.to_owned() })
        })
        .collect();
    let title = owner["name"].as_str().or(owner["title"].as_str()).map(str::trim).filter(|name| !name.is_empty()).map(str::to_owned);
    Collection { title, tracks }
}

/// A search query from a Spotify page's tags. Track pages say `Artist · Song · 1987`.
pub fn spotify_query_from_og(html: &str) -> String {
    let title = og_content(html, "title");
    if title.is_empty() {
        return String::new();
    }
    let description = og_content(html, "description");
    let parts: Vec<&str> = description.split('·').map(str::trim).collect();
    if parts.iter().skip(1).any(|part| part.eq_ignore_ascii_case("song")) {
        return format!("{} {title}", parts[0]);
    }
    title
}

fn parse_spotify_url(url: &str) -> Result<Url> {
    let Ok(parsed) = Url::parse(url.trim()) else {
        return err("Invalid Spotify URL");
    };
    if !parsed.host_str().is_some_and(|host| SPOTIFY_HOSTS.contains(&normalize_host(host).as_str())) {
        return err("Not a Spotify share URL");
    }
    if parsed.scheme() != "https" {
        return err("Invalid Spotify URL protocol: only https is allowed");
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return err("URL credentials are not allowed");
    }
    Ok(parsed)
}

/// What a Spotify link holds: its tracks when a playlist or album, else one search query. A
/// collection that cannot be read is an error, so it never turns into a song named after it.
pub async fn resolve_spotify(url: &str) -> Result<Collection> {
    let parsed = parse_spotify_url(url)?;
    let (mut kind, mut id) = parse_spotify_path(parsed.path());
    let mut page = None;
    if kind == SpotifyKind::Other {
        // Short links only reveal their target after redirects.
        let (_, body, final_url) = fetch_allowed(parsed.clone(), &SPOTIFY_HOSTS, BOT_AGENT, false).await?;
        (kind, id) = parse_spotify_path(final_url.path());
        page = Some(body);
    }
    if let (SpotifyKind::Album | SpotifyKind::Playlist, Some(id)) = (kind, id) {
        let name = if kind == SpotifyKind::Album { "album" } else { "playlist" };
        let embed = Url::parse(&format!("https://open.spotify.com/embed/{name}/{id}")).or_else(|_| err("Invalid Spotify URL"))?;
        let found = match fetch_allowed(embed, &SPOTIFY_HOSTS, BOT_AGENT, false).await {
            Ok((status, body, _)) if status.is_success() => parse_spotify_embed(&body),
            _ => Collection { title: None, tracks: Vec::new() },
        };
        if found.tracks.is_empty() {
            return err(format!("Could not read the tracks of that Spotify {name}. It may be private or region-locked; try a YouTube or Apple Music link instead."));
        }
        return Ok(found);
    }
    let body = match page {
        Some(body) => Some(body),
        None => fetch_allowed(parsed.clone(), &SPOTIFY_HOSTS, BOT_AGENT, false).await.ok().map(|(_, body, _)| body),
    };
    let mut query = body.as_deref().map(spotify_query_from_og).unwrap_or_default();
    if query.is_empty() {
        query = parsed.path_segments().and_then(|mut parts| parts.next_back()).unwrap_or_default().replace('-', " ");
    }
    Ok(Collection { title: None, tracks: vec![LinkTrack { artist: String::new(), title: query }] })
}

/// Search in the Spotify catalogue with a developer app. Spotify gives names only; the audio is
/// matched on YouTube, since Spotify's own is protected.
pub struct SpotifyCatalog {
    client_id: String,
    client_secret: String,
    market: String,
    token: Mutex<Option<(String, Instant)>>,
}

/// One catalogue result.
#[derive(Debug, Clone, PartialEq)]
pub struct SpotifyTrack {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub duration: f64,
    pub url: String,
}

impl SpotifyCatalog {
    /// From `SPOTIFY_CLIENT_ID` and `SPOTIFY_CLIENT_SECRET`, when both are set.
    pub fn from_env() -> Option<Self> {
        let read = |name: &str| std::env::var(name).ok().map(|value| value.trim().to_owned()).filter(|value| !value.is_empty());
        Some(Self {
            client_id: read("SPOTIFY_CLIENT_ID")?,
            client_secret: read("SPOTIFY_CLIENT_SECRET")?,
            market: read("SPOTIFY_MARKET").unwrap_or_else(|| "TR".to_owned()),
            token: Mutex::new(None),
        })
    }

    async fn access_token(&self, client: &reqwest::Client) -> Result<String> {
        if let Some((token, expires)) = self.token.lock().expect("token lock").as_ref() {
            if Instant::now() < *expires {
                return Ok(token.clone());
            }
        }
        let response = client
            .post("https://accounts.spotify.com/api/token")
            .basic_auth(&self.client_id, Some(&self.client_secret))
            .header(reqwest::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body("grant_type=client_credentials")
            .send()
            .await
            .or_else(|_| err("Spotify sign-in failed"))?;
        // The body is never shown: a failed sign-in must not leak the app's credentials.
        if !response.status().is_success() {
            return err(format!("Spotify sign-in failed (HTTP {}). Check SPOTIFY_CLIENT_ID and SPOTIFY_CLIENT_SECRET.", response.status().as_u16()));
        }
        let body: Value = response.json().await.or_else(|_| err("Spotify sign-in returned no token"))?;
        let Some(token) = body["access_token"].as_str().filter(|token| !token.is_empty()) else {
            return err("Spotify sign-in returned no token");
        };
        let lifetime = body["expires_in"].as_u64().unwrap_or(3600);
        *self.token.lock().expect("token lock") = Some((token.to_owned(), Instant::now() + Duration::from_secs(lifetime.saturating_sub(60).max(60))));
        Ok(token.to_owned())
    }

    pub async fn search(&self, query: &str, limit: u8) -> Result<Vec<SpotifyTrack>> {
        let client = reqwest::Client::builder().timeout(TIMEOUT).build().or_else(|_| err("Could not start a web request"))?;
        let token = self.access_token(&client).await?;
        let limit = limit.clamp(1, 10).to_string();
        let response = client
            .get("https://api.spotify.com/v1/search")
            .query(&[("q", query), ("type", "track"), ("limit", limit.as_str()), ("market", self.market.as_str())])
            .bearer_auth(token)
            .send()
            .await
            .or_else(|_| err("Spotify search failed"))?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            *self.token.lock().expect("token lock") = None;
        }
        if !response.status().is_success() {
            return err(format!("Spotify search failed (HTTP {})", response.status().as_u16()));
        }
        let body: Value = response.json().await.or_else(|_| err("Spotify search failed"))?;
        Ok(parse_spotify_search(&body))
    }
}

pub fn parse_spotify_search(body: &Value) -> Vec<SpotifyTrack> {
    let items = body["tracks"]["items"].as_array().map(Vec::as_slice).unwrap_or_default();
    items
        .iter()
        .filter_map(|track| {
            let id = track["id"].as_str().filter(|id| is_spotify_id(id))?;
            let title = track["name"].as_str()?;
            let artists: Vec<&str> = track["artists"].as_array().map(Vec::as_slice).unwrap_or_default().iter().filter_map(|artist| artist["name"].as_str()).filter(|name| !name.is_empty()).collect();
            Some(SpotifyTrack {
                id: id.to_owned(),
                title: title.to_owned(),
                artist: if artists.is_empty() { "Unknown".to_owned() } else { artists.join(", ") },
                duration: track["duration_ms"].as_f64().map_or(0.0, |ms| (ms / 1000.0).round()),
                url: format!("https://open.spotify.com/track/{id}"),
            })
        })
        .collect()
}

// ─── Apple Music ──────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedAppleUrl {
    /// `song`, `album`, `playlist` or `unknown`.
    pub kind: &'static str,
    pub id: Option<String>,
    pub song_id: Option<String>,
}

fn is_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

/// The kind and ids in an Apple Music address, without any request.
pub fn parse_apple_music_url(raw: &str) -> ParsedAppleUrl {
    let unknown = ParsedAppleUrl { kind: "unknown", ..ParsedAppleUrl::default() };
    let Ok(parsed) = Url::parse(raw.trim()) else { return unknown };
    if !parsed.host_str().is_some_and(|host| APPLE_HOSTS.contains(&normalize_host(host).as_str())) {
        return unknown;
    }
    let parts: Vec<&str> = parsed.path().split('/').filter(|part| !part.is_empty()).collect();
    // /{storefront}/album|song|playlist/{slug}/{id}
    let at = usize::from(parts.first().is_some_and(|part| part.len() == 2 && part.bytes().all(|b| b.is_ascii_alphabetic())));
    let kind = parts.get(at).map(|kind| kind.to_ascii_lowercase()).unwrap_or_default();
    let id = parts.get(at + 2).or(parts.get(at + 1)).copied().unwrap_or_default();
    let song_param = parsed.query_pairs().find(|(key, _)| key == "i").map(|(_, value)| value.into_owned()).filter(|value| is_digits(value));

    if kind == "song" && is_digits(id) {
        return ParsedAppleUrl { kind: "song", id: Some(id.to_owned()), song_id: Some(id.to_owned()) };
    }
    if kind == "album" && is_digits(id) {
        return match song_param {
            Some(song) => ParsedAppleUrl { kind: "song", id: Some(id.to_owned()), song_id: Some(song) },
            None => ParsedAppleUrl { kind: "album", id: Some(id.to_owned()), song_id: None },
        };
    }
    let playlist_id = id.strip_prefix("pl.").is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-')));
    if kind == "playlist" && playlist_id {
        return ParsedAppleUrl { kind: "playlist", id: Some(id.to_owned()), song_id: None };
    }
    // itunes.apple.com/album/id12345
    let lower = parsed.path().to_ascii_lowercase();
    if let Some(found) = lower.find("/album/id") {
        let digits: String = lower[found + "/album/id".len()..].chars().take_while(char::is_ascii_digit).collect();
        if !digits.is_empty() {
            return match song_param {
                Some(song) => ParsedAppleUrl { kind: "song", id: Some(digits), song_id: Some(song) },
                None => ParsedAppleUrl { kind: "album", id: Some(digits), song_id: None },
            };
        }
    }
    unknown
}

fn track_from_itunes(result: &Value) -> Option<LinkTrack> {
    let title = result["trackName"].as_str()?.trim();
    let artist = result["artistName"].as_str()?.trim();
    (!title.is_empty() && !artist.is_empty()).then(|| LinkTrack { artist: artist.to_owned(), title: title.to_owned() })
}

/// An iTunes Lookup answer as a track list.
pub fn parse_itunes_results(results: &[Value], album: bool) -> Option<Collection> {
    if album {
        let tracks: Vec<LinkTrack> = results.iter().filter(|result| result["kind"] == "song").filter_map(track_from_itunes).collect();
        if tracks.is_empty() {
            return None;
        }
        let title = results.iter().find(|result| result["wrapperType"] == "collection").and_then(|collection| collection["collectionName"].as_str()).map(str::to_owned);
        return Some(Collection { title, tracks });
    }
    let song = results.iter().find(|result| result["kind"] == "song").or(results.first())?;
    let track = track_from_itunes(song)?;
    Some(Collection { title: Some(format!("{} - {}", track.artist, track.title)), tracks: vec![track] })
}

async fn itunes_lookup(id: &str, album: bool) -> Result<Vec<Value>> {
    if !is_digits(id) {
        return err("Invalid iTunes id");
    }
    let client = reqwest::Client::builder().timeout(TIMEOUT).user_agent(BOT_AGENT).build().or_else(|_| err("Could not start a web request"))?;
    let mut query = vec![("id", id)];
    if album {
        query.push(("entity", "song"));
    }
    let response = client.get("https://itunes.apple.com/lookup").query(&query).send().await.or_else(|_| err("iTunes lookup failed"))?;
    if !response.status().is_success() {
        return err(format!("iTunes lookup failed: HTTP {}", response.status().as_u16()));
    }
    let body: Value = response.json().await.or_else(|_| err("iTunes lookup failed"))?;
    Ok(body["results"].as_array().cloned().unwrap_or_default())
}

/// Tracks in the Apple Music page's `serialized-server-data`: track rows first, then any
/// title and artist pairs.
pub fn extract_apple_tracks(data: &Value) -> Collection {
    struct Walk {
        tracks: Vec<LinkTrack>,
        seen: HashSet<(String, String)>,
        page_title: Option<String>,
    }
    impl Walk {
        fn add(&mut self, artist: &str, title: &str) {
            let (artist, title) = (artist.trim(), title.trim());
            if artist.is_empty() || title.is_empty() || artist.eq_ignore_ascii_case("apple music") || title.eq_ignore_ascii_case("preview") {
                return;
            }
            if self.seen.insert((artist.to_lowercase(), title.to_lowercase())) {
                self.tracks.push(LinkTrack { artist: artist.to_owned(), title: title.to_owned() });
            }
        }

        fn rows(&mut self, node: &Value) {
            match node {
                Value::Array(items) => items.iter().for_each(|item| self.rows(item)),
                Value::Object(object) => {
                    let title = node["title"].as_str().unwrap_or_default();
                    if node["pageFields"]["pageType"] == "Playlist" && !title.is_empty() && self.page_title.is_none() {
                        self.page_title = Some(title.to_owned());
                    }
                    let id = node["id"].as_str().unwrap_or_default();
                    let artist = node["artistName"].as_str().filter(|artist| !artist.is_empty()).or(node["subtitle"].as_str()).unwrap_or_default();
                    if id.contains("track-lockup") && !title.is_empty() && !artist.is_empty() {
                        self.add(artist, title);
                    }
                    if (id.contains("playlist-detail-header") || id.contains("album-detail-header")) && !title.is_empty() && self.page_title.is_none() {
                        self.page_title = Some(title.to_owned());
                    }
                    object.values().for_each(|value| self.rows(value));
                }
                _ => {}
            }
        }

        fn pairs(&mut self, node: &Value) {
            match node {
                Value::Array(items) => items.iter().for_each(|item| self.pairs(item)),
                Value::Object(object) => {
                    if let (Some(title), Some(artist)) = (node["title"].as_str(), node["artistName"].as_str()) {
                        self.add(artist, title);
                    }
                    object.values().for_each(|value| self.pairs(value));
                }
                _ => {}
            }
        }
    }
    let mut walk = Walk { tracks: Vec::new(), seen: HashSet::new(), page_title: None };
    walk.rows(data);
    if walk.tracks.is_empty() {
        walk.pairs(data);
    }
    Collection { title: walk.page_title, tracks: walk.tracks }
}

/// Tracks from an Apple Music web page; the page's title as one search when nothing else.
pub fn parse_apple_page(html: &str) -> Option<Collection> {
    if let Some(data) = script_body(html, "serialized-server-data").and_then(|body| serde_json::from_str::<Value>(body).ok()) {
        let extracted = extract_apple_tracks(&data);
        if !extracted.tracks.is_empty() {
            return Some(extracted);
        }
    }
    let title = og_content(html, "title");
    let title = title.strip_suffix(" on Apple Music").unwrap_or(&title).trim();
    if title.is_empty() || title.to_ascii_lowercase().starts_with("apple music") {
        return None;
    }
    let dash = [" – ", " — ", " - "].iter().filter_map(|mark| title.find(mark).map(|at| (at, mark.len()))).min();
    if let Some((at, length)) = dash {
        let track = LinkTrack { title: title[..at].trim().to_owned(), artist: title[at + length..].trim().to_owned() };
        return Some(Collection { title: Some(title.to_owned()), tracks: vec![track] });
    }
    Some(Collection { title: Some(title.to_owned()), tracks: vec![LinkTrack { title: title.to_owned(), artist: String::new() }] })
}

async fn apple_page(url: Url) -> Result<Option<Collection>> {
    let (status, body, _) = fetch_allowed(url, &APPLE_HOSTS, BROWSER_AGENT, true).await?;
    if !status.is_success() {
        return err(format!("Apple Music page fetch failed: HTTP {}", status.as_u16()));
    }
    Ok(parse_apple_page(&body))
}

/// The tracks of an Apple Music, iTunes or apple.co link.
pub async fn resolve_apple_music(url: &str) -> Result<Collection> {
    let Ok(mut parsed) = Url::parse(url.trim()) else {
        return err("Invalid Apple Music URL");
    };
    if !is_apple_music_share_url(url) {
        return err("Not an Apple Music share URL");
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return err("URL credentials are not allowed");
    }
    let host = parsed.host_str().map(normalize_host).unwrap_or_default();
    // Short links and iTunes addresses: follow to the canonical page.
    if host != "music.apple.com" {
        let (_, _, final_url) = fetch_allowed(parsed, &APPLE_HOSTS, BROWSER_AGENT, true).await?;
        parsed = final_url;
    }
    let link = parse_apple_music_url(parsed.as_str());
    // iTunes Lookup first for numeric ids: stable, no scraping.
    let lookup = match (link.kind, &link.song_id, &link.id) {
        ("song", Some(song), _) => itunes_lookup(song, false).await.ok().and_then(|results| parse_itunes_results(&results, false)),
        ("album", _, Some(album)) => itunes_lookup(album, true).await.ok().and_then(|results| parse_itunes_results(&results, true)),
        _ => None,
    };
    if let Some(found) = lookup {
        return Ok(found);
    }
    let page = apple_page(parsed).await?;
    if link.kind == "song" {
        // A song link must not expand to the whole album.
        let Some(first) = page.and_then(|page| page.tracks.into_iter().next()) else {
            return err("Could not resolve that Apple Music song");
        };
        return Ok(Collection { title: Some(format!("{} - {}", first.artist, first.title)), tracks: vec![first] });
    }
    match page {
        Some(found) if !found.tracks.is_empty() => Ok(found),
        _ => err("Could not resolve any tracks from that Apple Music URL"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn share_links_are_recognized_exactly() {
        for url in ["https://open.spotify.com/track/x", "https://spotify.link/x", "https://www.spotify.com./x"] {
            assert!(is_spotify_share_url(url), "{url}");
        }
        for url in ["https://spotify.com.evil.example/x", "https://evilspotify.com/x", "nonsense"] {
            assert!(!is_spotify_share_url(url), "{url}");
        }
        for url in ["https://music.apple.com/x", "http://itunes.apple.com/x", "https://apple.co/x"] {
            assert!(is_apple_music_share_url(url), "{url}");
        }
        for url in ["https://apple.com/x", "ftp://music.apple.com/x", "https://music.apple.com.evil.example/"] {
            assert!(!is_apple_music_share_url(url), "{url}");
        }
    }

    #[tokio::test]
    async fn requests_never_leave_the_allowed_hosts() {
        for url in ["https://evil.example/x", "https://user:pw@open.spotify.com/x"] {
            let result = fetch_allowed(Url::parse(url).unwrap(), &SPOTIFY_HOSTS, BOT_AGENT, false).await;
            assert!(result.is_err(), "{url}");
        }
        assert!(fetch_allowed(Url::parse("http://open.spotify.com/x").unwrap(), &SPOTIFY_HOSTS, BOT_AGENT, false).await.is_err());
    }

    #[test]
    fn spotify_paths_are_read() {
        let id = "a".repeat(22);
        assert_eq!(parse_spotify_path(&format!("/track/{id}")), (SpotifyKind::Track, Some(id.clone())));
        assert_eq!(parse_spotify_path(&format!("/intl-tr/album/{id}")), (SpotifyKind::Album, Some(id.clone())));
        assert_eq!(parse_spotify_path(&format!("/embed/playlist/{id}?x")).0, SpotifyKind::Other, "a query is not part of a path");
        assert_eq!(parse_spotify_path(&format!("/embed/playlist/{id}")), (SpotifyKind::Playlist, Some(id.clone())));
        assert_eq!(parse_spotify_path(&format!("/artist/{id}")).0, SpotifyKind::Other);
        assert_eq!(parse_spotify_path("/track/short").0, SpotifyKind::Other);
    }

    #[test]
    fn spotify_embeds_give_tracks() {
        let html = r#"<html><script id="__NEXT_DATA__" type="application/json">{"props":{"entity":{"name":"Liste","trackList":[{"title":"A","subtitle":"X, Y"},{"title":" "},{"title":"B"}]}}}</script></html>"#;
        let found = parse_spotify_embed(html);
        assert_eq!(found.title.as_deref(), Some("Liste"));
        assert_eq!(found.tracks, [LinkTrack { artist: "X, Y".to_owned(), title: "A".to_owned() }, LinkTrack { artist: String::new(), title: "B".to_owned() }]);
        assert!(parse_spotify_embed("<html></html>").tracks.is_empty());
        assert!(parse_spotify_embed(r#"<script id="__NEXT_DATA__">{bad</script>"#).tracks.is_empty());
    }

    #[test]
    fn spotify_pages_give_a_query() {
        let html = r#"<meta content="Kuzu &amp; Kuzu" property="og:title"><meta name="x" property="og:description" content="Tarkan · Song · 1997">"#;
        assert_eq!(spotify_query_from_og(html), "Tarkan Kuzu & Kuzu");
        let with_album = r#"<meta property="og:title" content="Şımarık"><meta property="og:description" content="Tarkan · Ölürüm Sana · Song · 1997">"#;
        assert_eq!(spotify_query_from_og(with_album), "Tarkan Şımarık");
        assert_eq!(spotify_query_from_og(r#"<meta property="og:title" content="Liste">"#), "Liste");
        assert_eq!(spotify_query_from_og("<html>"), "");
    }

    #[test]
    fn entities_are_decoded_once() {
        assert_eq!(decode_entities("&amp;#39; &#39; &lt;"), "&#39; ' <");
    }

    #[test]
    fn spotify_search_results_are_read() {
        let id = "b".repeat(22);
        let body = json!({"tracks":{"items":[
            {"id": id, "name":"S", "duration_ms": 123456, "artists":[{"name":"A"},{"name":"B"},{"name":""}]},
            {"id":"bad","name":"X"},
            {"id": "c".repeat(22)},
            {"id": "d".repeat(22), "name":"N"},
        ]}});
        let found = parse_spotify_search(&body);
        assert_eq!(found.len(), 2);
        assert_eq!((found[0].artist.as_str(), found[0].duration), ("A, B", 123.0));
        assert_eq!(found[0].url, format!("https://open.spotify.com/track/{id}"));
        assert_eq!(found[1].artist, "Unknown");
    }

    #[test]
    fn apple_addresses_are_read() {
        let song = parse_apple_music_url("https://music.apple.com/tr/song/kuzu/123");
        assert_eq!((song.kind, song.song_id.as_deref()), ("song", Some("123")));
        let album = parse_apple_music_url("https://music.apple.com/tr/album/ad/456");
        assert_eq!((album.kind, album.id.as_deref()), ("album", Some("456")));
        let in_album = parse_apple_music_url("https://music.apple.com/tr/album/ad/456?i=789");
        assert_eq!((in_album.kind, in_album.song_id.as_deref()), ("song", Some("789")));
        let playlist = parse_apple_music_url("https://music.apple.com/tr/playlist/ad/pl.abc-1");
        assert_eq!((playlist.kind, playlist.id.as_deref()), ("playlist", Some("pl.abc-1")));
        assert_eq!(parse_apple_music_url("https://itunes.apple.com/album/id555").kind, "album");
        assert_eq!(parse_apple_music_url("https://itunes.apple.com/album/id555?i=6").song_id.as_deref(), Some("6"));
        assert_eq!(parse_apple_music_url("https://music.apple.com/tr/artist/x/1").kind, "unknown");
        assert_eq!(parse_apple_music_url("https://evil.example/tr/song/x/1").kind, "unknown");
    }

    #[test]
    fn itunes_answers_are_read() {
        let results = [
            json!({"wrapperType":"collection","collectionName":"Albüm"}),
            json!({"kind":"song","trackName":"A","artistName":"X"}),
            json!({"kind":"song","trackName":" ","artistName":"X"}),
            json!({"kind":"song","trackName":"B","artistName":"Y"}),
        ];
        let album = parse_itunes_results(&results, true).unwrap();
        assert_eq!(album.title.as_deref(), Some("Albüm"));
        assert_eq!(album.tracks.len(), 2);
        let song = parse_itunes_results(&results, false).unwrap();
        assert_eq!(song.title.as_deref(), Some("X - A"));
        assert!(parse_itunes_results(&[], true).is_none());
        assert!(parse_itunes_results(&[], false).is_none());
    }

    #[test]
    fn apple_pages_give_tracks_or_a_search() {
        let data = json!({"a":[
            {"id":"album-detail-header-1","title":"Albüm"},
            {"id":"track-lockup-1","title":"One","artistName":"X"},
            {"id":"track-lockup-2","title":"one","artistName":"x"},
            {"id":"track-lockup-3","title":"Two","subtitle":"Y"},
            {"id":"track-lockup-4","title":"Preview","artistName":"Z"},
            {"title":"Not a row","artistName":"Z"},
        ]});
        let found = extract_apple_tracks(&data);
        assert_eq!(found.title.as_deref(), Some("Albüm"));
        assert_eq!(found.tracks.iter().map(|track| track.title.as_str()).collect::<Vec<_>>(), ["One", "Two"]);
        let pairs = extract_apple_tracks(&json!({"x":{"title":"P","artistName":"Q"}}));
        assert_eq!(pairs.tracks[0].artist, "Q");

        let html = format!(r#"<script id="serialized-server-data" type="x">{data}</script>"#);
        assert_eq!(parse_apple_page(&html).unwrap().tracks.len(), 2);
        let og = parse_apple_page(r#"<meta property="og:title" content="Song &amp; Name - Artist on Apple Music">"#).unwrap();
        assert_eq!(og.tracks, [LinkTrack { title: "Song & Name".to_owned(), artist: "Artist".to_owned() }]);
        assert_eq!(parse_apple_page(r#"<meta property="og:title" content="Tek">"#).unwrap().tracks[0].query(), "Tek");
        assert!(parse_apple_page(r#"<meta property="og:title" content="Apple Music - Web Player">"#).is_none());
    }
}
