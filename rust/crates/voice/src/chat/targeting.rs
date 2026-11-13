//! Several bots can sit in one channel and each hears every line. These pick the one that
//! answers: the bot the user named, otherwise the one playing, otherwise the lowest id.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelBot {
    pub id: i64,
    /// Bot name and connect nickname; the live nickname follows the track.
    pub names: Vec<String>,
    /// Playing or paused music, or streaming video.
    pub active: bool,
}

/// Commands without arguments, where a bare bot name (`!next Bot 2`) can only be a target.
const NO_ARG_COMMANDS: [&str; 14] = [
    "help", "commands", "stop", "pause", "skip", "next", "voteskip", "prev", "np", "nowplaying", "stopstream", "viewers",
    "remote", "history",
];

fn norm(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

fn find_by_name<'a>(bots: &'a [ChannelBot], name: &str) -> Option<&'a ChannelBot> {
    let wanted = norm(name);
    if wanted.is_empty() {
        return None;
    }
    bots.iter().find(|bot| bot.names.iter().any(|known| norm(known) == wanted))
}

/// Split a bot name off the arguments: `!next Bot 2` (only for commands without arguments) or
/// `!vol 30 @Bot 2` (any command). Unknown names stay, so `!playlist Test` still searches.
pub fn parse_bot_target(command: &str, args: &str, bots: &[ChannelBot]) -> (Option<i64>, String) {
    if NO_ARG_COMMANDS.contains(&command) {
        if let Some(bot) = find_by_name(bots, args) {
            return (Some(bot.id), String::new());
        }
    }
    if let Some(at) = args.rfind('@') {
        if at == 0 || args[..at].ends_with(char::is_whitespace) {
            if let Some(bot) = find_by_name(bots, &args[at + 1..]) {
                return (Some(bot.id), args[..at].trim().to_owned());
            }
        }
    }
    (None, args.to_owned())
}

/// The bot that should answer, or `None` when there are no bots to choose from.
pub fn choose_command_bot(bots: &[ChannelBot], target: Option<i64>) -> Option<i64> {
    if let Some(id) = target.filter(|id| bots.iter().any(|bot| bot.id == *id)) {
        return Some(id);
    }
    let mut by_id: Vec<&ChannelBot> = bots.iter().collect();
    by_id.sort_by_key(|bot| bot.id);
    by_id.iter().find(|bot| bot.active).or(by_id.first()).map(|bot| bot.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bots() -> Vec<ChannelBot> {
        vec![
            ChannelBot { id: 2, names: vec!["Bot 2".to_owned(), "İkinci".to_owned()], active: false },
            ChannelBot { id: 1, names: vec!["Bot 1".to_owned()], active: false },
            ChannelBot { id: 3, names: vec!["Radyo".to_owned()], active: true },
        ]
    }

    #[test]
    fn a_named_bot_answers() {
        assert_eq!(parse_bot_target("next", "bot  2", &bots()), (Some(2), String::new()));
        assert_eq!(parse_bot_target("vol", "30 @Bot 2", &bots()), (Some(2), "30".to_owned()));
        assert_eq!(parse_bot_target("play", "@radyo", &bots()), (Some(3), String::new()));
    }

    #[test]
    fn unknown_names_stay_in_the_arguments() {
        assert_eq!(parse_bot_target("playlist", "Bot 2", &bots()), (None, "Bot 2".to_owned()));
        assert_eq!(parse_bot_target("play", "song @nobody", &bots()), (None, "song @nobody".to_owned()));
        assert_eq!(parse_bot_target("play", "mail@Bot 2", &bots()), (None, "mail@Bot 2".to_owned()));
        assert_eq!(parse_bot_target("next", "", &bots()), (None, String::new()));
    }

    #[test]
    fn the_answer_is_named_then_playing_then_lowest() {
        assert_eq!(choose_command_bot(&bots(), Some(2)), Some(2));
        assert_eq!(choose_command_bot(&bots(), Some(99)), Some(3));
        assert_eq!(choose_command_bot(&bots(), None), Some(3));
        let mut idle = bots();
        idle[2].active = false;
        assert_eq!(choose_command_bot(&idle, None), Some(1));
        assert_eq!(choose_command_bot(&[], None), None);
    }
}
