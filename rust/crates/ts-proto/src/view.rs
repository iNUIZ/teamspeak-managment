//! What the client knows about the server: channels, who is where, and its own channel.

use std::collections::{HashMap, HashSet};

use crate::commands::{ParsedCommand, Params};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelInfo {
    pub cid: u64,
    pub name: String,
    pub parent_id: u64,
    pub order: u64,
}

/// One client of a `notifycliententerview`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnterViewEntry {
    pub clid: u64,
    pub cid: u64,
    pub is_query: bool,
}

#[derive(Debug, Default, Clone)]
pub struct View {
    pub client_id: u64,
    pub current_channel: u64,
    pub channels: HashMap<u64, ChannelInfo>,
    pub client_channels: HashMap<u64, u64>,
    pub channel_members: HashSet<u64>,
    pub query_members: HashSet<u64>,
}

/// Like JavaScript's `parseInt`: leading digits, or 0.
pub fn number(value: Option<&String>) -> u64 {
    value.map_or(0, |text| {
        let digits = text.trim_start();
        let end = digits.find(|c: char| !c.is_ascii_digit()).unwrap_or(digits.len());
        digits[..end].parse().unwrap_or(0)
    })
}

fn first_number(entry: &Params, keys: &[&str]) -> u64 {
    keys.iter().find_map(|key| entry.get(*key).filter(|v| !v.is_empty())).map_or(0, |v| number(Some(v)))
}

fn entries(parsed: &ParsedCommand) -> &[Params] {
    parsed.groups.as_deref().unwrap_or(std::slice::from_ref(&parsed.params))
}

fn is_query(entry: &Params) -> bool {
    entry.get("client_type").is_some_and(|v| v == "1")
}

/// The clients of one enter-view message. The fields they share (`ctid`, `cfid`,
/// `reasonid`) are sent in the first entry only, so later entries inherit them.
pub fn parse_enter_view(parsed: &ParsedCommand) -> Vec<EnterViewEntry> {
    entries(parsed)
        .iter()
        .filter_map(|entry| {
            let clid = number(entry.get("clid"));
            let mut cid = first_number(entry, &["ctid", "cid"]);
            if cid == 0 {
                cid = first_number(&parsed.params, &["ctid", "cid"]);
            }
            (clid != 0 && cid != 0).then(|| EnterViewEntry { clid, cid, is_query: is_query(entry) })
        })
        .collect()
}

impl View {
    pub fn clear_members(&mut self) {
        self.channel_members.clear();
        self.query_members.clear();
    }

    pub fn channel_id_by_name(&self, name: &str) -> Option<u64> {
        self.channels.values().find(|c| c.name == name).map(|c| c.cid)
    }

    /// Channels in server order within each parent.
    pub fn list_channels(&self) -> Vec<ChannelInfo> {
        let mut list: Vec<_> = self.channels.values().cloned().collect();
        list.sort_by_key(|c| (c.parent_id, c.order, c.cid));
        list
    }

    /// Learn home from our own entry, then track the others in it. A known home is
    /// never overwritten: a stale enter-view can race and clear peers mid-summon.
    /// Returns true when home was learned here.
    pub fn apply_enter_view(&mut self, entries: &[EnterViewEntry]) -> bool {
        let mut learned = false;
        if self.current_channel == 0 {
            if let Some(own) = entries.iter().find(|e| e.clid == self.client_id) {
                self.current_channel = own.cid;
                self.clear_members();
                learned = true;
            }
        }
        for entry in entries {
            self.client_channels.insert(entry.clid, entry.cid);
            if entry.clid == self.client_id || entry.cid != self.current_channel {
                continue;
            }
            if entry.is_query {
                self.query_members.insert(entry.clid);
            } else {
                self.channel_members.insert(entry.clid);
            }
        }
        learned
    }

    /// Returns true when our own client left the view.
    pub fn apply_left_view(&mut self, params: &Params) -> bool {
        let clid = number(params.get("clid"));
        if clid != 0 {
            self.channel_members.remove(&clid);
            self.query_members.remove(&clid);
            self.client_channels.remove(&clid);
        }
        clid != 0 && clid == self.client_id
    }

    /// Returns true when we were moved somewhere we did not expect and must refresh peers.
    pub fn apply_moved(&mut self, params: &Params) -> bool {
        let clid = number(params.get("clid"));
        let to = number(params.get("ctid"));
        if clid != 0 && to != 0 {
            self.client_channels.insert(clid, to);
        }
        if clid != 0 && clid == self.client_id {
            // An optimistic move already set the channel and may hold occupants that
            // arrived before this confirmation; clearing again would lose them.
            if self.current_channel != to {
                self.current_channel = to;
                self.clear_members();
                return true;
            }
            return false;
        }
        if clid != 0 {
            let from = number(params.get("cfid"));
            let current = self.current_channel;
            let target = if is_query(params) { &mut self.query_members } else { &mut self.channel_members };
            if from == current {
                target.remove(&clid);
            }
            if to == current {
                target.insert(clid);
            }
        }
        false
    }

    pub fn apply_channel_list(&mut self, parsed: &ParsedCommand) {
        for entry in entries(parsed) {
            let cid = number(entry.get("cid"));
            let Some(name) = entry.get("channel_name").filter(|n| !n.is_empty()) else {
                continue;
            };
            if cid != 0 {
                self.channels.insert(
                    cid,
                    ChannelInfo {
                        cid,
                        name: name.clone(),
                        parent_id: first_number(entry, &["cpid", "pid"]),
                        order: number(entry.get("channel_order")),
                    },
                );
            }
        }
    }

