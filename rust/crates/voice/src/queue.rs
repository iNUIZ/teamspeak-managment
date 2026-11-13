//! The play queue: order, shuffle, repeat and the history kept for "previous".

use std::collections::HashSet;

/// Most tracks one queue holds, played and waiting together.
pub const MAX_QUEUE_ITEMS: usize = 5000;
/// Played tracks kept behind the current one. Older ones are pruned, except under repeat-all.
pub const PLAYED_HISTORY_LIMIT: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatMode {
    Off,
    Track,
    Queue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueFull {
    pub requested: usize,
    pub room: usize,
}

impl std::fmt::Display for QueueFull {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.requested > 1 {
            write!(
                f,
                "The queue is full: only {} of these {} tracks fit (limit {MAX_QUEUE_ITEMS}), so none were queued.",
                self.room, self.requested
            )
        } else {
            write!(f, "The queue is full (limit {MAX_QUEUE_ITEMS} tracks).")
        }
    }
}

impl std::error::Error for QueueFull {}

/// Positions in the public methods are display positions: with shuffle on, the shuffled order.
#[derive(Debug, Clone)]
pub struct PlayQueue<T> {
    items: Vec<T>,
    /// `None` before the first track.
    current: Option<usize>,
    exhausted: bool,
    shuffle: bool,
    repeat: RepeatMode,
    shuffle_order: Vec<usize>,
    pruned: usize,
}

impl<T> Default for PlayQueue<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            current: None,
            exhausted: false,
            shuffle: false,
            repeat: RepeatMode::Off,
            shuffle_order: Vec::new(),
            pruned: 0,
        }
    }
}

fn shuffle_in_place(list: &mut [usize]) {
    for i in (1..list.len()).rev() {
        list.swap(i, fastrand::usize(0..=i));
    }
}

impl<T> PlayQueue<T> {
    pub fn new() -> Self {
        Self::default()
    }

    fn item_at(&self, position: usize) -> Option<&T> {
        let index = if self.shuffle { *self.shuffle_order.get(position)? } else { position };
        self.items.get(index)
    }

