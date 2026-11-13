//! Chat messages. The TS6 client renders Markdown and limited HTML; everything from metadata
//! is escaped so a song title cannot add markup, links or extra lines.

use super::commands::HelpLine;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackLine {
    pub title: String,
    pub artist: Option<String>,
    pub duration: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct NowPlaying {
    pub title: String,
    pub artist: Option<String>,
    pub position: Option<f64>,
    pub duration: Option<f64>,
    pub paused: bool,
    pub upcoming: Vec<TrackLine>,
    pub total_queue_length: usize,
    pub queue_index: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutomationSource {
    Playlist,
    Radio,
}

/// What a bot's Auto-DJ and 24/7 settings mean right now, for `!help`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutomationSummary {
    pub auto_dj: Option<AutomationSource>,
    pub stay_in_channel: bool,
}

pub struct CustomLine {
    pub name: String,
    pub description: Option<String>,
}

fn escape_inline(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '\\' | '*' | '_' | '`' | '~' | '|') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

pub fn mm_ss(seconds: f64) -> String {
    let total = seconds.max(0.0).floor() as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

fn duration_suffix(duration: Option<f64>) -> String {
    duration.filter(|duration| *duration > 0.0).map(|duration| format!(" _({})_", mm_ss(duration))).unwrap_or_default()
}

/// The block for `!np`.
pub fn now_playing_message(input: &NowPlaying) -> String {
    let mut lines = vec!["## 🎵 Now Playing".to_owned(), String::new()];
    let artist = input.artist.as_deref().map(|artist| format!(" — *{}*", escape_inline(artist))).unwrap_or_default();
    lines.push(format!("**{}**{artist}", escape_inline(&input.title)));
    if let (Some(position), Some(duration)) = (input.position, input.duration.filter(|duration| *duration > 0.0)) {
        lines.push(format!("{} / {}", mm_ss(position), mm_ss(duration)));
    }
    if input.paused {
        lines.push(String::new());
        lines.push("> ⏸ Paused".to_owned());
    }
    if !input.upcoming.is_empty() {
        lines.push(String::new());
        lines.push("### Up Next".to_owned());
        for (index, item) in input.upcoming.iter().enumerate() {
            let artist = item.artist.as_deref().map(|artist| format!(" — {}", escape_inline(artist))).unwrap_or_default();
            lines.push(format!("{}. {}{artist}{}", index + 1, escape_inline(&item.title), duration_suffix(item.duration)));
        }
        let after = input.queue_index.map_or(0, |index| input.total_queue_length.saturating_sub(index + 1));
        let extra = after.saturating_sub(input.upcoming.len());
        if extra > 0 {
            lines.push(format!("_… and {extra} more in queue_"));
        }
    }
    lines.extend(
        [
            "",
            "<details>",
            "<summary>Playback controls</summary>",
            "",
            "Type in channel chat (not clickable buttons):",
            "- `!pause` — pause / resume",
            "- `!skip` or `!next` — skip track",
            "- `!prev` — previous track",
            "- `!stop` — stop playback and clear the queue",
            "- `!queue show` — full queue",
            "</details>",
        ]
        .map(str::to_owned),
    );
    lines.join("\n")
}

/// The list for `!help`.
pub fn help_message(builtin: &[HelpLine], custom: &[CustomLine], automation: Option<AutomationSummary>) -> String {
    let mut lines = vec!["## Music bot commands".to_owned(), String::new(), "### Built-in".to_owned()];
    lines.extend(builtin.iter().map(|entry| format!("- **{}** — {}", entry.usage, entry.blurb)));
    lines.push(String::new());
    lines.push("_Several bots in one channel? Add a bot's name: **!next Bot 2** or **!vol 30 @Bot 2**._".to_owned());
    if !custom.is_empty() {
        lines.push(String::new());
        lines.push("### Custom".to_owned());
        lines.extend(custom.iter().map(custom_line));
    }
    // Auto-DJ and 24/7 have no chat command, so say here when the answering bot has them on.
    if let Some(summary) = automation.filter(|summary| summary.auto_dj.is_some() || summary.stay_in_channel) {
        lines.push(String::new());
        lines.push("### This bot".to_owned());
        if let Some(source) = summary.auto_dj {
            let source = if source == AutomationSource::Radio { "a radio station" } else { "a saved playlist" };
            lines.push(format!("- **Auto-DJ is on** — when nothing is queued it plays {source}. Your requests always play first."));
        }
        if summary.stay_in_channel {
            lines.push("- **24/7 is on** — the bot stays in its channel.".to_owned());
        }
    }
    lines.join("\n")
}

fn custom_line(command: &CustomLine) -> String {
    let blurb = command.description.as_deref().map(str::trim).filter(|text| !text.is_empty()).unwrap_or("Custom reply");
    format!("- **!{}** — {blurb}", command.name)
}

/// The list for `!commands`.
pub fn custom_commands_message(custom: &[CustomLine]) -> String {
    if custom.is_empty() {
        return "_No custom chat commands are enabled. Ask an admin to seed presets in Music Bots → Commands._".to_owned();
    }
    let mut lines = vec!["## Custom commands".to_owned(), String::new()];
    lines.extend(custom.iter().map(custom_line));
    lines.push(String::new());
    lines.push("_Type `!help` for built-in music commands._".to_owned());
    lines.join("\n")
}

/// The listing for `!queue show`. `start` is the first position shown; `number_offset` is
/// added to every track number.
pub fn queue_message(items: &[TrackLine], current: Option<usize>, max_lines: usize, start: usize, number_offset: usize) -> String {
    if items.is_empty() {
        return "_Queue is empty._".to_owned();
    }
    let mut lines = vec![format!("## Queue ({} tracks)", items.len()), String::new()];
    if start > 0 {
        lines.push(format!("_… {start} earlier_"));
    }
    let shown = &items[start.min(items.len())..(start + max_lines).min(items.len())];
    for (offset, item) in shown.iter().enumerate() {
        let position = start + offset;
        let prefix = if current == Some(position) { "▶️" } else { "•" };
        let artist = item.artist.as_deref().map(|artist| format!("{} — ", escape_inline(artist))).unwrap_or_default();
        lines.push(format!(
            "{prefix} **{}.** {artist}{}{}",
            number_offset + position + 1,
            escape_inline(&item.title),
            duration_suffix(item.duration)
        ));
    }
    let after = items.len().saturating_sub(start + shown.len());
    if after > 0 {
        lines.push(String::new());
        lines.push(format!("_… and {after} more_"));
    }
    lines.join("\n")
}

pub fn radio_list_message(stations: &[(i64, String, Option<String>)]) -> String {
    let mut lines = vec!["## Radio stations".to_owned(), String::new()];
    for (id, name, genre) in stations {
        let genre = genre.as_deref().filter(|genre| !genre.is_empty()).map(|genre| format!(" _({})_", escape_inline(genre))).unwrap_or_default();
        lines.push(format!("- **{id}** — {}{genre}", escape_inline(name)));
    }
    lines.push(String::new());
    lines.push("_Play with `!radio <id>`_".to_owned());
    lines.join("\n")
}

pub fn unknown_command_message(command: &str, suggestion: Option<&str>) -> String {
    let shown = if command.chars().count() > 32 { format!("{}…", command.chars().take(32).collect::<String>()) } else { command.to_owned() };
    let hint = suggestion.map(|name| format!(" Did you mean **!{name}**?")).unwrap_or_default();
    format!("Unknown command **!{shown}**.{hint} Type **!help** for music commands or **!commands** for custom ones.")
}

/// Metadata in `!history`: one line, no markup or links.
pub fn history_text(value: &str, limit: usize) -> String {
    let flat = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let cut: String = flat.chars().filter(|c| !c.is_control()).take(limit).collect();
    let mut out = String::with_capacity(cut.len());
    for c in cut.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '[' => out.push_str("&#91;"),
            ']' => out.push_str("&#93;"),
            '\\' | '*' | '_' | '`' | '~' | '|' | '(' | ')' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

pub struct HereEntry {
    pub id: i64,
    pub name: String,
    pub here: bool,
    pub idle: Option<bool>,
}

/// Summon candidates with their current state.
pub fn here_list_message(bots: &[HereEntry], busy: bool) -> String {
    let lines: Vec<String> = bots
        .iter()
        .map(|bot| {
            let tag = if bot.here {
                " (already here)"
            } else if bot.idle == Some(false) && !busy {
                " (busy)"
            } else {
                ""
            };
            format!("[{}] {}{tag}", bot.id, bot.name)
        })
        .collect();
    if busy {
        return format!(
            "All music bots are busy with other users:\n{}\nUse !here <id> to move a specific bot (may leave their channel).",
            lines.join("\n")
        );
    }
    format!("Available bots:\n{}\nUse: !here <id>", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::commands::COMMAND_HELP;

    #[test]
    fn durations_read_as_minutes() {
        assert_eq!(mm_ss(0.0), "0:00");
        assert_eq!(mm_ss(65.9), "1:05");
        assert_eq!(mm_ss(-3.0), "0:00");
    }

    #[test]
    fn now_playing_shows_progress_pause_and_what_is_next() {
        let message = now_playing_message(&NowPlaying {
            title: "A*B".to_owned(),
            artist: Some("X_Y".to_owned()),
            position: Some(61.0),
            duration: Some(180.0),
            paused: true,
            upcoming: vec![TrackLine { title: "Next".to_owned(), artist: None, duration: Some(10.0) }],
            total_queue_length: 5,
            queue_index: Some(0),
        });
        assert!(message.contains(r"**A\*B** — *X\_Y*"));
        assert!(message.contains("1:01 / 3:00"));
        assert!(message.contains("> ⏸ Paused"));
        assert!(message.contains("1. Next _(0:10)_"));
        assert!(message.contains("_… and 3 more in queue_"));
        assert!(message.contains("`!pause`"));
    }

    #[test]
    fn help_lists_builtin_custom_and_this_bot() {
        let custom = [CustomLine { name: "rules".to_owned(), description: Some("  ".to_owned()) }];
        let summary = AutomationSummary { auto_dj: Some(AutomationSource::Radio), stay_in_channel: true };
        let message = help_message(&COMMAND_HELP, &custom, Some(summary));
        assert!(message.contains("- **!help** — Show this command list"));
        assert!(message.contains("- **!rules** — Custom reply"));
        assert!(message.contains("it plays a radio station"));
        assert!(message.contains("**24/7 is on**"));
        assert!(!help_message(&COMMAND_HELP, &[], None).contains("### Custom"));
        assert!(!help_message(&COMMAND_HELP, &[], None).contains("### This bot"));
    }

    #[test]
    fn queue_numbers_stay_stable_and_mark_the_current_track() {
        let items: Vec<TrackLine> = (0..20).map(|i| TrackLine { title: format!("T{i}"), ..TrackLine::default() }).collect();
        let message = queue_message(&items, Some(5), 15, 3, 10);
        assert!(message.starts_with("## Queue (20 tracks)"));
        assert!(message.contains("_… 3 earlier_"));
        assert!(message.contains("▶️ **16.** T5"));
        assert!(message.contains("• **14.** T3"));
        assert!(message.contains("_… and 2 more_"));
        assert_eq!(queue_message(&[], None, 15, 0, 0), "_Queue is empty._");
    }

    #[test]
    fn history_text_cannot_add_markup_or_lines() {
        assert_eq!(history_text("a\n[b](c)<d>&", 70), r"a &#91;b&#93;\(c\)&lt;d&gt;&amp;");
        assert_eq!(history_text("abcdef", 3), "abc");
    }

    #[test]
    fn unknown_commands_get_the_suggestion() {
        assert_eq!(
            unknown_command_message("plya", Some("play")),
            "Unknown command **!plya**. Did you mean **!play**? Type **!help** for music commands or **!commands** for custom ones."
        );
        assert!(unknown_command_message(&"x".repeat(40), None).contains(&format!("{}…", "x".repeat(32))));
    }

    #[test]
    fn here_list_tags_each_bot() {
        let bots = [
            HereEntry { id: 1, name: "A".to_owned(), here: true, idle: Some(true) },
            HereEntry { id: 2, name: "B".to_owned(), here: false, idle: Some(false) },
            HereEntry { id: 3, name: "C".to_owned(), here: false, idle: Some(true) },
        ];
        assert_eq!(here_list_message(&bots, false), "Available bots:\n[1] A (already here)\n[2] B (busy)\n[3] C\nUse: !here <id>");
        assert!(here_list_message(&bots, true).starts_with("All music bots are busy"));
    }
}
