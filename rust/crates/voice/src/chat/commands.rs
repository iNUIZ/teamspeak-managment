//! The built-in chat commands: names, help, aliases and who may use them.

pub const BUILTIN_COMMANDS: [&str; 36] = [
    "help", "commands", "here", "come", "radio", "play", "ara", "say", "history", "again", "stop", "pause", "skip",
    "next", "voteskip", "remote", "prev", "vol", "volume", "np", "nowplaying", "queue", "add", "shuffle", "playlist",
    "pl", "repeat", "seek", "remove", "stream", "stopstream", "viewers", "channels", "tv", "iptv", "lyrics",
];

pub struct HelpLine {
    pub usage: &'static str,
    pub blurb: &'static str,
}

const fn help(usage: &'static str, blurb: &'static str) -> HelpLine {
    HelpLine { usage, blurb }
}

/// Shown by `!help`.
pub const COMMAND_HELP: [HelpLine; 36] = [
    help("!help", "Show this command list"),
    help("!say [--tr|--en] <text>", "Speak using local TTS (Bot settings > Extensions and commands > Text to speech)"),
    help("!commands", "List enabled custom chat commands"),
    help("!ara <song or artist>", "Search YouTube (and Spotify when set up) and pick from 10 results"),
    help("!here [id]", "Bring a music bot to your channel (prefers idle bots); works from channel chat or a private message to the bot"),
    help("!come [id]", "Alias for !here"),
    help("!play <url|song name>", "Play a YouTube / Spotify / Apple Music link, or the best YouTube match for a song name"),
    help("!queue [url|show|clear|remove <n>|play <n>]", "Show queue or add a URL"),
    help("!history", "Show the 10 most recent requests for this bot"),
    help("!again [id]", "Replay a history request (latest by default); queue it while music is playing"),
    help("!add <url>", "Alias for !queue <url>"),
    help("!playlist [name-or-id]", "List or queue playlists for this server/bot"),
    help("!pl <name-or-id>", "Alias for !playlist"),
    help("!repeat [off|track|queue]", "Show or set repeat mode"),
    help("!seek <seconds|+seconds|-seconds>", "Seek within a local/downloaded track"),
    help("!remove <text>", "Remove an unambiguous upcoming title/artist match"),
    help("!shuffle [on|off]", "Toggle or set queue shuffle"),
    help("!stop", "Stop playback and clear the queue"),
    help("!pause", "Pause / resume"),
    help("!skip", "Skip to next track"),
    help("!voteskip", "Vote to skip: more than half of human listeners in the bot channel"),
    help("!remote [bot name]", "Get a private one-time listener remote link for a bot in your channel"),
    help("!next", "Alias for !skip"),
    help("!prev", "Previous track"),
    help("!vol [0-100]", "Show or set volume for music, radio, video, and IPTV"),
    help("!volume [0-100]", "Alias for !vol"),
    help("!np", "Now playing"),
    help("!nowplaying", "Alias for !np"),
    help("!radio [id]", "List or play radio stations"),
    help("!stream <url> [preset]", "Start video stream (preset: auto, 480p–2160p)"),
    help("!stopstream", "Stop video stream"),
    help("!viewers", "List stream viewers"),
    help("!channels [search]", "List IPTV channels"),
    help("!tv <name>", "Stream an IPTV channel"),
    help("!iptv <name>", "Alias for !tv"),
    help("!lyrics [artist - title]", "Show lyrics for now playing or search"),
];

pub fn is_builtin(name: &str) -> bool {
    BUILTIN_COMMANDS.contains(&name)
}

/// The name settings use for a command and its aliases.
pub fn canonical_command(name: &str) -> &str {
    match name {
        "come" => "here",
        "pl" => "playlist",
        "next" => "skip",
        "volume" => "vol",
        "nowplaying" => "np",
        "add" => "queue",
        "iptv" => "tv",
        other => other,
    }
}

/// User input as a command name: no leading `!`, lower case, safe characters only.
pub fn normalize_command_name(raw: &str) -> String {
    raw.trim()
        .trim_start_matches('!')
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_' || *c == '-')
        .take(32)
        .collect()
}

