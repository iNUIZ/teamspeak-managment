//! VoiceBot, chat commands, Auto-DJ and queue.

pub mod bot;
pub mod clock;
pub mod manager;
pub mod nickname;
pub mod queue;

pub use bot::{BotConfig, BotEvent, Media, QueueItem, Snapshot, Source, Status, VoiceBot};
pub mod activity;
pub mod automation;
pub mod chat;
