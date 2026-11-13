//! Ranking and showing `!ara` results. No YouTube Music: it gave poor matches and was removed.

use std::collections::{HashMap, HashSet};

use teamhub_media::youtube::SearchResult;

const MAX_SONG_SECONDS: f64 = 20.0 * 60.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchSource {
    Youtube,
    Spotify,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RankedResult {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub duration: f64,
    pub url: String,
    pub source: SearchSource,
    pub score: f64,
}

/// A catalogue entry; `url` is set when it is not a YouTube watch page.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub result: SearchResult,
    pub url: Option<String>,
}

fn normalise(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        // Turkish lower case and the letters whose base letter is what people type.
        let mapped = match c {
            'İ' | 'I' | 'ı' => 'i',
            'Ş' | 'ş' => 's',
            'Ğ' | 'ğ' => 'g',
            'Ü' | 'ü' => 'u',
            'Ö' | 'ö' => 'o',
            'Ç' | 'ç' => 'c',
            'Â' | 'â' | 'Á' | 'á' | 'À' | 'à' | 'Ä' | 'ä' => 'a',
            'É' | 'é' | 'È' | 'è' | 'Ê' | 'ê' | 'Ë' | 'ë' => 'e',
            'Î' | 'î' | 'Í' | 'í' | 'Ì' | 'ì' | 'Ï' | 'ï' => 'i',
            'Ó' | 'ó' | 'Ò' | 'ò' | 'Ô' | 'ô' => 'o',
            'Ú' | 'ú' | 'Ù' | 'ù' | 'Û' | 'û' => 'u',
            'Ñ' | 'ñ' => 'n',
            other => other.to_ascii_lowercase(),
        };
        out.push(if mapped.is_ascii_lowercase() || mapped.is_ascii_digit() || mapped.is_whitespace() { mapped } else { ' ' });
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn contains_any(haystack: &str, words: &[&str]) -> bool {
    let lower = haystack.to_lowercase();
    words.iter().any(|word| lower.contains(word))
}

/// Relevance and likely audio quality.
pub fn quality_score(title: &str, artist: &str, duration: f64, query: &str, source: SearchSource) -> f64 {
    let haystack = normalise(&format!("{artist} {title}"));
    let query = normalise(query);
    let terms: Vec<&str> = query.split(' ').filter(|term| !term.is_empty()).collect();
    let matched = terms.iter().filter(|term| haystack.contains(**term)).count();
    let mut score = if terms.is_empty() { 0.0 } else { matched as f64 / terms.len() as f64 * 60.0 };
    if source == SearchSource::Spotify {
        score += 12.0;
    }
    if contains_any(&format!("{artist} {title}"), &["official", "topic", "vevo", "audio", "provided to youtube"]) {
        score += 12.0;
    }
    if (75.0..=480.0).contains(&duration) {
        score += 8.0;
    }
    if contains_any(title, &["live", "reaction", "cover", "nightcore", "slowed", "reverb", "karaoke"]) {
        score -= 8.0;
    }
    score
}

const NOISE_WORDS: [&str; 13] =
    ["official", "video", "audio", "lyric", "lyrics", "sozleri", "klip", "clip", "hd", "hq", "4k", "music", "visualizer"];

/// Whether bracket text says nothing about the song: "(Official Video)", "[HD]".
fn is_noise(inner: &str) -> bool {
    let inner = normalise(inner).replace("visualiser", "visualizer");
    inner.split(' ').any(|word| NOISE_WORDS.contains(&word))
}

/// Remove "(Official Video)" style groups, "#tags" and "| 8 Track" tails.
fn strip_title_noise(raw: &str) -> String {
    let mut out = String::new();
    let mut rest = raw;
    while let Some(open) = rest.find(['(', '[']) {
        let closing = if rest[open..].starts_with('(') { ')' } else { ']' };
        let Some(length) = rest[open..].find(closing) else { break };
        let inner = &rest[open + 1..open + length];
        out.push_str(rest[..open].trim_end_matches(' '));
        if !is_noise(inner) || inner.contains(['(', '[']) {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&rest[open..=open + length]);
        }
        rest = &rest[open + length + 1..];
    }
    out.push_str(rest);
    let words: Vec<&str> = out.split_whitespace().filter(|word| !(word.starts_with('#') && word.len() > 1)).collect();
    let joined = words.join(" ");
    match joined.find(" |") {
        Some(at) => joined[..at].trim().to_owned(),
        None => joined.trim().to_owned(),
    }
}

