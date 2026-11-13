//! Lyrics for `!lyrics`: LRCLIB (no key), then lyrics.ovh.

use std::time::Duration;

use serde_json::Value;

const TIMEOUT: Duration = Duration::from_secs(10);
const USER_AGENT: &str = "ts6-manager";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LyricsQuery {
    pub artist: Option<String>,
    pub title: Option<String>,
    pub query: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricsResult {
    pub artist: String,
    pub title: String,
    pub lyrics: String,
    pub instrumental: bool,
}

const NOISE: [&str; 13] =
    ["official", "officiel", "video", "vidéo", "clip", "lyric", "paroles", "audio", "visualizer", "visualiser", "remaster", "hd", "4k"];

fn is_noise(group: &str) -> bool {
    let lower = group.to_lowercase();
    NOISE.iter().any(|word| {
        if word.len() <= 2 {
            lower.split(|c: char| !c.is_alphanumeric()).any(|part| part == *word)
        } else {
            lower.contains(word)
        }
    }) || lower.split(|c: char| !c.is_alphanumeric()).any(|part| part == "mv")
}

/// Strip what YouTube appends to titles ("(Official Video)", "[Lyrics]"). Brackets without such
/// a word belong to the song and stay.
pub fn clean_track_title(title: &str) -> String {
    let mut out = String::new();
    let mut rest = title;
    while let Some(open) = rest.find(['(', '[', '{']) {
        let close = rest[open + 1..].find(['(', '[', '{', ')', ']', '}']).map(|at| open + 1 + at);
        let Some(close) = close.filter(|close| matches!(rest.as_bytes()[*close], b')' | b']' | b'}')) else {
            out.push_str(&rest[..=open]);
            rest = &rest[open + 1..];
            continue;
        };
        out.push_str(&rest[..open]);
        if is_noise(&rest[open..=close]) {
            out.push(' ');
        } else {
            out.push_str(&rest[open..=close]);
        }
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Cut `header` and `lyrics` into pieces of at most `max_chars`, on line boundaries. A longer
/// single line is cut hard. Empty pieces are dropped.
pub fn chunk_lyrics(header: &str, lyrics: &str, max_chars: usize) -> Vec<String> {
    let text = if header.is_empty() { lyrics.to_owned() } else { format!("{header}\n{lyrics}") };
    let mut chunks = Vec::new();
    let mut buffer: Option<String> = None;
    for line in text.split('\n') {
        let mut line: Vec<char> = line.chars().collect();
        while line.len() > max_chars {
            if let Some(full) = buffer.take() {
                chunks.push(full);
            }
            chunks.push(line[..max_chars].iter().collect());
            line.drain(..max_chars);
        }
        let line: String = line.into_iter().collect();
        buffer = match buffer {
            None => Some(line),
            Some(mut full) if full.chars().count() + 1 + line.chars().count() <= max_chars => {
                full.push('\n');
                full.push_str(&line);
                Some(full)
            }
            Some(full) => {
                chunks.push(full);
                Some(line)
            }
        };
    }
    chunks.extend(buffer);
    chunks.retain(|chunk| !chunk.trim().is_empty());
    chunks
}

/// The search for a playing track: placeholder artists count as absent, the title is cleaned,
/// and the label keeps the original title.
pub fn lyrics_input_from_track(artist: Option<&str>, title: &str) -> (LyricsQuery, String) {
    let artist = artist.filter(|artist| !matches!(*artist, "Unknown" | "Unknown Artist"));
    let label = match artist {
        Some(artist) => format!("{artist} — {title}"),
        None => title.to_owned(),
    };
    (LyricsQuery { artist: artist.map(str::to_owned), title: Some(clean_track_title(title)), query: None }, label)
}

/// An LRCLIB record, when it has usable lyrics.
pub fn from_lrclib(entry: &Value) -> Option<LyricsResult> {
    entry.as_object()?;
    let text = |key: &str| entry[key].as_str().unwrap_or_default().to_owned();
    let (artist, title) = (text("artistName"), text("trackName"));
    if entry["instrumental"] == true {
        return Some(LyricsResult { artist, title, lyrics: String::new(), instrumental: true });
    }
    let lyrics = entry["plainLyrics"].as_str().unwrap_or_default().trim();
    (!lyrics.is_empty()).then(|| LyricsResult { artist, title, lyrics: lyrics.to_owned(), instrumental: false })
}

async fn get_json(client: &reqwest::Client, url: reqwest::Url) -> Option<Value> {
    let response = client.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json().await.ok()
}

fn http_client() -> Option<reqwest::Client> {
    reqwest::Client::builder().user_agent(USER_AGENT).timeout(TIMEOUT).build().ok()
}

/// Exact match when artist and title are known, then a fuzzy search, then lyrics.ovh.
pub async fn fetch_lyrics(input: &LyricsQuery) -> Option<LyricsResult> {
    let client = http_client()?;
    let artist = input.artist.as_deref().unwrap_or_default().trim();
    let title = input.title.as_deref().unwrap_or_default().trim();
    let query = input
        .query
        .as_deref()
        .map(str::trim)
        .filter(|query| !query.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| [artist, title].iter().filter(|part| !part.is_empty()).copied().collect::<Vec<_>>().join(" "));

    if !artist.is_empty() && !title.is_empty() {
        let url = reqwest::Url::parse_with_params("https://lrclib.net/api/get", &[("artist_name", artist), ("track_name", title)]).ok()?;
        if let Some(found) = get_json(&client, url).await.as_ref().and_then(from_lrclib) {
            return Some(found);
        }
    }
    if !query.is_empty() {
        let url = reqwest::Url::parse_with_params("https://lrclib.net/api/search", &[("q", query.as_str())]).ok()?;
        if let Some(Value::Array(entries)) = get_json(&client, url).await {
            if let Some(found) = entries.iter().find_map(from_lrclib) {
                return Some(found);
            }
        }
    }
    if !artist.is_empty() && !title.is_empty() {
        let mut url = reqwest::Url::parse("https://api.lyrics.ovh/v1/").ok()?;
        url.path_segments_mut().ok()?.pop_if_empty().push(artist).push(title);
        let data = get_json(&client, url).await?;
        let lyrics = data["lyrics"].as_str().unwrap_or_default().trim();
        if !lyrics.is_empty() {
            return Some(LyricsResult { artist: artist.to_owned(), title: title.to_owned(), lyrics: lyrics.to_owned(), instrumental: false });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn upload_noise_leaves_the_title() {
        assert_eq!(clean_track_title("Song (Official Video)"), "Song");
        assert_eq!(clean_track_title("Song [Clip Officiel] (Lyrics) {4K}"), "Song");
        assert_eq!(clean_track_title("Song (Live in Paris)"), "Song (Live in Paris)");
        assert_eq!(clean_track_title("Shape (of You"), "Shape (of You");
        assert_eq!(clean_track_title("Thin Air (HD)"), "Thin Air");
        assert_eq!(clean_track_title("Hard (Rock)"), "Hard (Rock)", "\"hd\" must be a whole word");
    }

    #[test]
    fn pieces_fit_and_cut_on_lines() {
        assert_eq!(chunk_lyrics("H", "aaa\nbbb\nccc", 7), ["H\naaa", "bbb\nccc"]);
        assert_eq!(chunk_lyrics("", "abcdefgh", 3), ["abc", "def", "gh"]);
        assert_eq!(chunk_lyrics("", "\n\n", 3), Vec::<String>::new());
        assert!(chunk_lyrics("Ş", &"ğ".repeat(10), 4).iter().all(|chunk| chunk.chars().count() <= 4));
    }

    #[test]
    fn placeholder_artists_are_absent() {
        let (query, label) = lyrics_input_from_track(Some("Unknown"), "Song (Official Video)");
        assert_eq!(query.artist, None);
        assert_eq!(query.title.as_deref(), Some("Song"));
        assert_eq!(label, "Song (Official Video)");
        let (query, label) = lyrics_input_from_track(Some("Tarkan"), "Şımarık");
        assert_eq!(query.artist.as_deref(), Some("Tarkan"));
        assert_eq!(label, "Tarkan — Şımarık");
    }

    #[test]
    fn lrclib_records_need_lyrics() {
        assert_eq!(from_lrclib(&json!({"artistName":"A","trackName":"T","plainLyrics":" la "})).unwrap().lyrics, "la");
        assert!(from_lrclib(&json!({"instrumental":true})).unwrap().instrumental);
        assert_eq!(from_lrclib(&json!({"plainLyrics":"  "})), None);
        assert_eq!(from_lrclib(&json!(null)), None);
    }
}