    pub fn apply_channel_edited(&mut self, params: &Params) {
        let cid = number(params.get("cid"));
        if let (Some(channel), Some(name)) = (self.channels.get_mut(&cid), params.get("channel_name")) {
            channel.name = name.clone();
        }
    }

    pub fn apply_channel_deleted(&mut self, params: &Params) {
        self.channels.remove(&number(params.get("cid")));
    }

    /// Returns true when home was learned from this snapshot, so it must be requested again.
    pub fn apply_client_list(&mut self, parsed: &ParsedCommand) -> bool {
        let list = entries(parsed);
        let mut learned = false;
        if self.current_channel == 0 && self.client_id != 0 {
            if let Some(own) = list.iter().find(|e| number(e.get("clid")) == self.client_id) {
                let cid = first_number(own, &["cid", "client_channel_id", "ctid", "channel_id"]);
                if cid != 0 {
                    self.current_channel = cid;
                    learned = true;
                }
            }
        }
        for entry in list {
            let clid = number(entry.get("clid"));
            let cid = first_number(entry, &["cid", "client_channel_id"]);
            if clid == 0 || cid == 0 {
                continue;
            }
            self.client_channels.insert(clid, cid);
            if clid == self.client_id || self.current_channel == 0 || cid != self.current_channel {
                continue;
            }
            if is_query(entry) {
                self.query_members.insert(clid);
            } else {
                self.channel_members.insert(clid);
            }
        }
        learned
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::parse_command;

    #[test]
    fn enter_view_entries_inherit_the_shared_channel() {
        let parsed = parse_command("notifycliententerview ctid=5 clid=1 client_type=0|clid=2 client_type=1|clid=9");
        let list = parse_enter_view(&parsed);
        assert_eq!(list.len(), 3);
        assert!(list.iter().all(|e| e.cid == 5));
        assert!(list[1].is_query);
    }

    #[test]
    fn own_entry_later_in_the_message_still_counts_earlier_peers() {
        let mut view = View { client_id: 9, ..View::default() };
        let parsed = parse_command("notifycliententerview ctid=5 clid=1 client_type=0|clid=2 client_type=1|clid=9");
        assert!(view.apply_enter_view(&parse_enter_view(&parsed)));
        assert_eq!(view.current_channel, 5);
        assert!(view.channel_members.contains(&1));
        assert!(view.query_members.contains(&2));
        assert!(!view.channel_members.contains(&9));
    }

    #[test]
    fn a_known_home_is_not_overwritten_by_a_stale_enter_view() {
        let mut view = View { client_id: 9, current_channel: 7, ..View::default() };
        let parsed = parse_command("notifycliententerview ctid=5 clid=9");
        assert!(!view.apply_enter_view(&parse_enter_view(&parsed)));
        assert_eq!(view.current_channel, 7);
    }

    #[test]
    fn peers_are_tracked_across_moves_and_leaving() {
        let mut view = View { client_id: 9, current_channel: 5, ..View::default() };
        view.apply_moved(&parse_command("notifyclientmoved clid=3 cfid=1 ctid=5").params);
        assert!(view.channel_members.contains(&3));
        assert_eq!(view.client_channels[&3], 5);
        view.apply_moved(&parse_command("notifyclientmoved clid=3 cfid=5 ctid=1").params);
        assert!(!view.channel_members.contains(&3));
        assert!(!view.apply_left_view(&parse_command("notifyclientleftview clid=3").params));
        assert!(!view.client_channels.contains_key(&3));
        assert!(view.apply_left_view(&parse_command("notifyclientleftview clid=9").params));
    }

    #[test]
    fn a_confirmed_optimistic_move_keeps_its_peers() {
        let mut view = View { client_id: 9, current_channel: 5, ..View::default() };
        view.channel_members.insert(3);
        assert!(!view.apply_moved(&parse_command("notifyclientmoved clid=9 ctid=5").params));
        assert!(view.channel_members.contains(&3));
        assert!(view.apply_moved(&parse_command("notifyclientmoved clid=9 ctid=6").params));
        assert!(view.channel_members.is_empty());
    }

    #[test]
    fn client_list_learns_home_and_seeds_peers() {
        let mut view = View { client_id: 9, ..View::default() };
        let parsed = parse_command("clientlist clid=9 cid=4 client_type=0|clid=3 cid=4 client_type=0|clid=2 cid=4 client_type=1|clid=8 cid=1");
        assert!(view.apply_client_list(&parsed));
        assert_eq!(view.current_channel, 4);
        assert!(view.channel_members.contains(&3));
        assert!(view.query_members.contains(&2));
        assert_eq!(view.client_channels[&8], 1);
        assert!(!view.channel_members.contains(&8));
        assert!(!view.apply_client_list(&parsed));
    }

    #[test]
    fn channels_are_listed_edited_and_deleted() {
        let mut view = View::default();
        view.apply_channel_list(&parse_command("channellist cid=2 cpid=0 channel_name=B channel_order=1|cid=1 cpid=0 channel_name=A\\sA channel_order=0"));
        assert_eq!(view.list_channels().iter().map(|c| c.cid).collect::<Vec<_>>(), [1, 2]);
        assert_eq!(view.channel_id_by_name("A A"), Some(1));
        view.apply_channel_edited(&parse_command("notifychanneledited cid=1 channel_name=C").params);
        assert_eq!(view.channel_id_by_name("C"), Some(1));
        view.apply_channel_deleted(&parse_command("notifychanneldeleted cid=1").params);
        assert_eq!(view.channel_id_by_name("C"), None);
    }
}