    pub fn current(&self) -> Option<&T> {
        if self.exhausted {
            return None;
        }
        self.item_at(self.current?)
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn index(&self) -> Option<usize> {
        self.current
    }

    pub fn repeat(&self) -> RepeatMode {
        self.repeat
    }

    pub fn shuffle(&self) -> bool {
        self.shuffle
    }

    /// Played tracks pruned since the last clear. Chat numbers add it so they stay stable.
    pub fn pruned(&self) -> usize {
        self.pruned
    }

    pub fn room(&self) -> usize {
        MAX_QUEUE_ITEMS.saturating_sub(self.items.len())
    }

    pub fn get_all(&self) -> Vec<&T> {
        (0..self.items.len()).filter_map(|position| self.item_at(position)).collect()
    }

    /// Tracks after the current one, in display order.
    pub fn upcoming(&self) -> Vec<&T> {
        let start = self.current.map_or(0, |current| current + 1);
        (start..self.items.len()).filter_map(|position| self.item_at(position)).collect()
    }

    pub fn upcoming_count(&self) -> usize {
        self.items.len().saturating_sub(self.current.map_or(0, |current| current + 1))
    }

    /// Append one track; a full queue changes nothing.
    pub fn add(&mut self, item: T) -> Result<(), QueueFull> {
        if self.room() == 0 {
            return Err(QueueFull { requested: 1, room: 0 });
        }
        self.items.push(item);
        if self.shuffle {
            // Among the upcoming tracks, so appending never changes the current one.
            let start = self.current.map_or(0, |current| current + 1);
            let position = fastrand::usize(start..=self.shuffle_order.len());
            self.shuffle_order.insert(position, self.items.len() - 1);
        }
        Ok(())
    }

    /// Append every track or, when they do not all fit, none of them.
    pub fn add_many(&mut self, items: Vec<T>) -> Result<(), QueueFull> {
        if items.len() > self.room() {
            return Err(QueueFull { requested: items.len(), room: self.room() });
        }
        for item in items {
            self.add(item)?;
        }
        Ok(())
    }

    /// Remove a displayed position.
    pub fn remove_at(&mut self, position: usize) -> bool {
        if position >= self.items.len() {
            return false;
        }
        let index = if self.shuffle { self.shuffle_order[position] } else { position };
        self.items.remove(index);
        if self.shuffle {
            self.shuffle_order.retain(|entry| *entry != index);
            for entry in &mut self.shuffle_order {
                if *entry > index {
                    *entry -= 1;
                }
            }
        }
        if let Some(current) = self.current {
            if position < current {
                self.current = Some(current - 1);
            } else if position == current {
                // The next track takes this place; the last one stays selected at the end.
                self.current = self.items.len().checked_sub(1).map(|last| current.min(last));
            }
        }
        true
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.current = None;
        self.exhausted = false;
        self.shuffle_order.clear();
        self.pruned = 0;
    }

    /// Keep every item and make the current one upcoming again.
    pub fn rewind(&mut self) {
        self.current = None;
        self.exhausted = false;
    }

    /// Advance. Track repeat is the bot's job: here the queue always moves on.
    pub fn next(&mut self) -> Option<&T> {
        if self.items.is_empty() {
            return None;
        }
        self.exhausted = false;
        let next = self.current.map_or(0, |current| current + 1);
        if next >= self.items.len() {
            if self.repeat == RepeatMode::Queue {
                self.current = Some(0);
                if self.shuffle {
                    self.regenerate_shuffle_order();
                }
            } else {
                // Keep the position so a later request never replays history.
                self.current = Some(self.items.len() - 1);
                self.exhausted = true;
                self.prune_played();
                return None;
            }
        } else {
            self.current = Some(next);
        }
        self.prune_played();
        self.current()
    }

    pub fn play_at(&mut self, position: usize) -> Option<&T> {
        if position >= self.items.len() {
            return None;
        }
        self.current = Some(position);
        self.exhausted = false;
        self.current()
    }

    pub fn previous(&mut self) -> Option<&T> {
        if self.items.is_empty() {
            return None;
        }
        self.exhausted = false;
        self.current = Some(match self.current {
            Some(current) if current > 0 => current - 1,
            _ if self.repeat == RepeatMode::Queue => self.items.len() - 1,
            // The oldest kept track: stepping back further replays it.
            _ => 0,
        });
        self.current()
    }

    /// Move a displayed position; with shuffle on, only the shuffled order changes.
    pub fn move_entry(&mut self, from: usize, to: usize) -> bool {
        if from >= self.items.len() || to >= self.items.len() {
            return false;
        }
        if from == to {
            return true;
        }
        if self.shuffle {
            let entry = self.shuffle_order.remove(from);
            self.shuffle_order.insert(to, entry);
        } else {
            let entry = self.items.remove(from);
            self.items.insert(to, entry);
        }
        if let Some(current) = self.current {
            if current == from {
                self.current = Some(to);
            } else if from < current && to >= current {
                self.current = Some(current - 1);
            } else if from > current && to <= current {
                self.current = Some(current + 1);
            }
        }
        true
    }

    pub fn set_repeat(&mut self, mode: RepeatMode) {
        self.repeat = mode;
    }

    /// Shuffle or unshuffle only what is still to play: nothing unplayed lands behind the
    /// current track, and nothing played comes back as upcoming.
    pub fn set_shuffle(&mut self, enabled: bool) {
        if enabled == self.shuffle {
            return;
        }
        let display: Vec<usize> =
            if self.shuffle { self.shuffle_order.clone() } else { (0..self.items.len()).collect() };
        let kept = self.current.map_or(0, |current| current + 1).min(display.len());
        let (played, rest) = display.split_at(kept);
        let mut rest = rest.to_vec();
        if enabled {
            shuffle_in_place(&mut rest);
            self.shuffle_order = played.iter().copied().chain(rest).collect();
            self.shuffle = true;
            return;
        }
        rest.sort_unstable();
        let order: Vec<usize> = played.iter().copied().chain(rest).collect();
        let mut taken: Vec<Option<T>> = std::mem::take(&mut self.items).into_iter().map(Some).collect();
        self.items = order.into_iter().map(|index| taken[index].take().expect("each index once")).collect();
        self.shuffle = false;
        self.shuffle_order.clear();
    }

    /// Drop played tracks beyond the history limit, oldest first in display order.
    fn prune_played(&mut self) {
        if self.repeat == RepeatMode::Queue {
            return;
        }
        let Some(current) = self.current else { return };
        let excess = current.saturating_sub(PLAYED_HISTORY_LIMIT);
        if excess == 0 {
            return;
        }
        if self.shuffle {
            let dropped: HashSet<usize> = self.shuffle_order[..excess].iter().copied().collect();
            let mut renumbered = vec![0; self.items.len()];
            let mut next = 0;
            for (index, entry) in renumbered.iter_mut().enumerate() {
                if !dropped.contains(&index) {
                    *entry = next;
                    next += 1;
                }
            }
            let mut index = 0;
            self.items.retain(|_| {
                let keep = !dropped.contains(&index);
                index += 1;
                keep
            });
            self.shuffle_order = self.shuffle_order[excess..].iter().map(|entry| renumbered[*entry]).collect();
        } else {
            self.items.drain(..excess);
        }
        self.current = Some(current - excess);
        self.pruned += excess;
    }

    fn regenerate_shuffle_order(&mut self) {
        self.shuffle_order = (0..self.items.len()).collect();
        shuffle_in_place(&mut self.shuffle_order);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queue(count: u32) -> PlayQueue<u32> {
        let mut queue = PlayQueue::new();
        queue.add_many((0..count).collect()).unwrap();
        queue
    }

    fn all(queue: &PlayQueue<u32>) -> Vec<u32> {
        queue.get_all().into_iter().copied().collect()
    }

    #[test]
    fn it_plays_in_order_and_stops_at_the_end() {
        let mut q = queue(2);
        assert_eq!(q.current(), None);
        assert_eq!(q.next(), Some(&0));
        assert_eq!(q.next(), Some(&1));
        assert_eq!(q.next(), None);
        assert_eq!(q.current(), None);
    }

    #[test]
    fn a_request_after_the_end_does_not_replay_history() {
        let mut q = queue(1);
        q.next();
        assert_eq!(q.next(), None);
        q.add(9).unwrap();
        assert_eq!(q.next(), Some(&9));
    }

    #[test]
    fn repeat_all_wraps_both_ways() {
        let mut q = queue(2);
        q.set_repeat(RepeatMode::Queue);
        q.next();
        q.next();
        assert_eq!(q.next(), Some(&0));
        assert_eq!(q.previous(), Some(&1));
    }

    #[test]
    fn previous_stops_at_the_oldest_track() {
        let mut q = queue(2);
        q.next();
        assert_eq!(q.previous(), Some(&0));
        assert_eq!(q.previous(), Some(&0));
    }

    #[test]
    fn removing_keeps_the_current_track() {
        let mut q = queue(4);
        q.play_at(2);
        assert!(q.remove_at(0));
        assert_eq!(q.current(), Some(&2));
        assert!(q.remove_at(1));
        assert_eq!(q.current(), Some(&3));
        assert!(!q.remove_at(9));
        assert!(q.remove_at(1));
        assert_eq!(q.current(), Some(&1));
        assert!(q.remove_at(0));
        assert_eq!(q.current(), None);
    }

    #[test]
    fn moving_follows_the_current_track() {
        let mut q = queue(4);
        q.play_at(1);
        assert!(q.move_entry(1, 3));
        assert_eq!(q.current(), Some(&1));
        assert_eq!(all(&q), [0, 2, 3, 1]);
        assert!(q.move_entry(0, 3));
        assert_eq!(q.current(), Some(&1));
        assert!(q.move_entry(3, 0));
        assert_eq!(q.current(), Some(&1));
        assert!(!q.move_entry(0, 9));
    }

    #[test]
    fn a_full_queue_refuses_and_changes_nothing() {
        let mut q = queue(MAX_QUEUE_ITEMS as u32 - 1);
        assert_eq!(q.add_many(vec![1, 2]), Err(QueueFull { requested: 2, room: 1 }));
        assert_eq!(q.len(), MAX_QUEUE_ITEMS - 1);
        q.add(1).unwrap();
        assert_eq!(q.add(2), Err(QueueFull { requested: 1, room: 0 }));
    }

    #[test]
    fn shuffle_touches_only_what_is_still_to_play() {
        let mut q = queue(40);
        q.play_at(4);
        q.set_shuffle(true);
        assert_eq!(all(&q)[..5], [0, 1, 2, 3, 4]);
        assert_eq!(q.current(), Some(&4));
        let mut sorted = all(&q);
        sorted.sort_unstable();
        assert_eq!(sorted, (0..40).collect::<Vec<_>>());
        assert_ne!(all(&q), (0..40).collect::<Vec<_>>(), "1 in 35! chance of failing by luck");

        // Tracks played while shuffled stay played after shuffle ends.
        let played: Vec<u32> = (0..5).filter_map(|_| q.next().copied()).collect();
        q.set_shuffle(false);
        assert_eq!(all(&q)[5..10], played[..]);
        assert_eq!(q.current(), Some(&played[4]));
        let upcoming: Vec<u32> = q.upcoming().into_iter().copied().collect();
        assert!(upcoming.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn appending_under_shuffle_never_changes_the_current_track() {
        let mut q = queue(5);
        q.set_shuffle(true);
        let current = *q.next().unwrap();
        for value in 10..50 {
            q.add(value).unwrap();
            assert_eq!(q.current(), Some(&current));
        }
        assert_eq!(q.get_all().len(), 45);
    }

    #[test]
    fn removing_under_shuffle_uses_the_displayed_position() {
        let mut q = queue(6);
        q.set_shuffle(true);
        let display = all(&q);
        assert!(q.remove_at(2));
        let mut expected = display;
        expected.remove(2);
        assert_eq!(all(&q), expected);
    }

    #[test]
    fn old_tracks_are_pruned_and_counted() {
        let mut q = queue(PLAYED_HISTORY_LIMIT as u32 + 10);
        for _ in 0..PLAYED_HISTORY_LIMIT + 5 {
            q.next();
        }
        assert_eq!(q.pruned(), 4);
        assert_eq!(q.index(), Some(PLAYED_HISTORY_LIMIT));
        assert_eq!(q.current(), Some(&(PLAYED_HISTORY_LIMIT as u32 + 4)));
        assert_eq!(q.len(), PLAYED_HISTORY_LIMIT + 6);
    }

    #[test]
    fn pruning_under_shuffle_keeps_the_order() {
        let mut q = queue(PLAYED_HISTORY_LIMIT as u32 + 10);
        q.set_shuffle(true);
        let display = all(&q);
        for _ in 0..PLAYED_HISTORY_LIMIT + 5 {
            q.next();
        }
        assert_eq!(all(&q), display[4..]);
        assert_eq!(q.current(), Some(&display[PLAYED_HISTORY_LIMIT + 4]));
    }

    #[test]
    fn repeat_all_never_prunes() {
        let mut q = queue(PLAYED_HISTORY_LIMIT as u32 + 10);
        q.set_repeat(RepeatMode::Queue);
        for _ in 0..PLAYED_HISTORY_LIMIT + 9 {
            q.next();
        }
        assert_eq!(q.pruned(), 0);
    }

    #[test]
    fn upcoming_and_rewind() {
        let mut q = queue(3);
        assert_eq!(q.upcoming().len(), 3);
        q.next();
        assert_eq!(q.upcoming(), [&1, &2]);
        q.rewind();
        assert_eq!(q.next(), Some(&0));
        q.clear();
        assert!(q.is_empty());
        assert_eq!(q.next(), None);
    }
}