fn edit_distance(a: &[char], b: &[char]) -> usize {
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

/// The closest built-in to a mistyped name (`plya` gives `play`), when one is close.
pub fn suggest_builtin(raw: &str) -> Option<&'static str> {
    let name: Vec<char> = raw.to_lowercase().chars().collect();
    if !(2..=32).contains(&name.len()) {
        return None;
    }
    let limit = if name.len() <= 3 { 1 } else { 2 };
    let mut best = None;
    let mut best_distance = usize::MAX;
    for candidate in BUILTIN_COMMANDS {
        let distance = edit_distance(&name, &candidate.chars().collect::<Vec<_>>());
        if distance < best_distance {
            best = Some(candidate);
            best_distance = distance;
        }
    }
    best.filter(|_| best_distance <= limit)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandGroup {
    Info,
    Listener,
    Playback,
    Queue,
    Video,
}

/// Which permission a command form needs. Read-only forms stay public when controls are
/// restricted. `None` for a name that is not built in.
pub fn command_group(command: &str, args: &str) -> Option<CommandGroup> {
    let value = args.trim();
    if command == "queue" && (value.is_empty() || value.eq_ignore_ascii_case("show")) {
        return Some(CommandGroup::Info);
    }
    if value.is_empty() && matches!(command, "vol" | "volume" | "playlist" | "pl" | "radio" | "repeat") {
        return Some(CommandGroup::Info);
    }
    Some(match command {
        "help" | "commands" | "np" | "nowplaying" | "viewers" | "channels" | "lyrics" | "history" => CommandGroup::Info,
        "here" | "come" | "radio" | "play" | "ara" | "say" | "again" | "stop" | "pause" | "skip" | "next" | "prev"
        | "vol" | "volume" | "repeat" | "seek" => CommandGroup::Playback,
        "queue" | "add" | "shuffle" | "playlist" | "pl" | "remove" => CommandGroup::Queue,
        "stream" | "stopstream" | "tv" | "iptv" => CommandGroup::Video,
        "voteskip" | "remote" => CommandGroup::Listener,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_has_help_and_a_group() {
        for name in BUILTIN_COMMANDS {
            let usage = format!("!{name}");
            assert!(COMMAND_HELP.iter().any(|line| line.usage == usage || line.usage.starts_with(&format!("{usage} "))), "{name}");
            assert!(command_group(name, "x").is_some(), "{name}");
        }
    }

    #[test]
    fn aliases_share_one_settings_name() {
        for (alias, name) in [("come", "here"), ("pl", "playlist"), ("next", "skip"), ("volume", "vol"), ("nowplaying", "np"), ("add", "queue"), ("iptv", "tv")] {
            assert_eq!(canonical_command(alias), name);
        }
        assert_eq!(canonical_command("play"), "play");
    }

    #[test]
    fn typing_mistakes_get_a_suggestion() {
        assert_eq!(suggest_builtin("plya"), Some("play"));
        assert_eq!(suggest_builtin("HELP"), Some("help"));
        assert_eq!(suggest_builtin("voluem"), Some("volume"));
        assert_eq!(suggest_builtin("completelydifferent"), None);
        assert_eq!(suggest_builtin("x"), None);
    }

    #[test]
    fn names_are_normalized() {
        assert_eq!(normalize_command_name(" !!Ku-ral_1 ş "), "ku-ral_1");
    }

    #[test]
    fn read_only_forms_are_public() {
        assert_eq!(command_group("queue", ""), Some(CommandGroup::Info));
        assert_eq!(command_group("queue", "SHOW"), Some(CommandGroup::Info));
        assert_eq!(command_group("queue", "clear"), Some(CommandGroup::Queue));
        assert_eq!(command_group("vol", ""), Some(CommandGroup::Info));
        assert_eq!(command_group("vol", "30"), Some(CommandGroup::Playback));
        assert_eq!(command_group("radio", ""), Some(CommandGroup::Info));
        assert_eq!(command_group("radio", "1"), Some(CommandGroup::Playback));
        assert_eq!(command_group("stream", "u"), Some(CommandGroup::Video));
        assert_eq!(command_group("voteskip", ""), Some(CommandGroup::Listener));
        assert_eq!(command_group("nope", ""), None);
    }
}
