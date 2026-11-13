//! TS command text format: `name key=value key=value|key=value`.
//! Ported from `voice/tslib/commands.ts`, including its live fixes.

use std::collections::BTreeMap;

/// Largest client-to-server command in UTF-8 bytes. A voice packet is at most 500 bytes
/// (8 MAC + 5 header + 487 payload); longer chat commands never appear in the channel.
pub const MAX_TS_COMMAND_BYTES: usize = 487;

pub fn ts_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '/' => out.push_str("\\/"),
            ' ' => out.push_str("\\s"),
            '|' => out.push_str("\\p"),
            '\u{0c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0b}' => out.push_str("\\v"),
            other => out.push(other),
        }
    }
    out
}

/// Never fails: a lone trailing backslash stays as text, and an unknown escape
/// (`\H` on some TS6 builds) keeps the following character.
pub fn ts_unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            None => out.push('\\'),
            Some('s') => out.push(' '),
            Some('p') => out.push('|'),
            Some('f') => out.push('\u{0c}'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('v') => out.push('\u{0b}'),
            Some(other) => out.push(other),
        }
    }
    out
}

pub type Params = BTreeMap<String, String>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCommand {
    pub name: String,
    pub params: Params,
    /// Every `|`-separated group, first one included; `None` for a single group.
    pub groups: Option<Vec<Params>>,
}

fn parse_tokens<'a>(tokens: impl Iterator<Item = &'a str>) -> Params {
    tokens
        .map(|token| match token.split_once('=') {
            Some((key, value)) => (key.to_owned(), ts_unescape(value)),
            None => (token.to_owned(), String::new()),
        })
        .collect()
}

pub fn parse_command(raw: &str) -> ParsedCommand {
    let mut parts = raw.split('|');
    let mut tokens = parts.next().unwrap_or_default().trim().split(' ');
    let name = ts_unescape(tokens.next().unwrap_or_default());
    let params = parse_tokens(tokens);
    let rest: Vec<&str> = parts.collect();
    let groups = (!rest.is_empty()).then(|| {
        let mut groups = vec![params.clone()];
        groups.extend(rest.iter().map(|part| parse_tokens(part.trim().split(' '))));
        groups
    });
    ParsedCommand { name, params, groups }
}

pub fn build_command(name: &str, params: &[(&str, &str)]) -> String {
    let mut cmd = ts_escape(name);
    for (key, value) in params {
        cmd.push(' ');
        cmd.push_str(key);
        cmd.push('=');
        cmd.push_str(&ts_escape(value));
    }
    cmd
}

fn largest_fitting_prefix(text: &str, build: &impl Fn(&str) -> String) -> usize {
    let ends: Vec<usize> = text.char_indices().map(|(i, c)| i + c.len_utf8()).collect();
    let fits = ends.partition_point(|&end| build(&text[..end]).len() <= MAX_TS_COMMAND_BYTES);
    // A single character always goes through, so the text can never stall.
    if fits == 0 { ends.first().copied().unwrap_or(0) } else { ends[fits - 1] }
}

/// `sendtextmessage` commands that each fit one voice packet. Pieces prefer a newline
/// boundary and concatenate back to `message` with nothing added.
pub fn build_send_text_commands(message: &str, target_mode: u8, target: Option<u64>) -> Vec<String> {
    let mode = target_mode.to_string();
    let target = target.filter(|_| target_mode == 1).map(|t| t.to_string());
    let build = |piece: &str| {
        let mut params = vec![("targetmode", mode.as_str())];
        if let Some(target) = &target {
            params.push(("target", target));
        }
        params.push(("msg", piece));
        build_command("sendtextmessage", &params)
    };
    if build(message).len() <= MAX_TS_COMMAND_BYTES {
        return vec![build(message)];
    }
    let mut commands = Vec::new();
    let mut rest = message;
    while !rest.is_empty() {
        let mut take = largest_fitting_prefix(rest, &build);
        if take < rest.len() {
            // Keep list lines intact unless the boundary is at the very start.
            if let Some(newline) = rest[..take].rfind('\n').filter(|&n| n >= 40) {
                take = newline + 1;
            }
        }
        commands.push(build(&rest[..take]));
        rest = &rest[take..];
    }
    commands
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_is_one_command() {
        assert_eq!(build_send_text_commands("hi", 2, None), ["sendtextmessage targetmode=2 msg=hi"]);
        assert_eq!(build_send_text_commands("hi", 1, Some(3)), ["sendtextmessage targetmode=1 target=3 msg=hi"]);
    }

    #[test]
    fn long_text_is_split_into_single_packets_and_loses_nothing() {
        let message: String = (0..200).map(|i| format!("**{i}.** şarkı adı — *sanatçı* · `3:21`\n")).collect();
        let commands = build_send_text_commands(&message, 2, None);
        assert!(commands.len() > 1);
        let mut joined = String::new();
        for command in &commands {
            assert!(command.len() <= MAX_TS_COMMAND_BYTES, "{} bytes", command.len());
            joined.push_str(&parse_command(command).params["msg"]);
        }
        assert_eq!(joined, message);
        // Every piece but the last ends on a line break.
        assert!(commands[..commands.len() - 1].iter().all(|c| parse_command(c).params["msg"].ends_with('\n')));
    }

    #[test]
    fn text_that_grows_when_escaped_still_fits() {
        let commands = build_send_text_commands(&" ".repeat(2000), 2, None);
        assert!(commands.iter().all(|c| c.len() <= MAX_TS_COMMAND_BYTES));
        let total: usize = commands.iter().map(|c| parse_command(c).params["msg"].len()).sum();
        assert_eq!(total, 2000);
    }


    #[test]
    fn escaping_round_trips() {
        let text = "a b|c/d\\e\nf\tg";
        assert_eq!(ts_unescape(&ts_escape(text)), text);
        assert_eq!(ts_escape("a b"), "a\\sb");
    }

    #[test]
    fn a_lone_trailing_backslash_is_kept_as_text() {
        assert_eq!(ts_unescape("Don\\"), "Don\\");
        assert_eq!(parse_command("notifytextmessage msg=hello\\").params["msg"], "hello\\");
    }

    #[test]
    fn an_unknown_escape_keeps_the_next_character() {
        assert_eq!(ts_unescape("a\\Hb"), "aHb");
    }

    #[test]
    fn parses_params_flags_and_groups() {
        let parsed = parse_command("clientlist cid=1 -away|cid=2 client_nickname=A\\sB");
        assert_eq!(parsed.name, "clientlist");
        assert_eq!(parsed.params["cid"], "1");
        assert_eq!(parsed.params["-away"], "");
        let groups = parsed.groups.expect("two groups");
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[1]["client_nickname"], "A B");
    }

    #[test]
    fn a_value_may_contain_an_equals_sign() {
        assert_eq!(parse_command("x a=b=c").params["a"], "b=c");
    }

    #[test]
    fn builds_escaped_commands() {
        assert_eq!(
            build_command("clientupdate", &[("client_nickname", "♪ My Song")]),
            "clientupdate client_nickname=♪\\sMy\\sSong"
        );
    }
}