/// "Tarkan - Topic" and "TarkanVEVO" are channel names, not artists.
fn clean_artist(artist: &str) -> String {
    let trimmed = artist.trim();
    let lower = trimmed.to_lowercase();
    let cleaned = if lower.ends_with(" - topic") {
        trimmed[..trimmed.len() - " - topic".len()].trim()
    } else if lower.ends_with("vevo") {
        trimmed[..trimmed.len() - "vevo".len()].trim()
    } else {
        trimmed
    };
    if cleaned.is_empty() { artist.to_owned() } else { cleaned.to_owned() }
}

/// A title such as "Heijan & Muti - BİRADER (Official Video) #HERMANO" holds the real artist and
/// song; the uploader often is only a channel.
pub fn clean_track(raw_title: &str, raw_artist: &str) -> (String, String) {
    let mut title = strip_title_noise(raw_title);
    let mut artist = clean_artist(raw_artist);
    let dash = [" - ", " – ", " — "].iter().filter_map(|mark| title.find(mark).map(|at| (at, mark.len()))).min();
    if let Some((at, length)) = dash.filter(|(at, length)| *at > 0 && at + length < title.len()) {
        artist = title[..at].trim().to_owned();
        title = title[at + length..].trim().to_owned();
    } else if let Some(body) = title.strip_suffix(')') {
        // "Birader (Osman Öztunç)" on "ZeMahşer Osman Öztunç TV": the name in brackets is the artist.
        if let Some(open) = body.rfind('(') {
            let credit = body[open + 1..].trim();
            if !credit.is_empty() && !body[..open].contains(['(', ')']) && normalise(raw_artist).contains(&normalise(credit)) {
                artist = credit.to_owned();
                title = body[..open].trim().to_owned();
            }
        }
    }
    let title = if title.is_empty() { raw_title.to_owned() } else { title };
    let artist = if artist.is_empty() { raw_artist.to_owned() } else { artist };
    (title, artist)
}

fn is_video_id(id: &str) -> bool {
    id.len() == 11 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Merge, drop what cannot play, keep the better copy of each song, best ten.
pub fn merge_and_rank(query: &str, groups: &[(SearchSource, Vec<Candidate>)]) -> Vec<RankedResult> {
    let mut unique: HashMap<String, RankedResult> = HashMap::new();
    for (source, items) in groups {
        for candidate in items {
            let raw = &candidate.result;
            if raw.id.is_empty() || unique.contains_key(&raw.id) {
                continue;
            }
            // Channels and playlists show up in YouTube search too.
            if *source == SearchSource::Youtube && !is_video_id(&raw.id) {
                continue;
            }
            if raw.duration > MAX_SONG_SECONDS {
                continue;
            }
            let score = quality_score(&raw.title, &raw.artist, raw.duration, query, *source);
            let (title, artist) = clean_track(&raw.title, &raw.artist);
            let url = candidate.url.clone().unwrap_or_else(|| format!("https://www.youtube.com/watch?v={}", raw.id));
            unique.insert(raw.id.clone(), RankedResult { id: raw.id.clone(), title, artist, duration: raw.duration, url, source: *source, score });
        }
    }
    let mut sorted: Vec<RankedResult> = unique.into_values().collect();
    sorted.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.title.cmp(&b.title)).then_with(|| a.id.cmp(&b.id)));
    let mut seen = HashSet::new();
    sorted.retain(|result| seen.insert(normalise(&format!("{} {}", result.artist, result.title))));
    sorted.truncate(10);
    sorted
}

