//! Who may use media commands, stored per server as a JSON setting. A broken stored policy
//! closes the commands instead of opening them.

use serde_json::{json, Value};

use super::commands::{command_group, CommandGroup};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Access {
    Everyone,
    ServerGroups(Vec<i64>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Permissions {
    pub playback: Access,
    pub queue: Access,
    pub video: Access,
}

impl Default for Permissions {
    /// An absent policy keeps existing installations working.
    fn default() -> Self {
        Self { playback: Access::Everyone, queue: Access::Everyone, video: Access::Everyone }
    }
}

impl Permissions {
    /// A saved policy that cannot be read: nobody passes.
    pub fn closed() -> Self {
        let none = || Access::ServerGroups(Vec::new());
        Self { playback: none(), queue: none(), video: none() }
    }

    pub fn to_json(&self) -> Value {
        let access = |access: &Access| match access {
            Access::Everyone => json!({"mode": "everyone"}),
            Access::ServerGroups(ids) => json!({"mode": "server_groups", "serverGroupIds": ids}),
        };
        json!({"playback": access(&self.playback), "queue": access(&self.queue), "video": access(&self.video)})
    }

    /// The stored setting's value, or closed when it is not valid.
    pub fn from_stored(text: Option<&str>) -> Self {
        let Some(text) = text else { return Self::default() };
        serde_json::from_str::<Value>(text).ok().as_ref().and_then(parse).unwrap_or_else(Self::closed)
    }
}

pub fn permissions_key(server_config_id: i64, virtual_server_id: i64) -> String {
    format!("media_command_permissions:{server_config_id}:{virtual_server_id}")
}

fn parse_access(value: &Value) -> Option<Access> {
    let access = value.as_object()?;
    if access.get("mode")?.as_str()? == "everyone" {
        return (access.len() == 1).then_some(Access::Everyone);
    }
    if access["mode"] != "server_groups" || access.keys().any(|key| key != "mode" && key != "serverGroupIds") {
        return None;
    }
    let raw = access.get("serverGroupIds")?.as_array()?;
    if raw.len() > 256 {
        return None;
    }
    let mut ids = Vec::with_capacity(raw.len());
    for id in raw {
        let id = id.as_i64().filter(|id| *id > 0)?;
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    Some(Access::ServerGroups(ids))
}

/// Full validation: unknown groups or fields make the whole policy invalid.
pub fn parse(value: &Value) -> Option<Permissions> {
    let object = value.as_object()?;
    if object.keys().any(|key| !matches!(key.as_str(), "playback" | "queue" | "video")) {
        return None;
    }
    Some(Permissions {
        playback: parse_access(object.get("playback")?)?,
        queue: parse_access(object.get("queue")?)?,
        video: parse_access(object.get("video")?)?,
    })
}

/// What the server says about the client. Never taken from the client itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Identity {
    pub uid: String,
    pub server_group_ids: Vec<i64>,
}

/// `None` as identity means nothing is known, which is enough only for open commands.
pub fn authorize(policy: &Permissions, command: &str, args: &str, identity: Option<&Identity>) -> bool {
    let access = match command_group(command, args) {
        None => return false,
        // Votes have their own current-listener check.
        Some(CommandGroup::Info | CommandGroup::Listener) => return true,
        Some(CommandGroup::Playback) => &policy.playback,
        Some(CommandGroup::Queue) => &policy.queue,
        Some(CommandGroup::Video) => &policy.video,
    };
    match access {
        Access::Everyone => true,
        Access::ServerGroups(allowed) => identity.is_some_and(|identity| {
            !identity.uid.trim().is_empty() && identity.server_group_ids.iter().any(|id| allowed.contains(id))
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(groups: &[i64]) -> Identity {
        Identity { uid: "uid".to_owned(), server_group_ids: groups.to_vec() }
    }

    #[test]
    fn no_policy_means_everyone() {
        let policy = Permissions::from_stored(None);
        assert!(authorize(&policy, "play", "x", None));
        assert!(authorize(&policy, "stream", "x", None));
    }

    #[test]
    fn a_broken_policy_closes_the_controls_but_not_the_information() {
        for stored in ["nonsense", "{}", r#"{"playback":{"mode":"everyone"}}"#, r#"{"playback":{"mode":"everyone"},"queue":{"mode":"everyone"},"video":{"mode":"everyone"},"extra":{"mode":"everyone"}}"#] {
            let policy = Permissions::from_stored(Some(stored));
            assert_eq!(policy, Permissions::closed(), "{stored}");
            assert!(!authorize(&policy, "play", "x", Some(&identity(&[1]))));
            assert!(authorize(&policy, "np", "", None));
            assert!(authorize(&policy, "vol", "", None));
            assert!(!authorize(&policy, "vol", "30", None));
        }
    }

    #[test]
    fn invalid_groups_are_refused() {
        for groups in [r#"[0]"#, r#"[-1]"#, r#"["1"]"#, r#"1"#] {
            let stored = format!(r#"{{"playback":{{"mode":"server_groups","serverGroupIds":{groups}}},"queue":{{"mode":"everyone"}},"video":{{"mode":"everyone"}}}}"#);
            assert_eq!(Permissions::from_stored(Some(&stored)), Permissions::closed(), "{groups}");
        }
    }

    #[test]
    fn server_groups_are_checked() {
        let stored = r#"{"playback":{"mode":"server_groups","serverGroupIds":[6,6,7]},"queue":{"mode":"everyone"},"video":{"mode":"everyone"}}"#;
        let policy = Permissions::from_stored(Some(stored));
        assert_eq!(policy.playback, Access::ServerGroups(vec![6, 7]));
        assert!(authorize(&policy, "skip", "", Some(&identity(&[8, 7]))));
        assert!(!authorize(&policy, "skip", "", Some(&identity(&[8]))));
        assert!(!authorize(&policy, "skip", "", None));
        assert!(!authorize(&policy, "skip", "", Some(&Identity { uid: " ".to_owned(), server_group_ids: vec![7] })));
        assert!(authorize(&policy, "add", "u", None));
        assert!(authorize(&policy, "voteskip", "", None));
        assert!(!authorize(&policy, "unknown", "", Some(&identity(&[7]))));
    }

    #[test]
    fn the_settings_key_matches_the_old_program() {
        assert_eq!(permissions_key(1, 2), "media_command_permissions:1:2");
    }
}
