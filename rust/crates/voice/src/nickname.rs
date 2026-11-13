//! While a track plays the whole nickname becomes the track ("♪ Song").

const MAX_NICKNAME_CHARS: usize = 30;
const PREFIX: &str = "♪ ";

/// TeamSpeak allows 3-30 characters. An empty title keeps the bot's own name.
pub fn now_playing_nickname(title: &str, fallback: &str) -> String {
    let clean = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.is_empty() {
        return fallback.to_owned();
    }
    let max = MAX_NICKNAME_CHARS - PREFIX.chars().count();
    if clean.chars().count() <= max {
        return format!("{PREFIX}{clean}");
    }
    let cut: String = clean.chars().take(max - 1).collect();
    format!("{PREFIX}{}…", cut.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_whole_nickname_becomes_the_track() {
        assert_eq!(now_playing_nickname("Tarkan - Kuzu Kuzu", "Müzik Botu"), "♪ Tarkan - Kuzu Kuzu");
    }

    #[test]
    fn long_titles_fit_the_limit() {
        let nick = now_playing_nickname("A very long song title that never seems to end at all", "Bot");
        assert_eq!(nick.chars().count(), 30);
        assert!(nick.starts_with("♪ A very long"));
        assert!(nick.ends_with('…'));
    }

    #[test]
    fn an_empty_title_keeps_the_bot_name() {
        assert_eq!(now_playing_nickname("   ", "Müzik Botu"), "Müzik Botu");
    }
}