fn markdown_text(text: &str, max: usize) -> String {
    let short: String = if text.chars().count() > max {
        format!("{}…", text.chars().take(max - 1).collect::<String>())
    } else {
        text.to_owned()
    };
    let mut out = String::with_capacity(short.len());
    for c in short.chars() {
        if matches!(c, '\\' | '*' | '_' | '`' | '~' | '|' | '[' | ']' | '<' | '>' | '#') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// One line per result, so splitting a long message at line breaks never cuts a result.
pub fn format_results(query: &str, results: &[RankedResult]) -> String {
    let mut lines = vec![format!("### 🔎 {}", markdown_text(query, 60))];
    for (index, item) in results.iter().enumerate() {
        let duration = if item.duration > 0.0 {
            let seconds = item.duration.floor() as u64;
            format!(" · `{}:{:02}`", seconds / 60, seconds % 60)
        } else {
            String::new()
        };
        let source = if item.source == SearchSource::Spotify { " · Spotify" } else { "" };
        lines.push(format!("**{}.** {} — *{}*{duration}{source}", index + 1, markdown_text(&item.title, 48), markdown_text(&item.artist, 32)));
    }
    lines.push(String::new());
    lines.push(format!("Çalmak için numarayı yaz: **1**–**{}**", results.len()));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: &str, title: &str, artist: &str, duration: f64) -> Candidate {
        Candidate {
            result: SearchResult { id: id.to_owned(), title: title.to_owned(), artist: artist.to_owned(), duration, thumbnail: String::new() },
            url: None,
        }
    }

    #[test]
    fn titles_lose_their_noise_and_give_the_artist() {
        assert_eq!(
            clean_track("Heijan & Muti - BİRADER (Official Video) #HERMANO", "AVA Media"),
            ("BİRADER".to_owned(), "Heijan & Muti".to_owned())
        );
        assert_eq!(clean_track("Kuzu Kuzu [HD]", "Tarkan - Topic"), ("Kuzu Kuzu".to_owned(), "Tarkan".to_owned()));
        assert_eq!(clean_track("Şarkı | 8 Track", "TarkanVEVO"), ("Şarkı".to_owned(), "Tarkan".to_owned()));
        assert_eq!(
            clean_track("Birader (Osman Öztunç)", "ZeMahşer Osman Öztunç TV"),
            ("Birader".to_owned(), "Osman Öztunç".to_owned())
        );
        assert_eq!(clean_track("Song (Remix)", "Someone"), ("Song (Remix)".to_owned(), "Someone".to_owned()));
    }

    #[test]
    fn the_better_matching_upload_ranks_higher() {
        let official = quality_score("Kuzu Kuzu (Official Audio)", "Tarkan", 240.0, "tarkan kuzu", SearchSource::Youtube);
        let cover = quality_score("Kuzu Kuzu cover", "Birisi", 900.0, "tarkan kuzu", SearchSource::Youtube);
        assert!(official > cover, "{official} {cover}");
        assert_eq!(quality_score("x", "y", 0.0, "", SearchSource::Youtube), 0.0);
    }

    #[test]
    fn turkish_letters_match_what_people_type() {
        assert_eq!(normalise("ŞARKI İçin ığ-üö"), "sarki icin ig uo");
    }

    #[test]
    fn what_cannot_play_is_dropped() {
        let results = merge_and_rank("x", &[(SearchSource::Youtube, vec![
            candidate("UCchannel-not-a-video", "Channel", "A", 0.0),
            candidate("aaaaaaaaaaa", "Good", "A", 100.0),
            candidate("bbbbbbbbbbb", "A film", "A", 3000.0),
            candidate("aaaaaaaaaaa", "Duplicate id", "A", 100.0),
        ])]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].url, "https://www.youtube.com/watch?v=aaaaaaaaaaa");
    }

    #[test]
    fn one_song_keeps_its_better_copy() {
        let results = merge_and_rank("tarkan kuzu", &[(SearchSource::Youtube, vec![
            candidate("aaaaaaaaaaa", "Tarkan - Kuzu Kuzu", "Bir Kanal", 240.0),
            candidate("bbbbbbbbbbb", "Tarkan - Kuzu Kuzu (Official Video)", "TarkanVEVO", 240.0),
        ])]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "bbbbbbbbbbb");
        assert_eq!(results[0].artist, "Tarkan");
    }

    #[test]
    fn spotify_tracks_keep_their_link_and_label() {
        let mut spotify = candidate("spotify:abc", "Song", "Artist", 200.0);
        spotify.url = Some("https://open.spotify.com/track/abc".to_owned());
        let results = merge_and_rank("song", &[(SearchSource::Spotify, vec![spotify])]);
        assert_eq!(results[0].url, "https://open.spotify.com/track/abc");
        assert!(format_results("song", &results).contains("· Spotify"));
    }

    #[test]
    fn at_most_ten_results_on_one_line_each() {
        let items: Vec<Candidate> = (0..15).map(|i| candidate(&format!("aaaaaaaaa{i:02}"), &format!("Song {i}"), "A", 100.0)).collect();
        let results = merge_and_rank("song", &[(SearchSource::Youtube, items)]);
        assert_eq!(results.len(), 10);
        let message = format_results("a*b", &results);
        assert!(message.starts_with(r"### 🔎 a\*b"));
        assert_eq!(message.lines().count(), 13);
        assert!(message.ends_with("Çalmak için numarayı yaz: **1**–**10**"));
    }
}
