//! Auto-DJ and 24/7: the fallback that plays when nothing is requested. This is the state
//! machine only, with time passed in; [`crate::chat::service`] runs it. It owns fallback playback
//! and never the request queue: any request takes over at once.

use serde_json::{json, Value};

use crate::chat::format::{AutomationSource, AutomationSummary};

const HEALTHY_AFTER_MS: i64 = 30_000;
const MAX_FAILURES: u32 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutomationTarget {
    pub kind: AutomationSource,
    pub id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutomationSettings {
    pub enabled: bool,
    pub source: Option<AutomationTarget>,
    pub require_listeners: bool,
    pub quiet_period_seconds: i64,
    pub retry_base_seconds: i64,
    pub retry_max_seconds: i64,
    pub stay_in_channel: bool,
}

impl Default for AutomationSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            source: None,
            require_listeners: false,
            quiet_period_seconds: 300,
            retry_base_seconds: 30,
            retry_max_seconds: 1800,
            stay_in_channel: false,
        }
    }
}

impl AutomationSettings {
    /// As the panel reads them, and as they are stored.
    pub fn to_json(&self) -> Value {
        json!({
            "enabled": self.enabled,
            "source": self.source.map(|source| json!({"kind": if source.kind == AutomationSource::Radio { "radio" } else { "playlist" }, "id": source.id})),
            "requireListeners": self.require_listeners,
            "quietPeriodSeconds": self.quiet_period_seconds,
            "retryBaseSeconds": self.retry_base_seconds,
            "retryMaxSeconds": self.retry_max_seconds,
            "stayInChannel": self.stay_in_channel,
        })
    }

    /// Strict: every field present, no others, in range. Saved settings that fail this are off,
    /// so old rows and corrupt checkpoints cannot enable automation by accident.
    pub fn parse(value: &Value) -> Option<Self> {
        let data = value.as_object()?;
        const FIELDS: [&str; 7] = ["enabled", "source", "requireListeners", "quietPeriodSeconds", "retryBaseSeconds", "retryMaxSeconds", "stayInChannel"];
        if data.len() != FIELDS.len() || FIELDS.iter().any(|field| !data.contains_key(*field)) {
            return None;
        }
        let number = |key: &str, min: i64, max: i64| data[key].as_i64().filter(|n| (min..=max).contains(n));
        let quiet_period_seconds = number("quietPeriodSeconds", 0, 86_400)?;
        let retry_base_seconds = number("retryBaseSeconds", 1, 3_600)?;
        let retry_max_seconds = number("retryMaxSeconds", 1, 86_400)?;
        if retry_max_seconds < retry_base_seconds {
            return None;
        }
        let source = match &data["source"] {
            Value::Null => None,
            candidate => {
                let candidate = candidate.as_object()?;
                if candidate.len() != 2 {
                    return None;
                }
                let kind = match candidate.get("kind")?.as_str()? {
                    "playlist" => AutomationSource::Playlist,
                    "radio" => AutomationSource::Radio,
                    _ => return None,
                };
                Some(AutomationTarget { kind, id: candidate.get("id")?.as_i64().filter(|id| *id > 0)? })
            }
        };
        let enabled = data["enabled"].as_bool()?;
        if enabled && source.is_none() {
            return None;
        }
        Some(Self {
            enabled,
            source,
            require_listeners: data["requireListeners"].as_bool()?,
            quiet_period_seconds,
            retry_base_seconds,
            retry_max_seconds,
            stay_in_channel: data["stayInChannel"].as_bool()?,
        })
    }

    pub fn from_stored(text: Option<&str>) -> Self {
        let parsed = text.and_then(|text| serde_json::from_str(text).ok()).and_then(|value| Self::parse(&value));
        if text.is_some() && parsed.is_none() {
            tracing::warn!("Invalid saved automation settings; using disabled defaults");
        }
        parsed.unwrap_or_default()
    }

    fn source_key(&self) -> Option<String> {
        self.source.map(|source| {
            let kind = if source.kind == AutomationSource::Radio { "radio" } else { "playlist" };
            format!("{kind}:{}", source.id)
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuntimeState {
    pub quiet_until: i64,
    pub retry_until: i64,
    pub failures: u32,
    pub last_track_id: Option<String>,
    pub presence_suppressed: bool,
    pub source_key: Option<String>,
    pub channel_id: Option<i64>,
}

impl RuntimeState {
    /// The stored JSON, same fields as the old program. Bad values fall back safely.
    pub fn to_stored(&self) -> String {
        json!({
            "quietUntil": self.quiet_until,
            "retryUntil": self.retry_until,
            "failures": self.failures,
            "lastTrackId": self.last_track_id,
            "presenceSuppressed": self.presence_suppressed,
            "sourceKey": self.source_key,
            "channelId": self.channel_id,
        })
        .to_string()
    }
}

/// What the bot looks like right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    pub connected: bool,
    pub busy: bool,
    pub has_requests: bool,
    pub human_listeners: usize,
}

/// One unit of work for whoever runs the controller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Nothing,
    /// Find the next track; hand it to [`AutomationController::loaded`] with this token.
    Load { token: u64, source: AutomationTarget, last_track_id: Option<String> },
}

#[derive(Debug)]
pub struct AutomationController {
    settings: AutomationSettings,
    runtime: RuntimeState,
    generation: u64,
    owned: bool,
    started_at: i64,
    loading: bool,
    disposed: bool,
    stop_requested: bool,
    state_changed: bool,
}

fn timestamp(value: &Value) -> i64 {
    value.as_f64().filter(|n| n.is_finite() && *n >= 0.0).unwrap_or(0.0) as i64
}

impl AutomationController {
    pub fn new(settings: AutomationSettings, stored_state: Option<&str>) -> Self {
        let persisted: Value = stored_state.and_then(|text| serde_json::from_str(text).ok()).unwrap_or(Value::Null);
        let source_key = settings.source_key();
        let mut runtime = RuntimeState {
            quiet_until: timestamp(&persisted["quietUntil"]),
            retry_until: timestamp(&persisted["retryUntil"]),
            failures: (timestamp(&persisted["failures"]) as u32).min(MAX_FAILURES),
            last_track_id: persisted["lastTrackId"].as_str().map(str::to_owned),
            presence_suppressed: persisted["presenceSuppressed"] == true,
            source_key: source_key.clone(),
            channel_id: persisted["channelId"].as_i64().filter(|id| *id > 0),
        };
        // A saved state from another source says nothing about this one.
        if persisted.get("sourceKey").is_some() && persisted["sourceKey"].as_str().map(str::to_owned) != source_key {
            runtime.retry_until = 0;
            runtime.failures = 0;
            runtime.last_track_id = None;
        }
        Self { settings, runtime, generation: 0, owned: false, started_at: 0, loading: false, disposed: false, stop_requested: false, state_changed: false }
    }

    pub fn active(&self) -> bool {
        self.owned
    }

    pub fn configuration(&self) -> &AutomationSettings {
        &self.settings
    }

    pub fn state(&self) -> &RuntimeState {
        &self.runtime
    }

    pub fn keep_channel(&self) -> bool {
        self.settings.stay_in_channel && !self.runtime.presence_suppressed
    }

    /// `None` when neither Auto-DJ nor 24/7 is on, and while automation is suspended.
    pub fn summary(&self) -> Option<AutomationSummary> {
        if self.runtime.presence_suppressed {
            return None;
        }
        let auto_dj = self.settings.source.filter(|_| self.settings.enabled).map(|source| source.kind);
        (auto_dj.is_some() || self.settings.stay_in_channel).then_some(AutomationSummary { auto_dj, stay_in_channel: self.settings.stay_in_channel })
    }

    /// Whether the audio it owns must stop now. Each request is reported once.
    pub fn take_stop(&mut self) -> bool {
        std::mem::take(&mut self.stop_requested)
    }

    /// The state to save, when it changed since the last call.
    pub fn take_state_to_save(&mut self) -> Option<String> {
        std::mem::take(&mut self.state_changed).then(|| self.runtime.to_stored())
    }

    pub fn configure(&mut self, settings: AutomationSettings) {
        let source_changed = settings.source_key() != self.settings.source_key();
        let changed = settings != self.settings;
        let enabled = settings.enabled;
        self.settings = settings;
        if changed && (source_changed || !enabled) {
            self.invalidate(true);
        } else if changed && self.loading && !self.owned {
            self.invalidate(false);
        }
        if source_changed {
            self.runtime.source_key = self.settings.source_key();
            self.runtime.last_track_id = None;
            self.reset_failures();
            self.state_changed = true;
        }
    }

    pub fn request_queued(&mut self) {
        self.invalidate(true);
    }

    pub fn manual_stop(&mut self, now: i64) {
        self.runtime.quiet_until = now + self.settings.quiet_period_seconds * 1000;
        self.state_changed = true;
        self.invalidate(true);
    }

    pub fn disconnected(&mut self) {
        self.invalidate(false);
    }

    pub fn suspend_presence(&mut self) {
        self.runtime.presence_suppressed = true;
        self.state_changed = true;
        self.invalidate(true);
    }

    pub fn resume_presence(&mut self) {
        self.runtime.presence_suppressed = false;
        self.state_changed = true;
    }

    pub fn remember_channel(&mut self, channel: i64) {
        if self.keep_channel() && channel > 0 && Some(channel) != self.runtime.channel_id {
            self.runtime.channel_id = Some(channel);
            self.state_changed = true;
        }
    }

    pub fn source_ended(&mut self, failed: bool, now: i64) {
        if !self.owned {
            return;
        }
        let radio = self.settings.source.is_some_and(|source| source.kind == AutomationSource::Radio);
        let unhealthy = failed || (radio && now - self.started_at < HEALTHY_AFTER_MS);
        self.invalidate(false);
        if unhealthy {
            self.record_failure(now);
        } else {
            self.reset_failures();
            self.state_changed = true;
        }
    }

    pub fn dispose(&mut self) {
        self.disposed = true;
        self.invalidate(false);
    }

    fn eligible(&self, snapshot: Snapshot, now: i64) -> bool {
        !self.disposed
            && self.settings.enabled
            && self.settings.source.is_some()
            && !self.runtime.presence_suppressed
            && snapshot.connected
            && !snapshot.has_requests
            && (!self.settings.require_listeners || snapshot.human_listeners > 0)
            && now >= self.runtime.quiet_until.max(self.runtime.retry_until)
    }

    /// Look at the bot and say what to do next. Called on every change and once a second.
    pub fn reconcile(&mut self, snapshot: Snapshot, now: i64) -> Step {
        if !self.eligible(snapshot, now) {
            // Quiet time alone must never stop a human request.
            if self.owned || self.loading {
                self.invalidate(self.owned && snapshot.connected);
            }
            return Step::Nothing;
        }
        if self.owned {
            if now - self.started_at >= HEALTHY_AFTER_MS && self.runtime.failures > 0 {
                self.reset_failures();
                self.state_changed = true;
            }
            return Step::Nothing;
        }
        if snapshot.busy || self.loading {
            return Step::Nothing;
        }
        let Some(source) = self.settings.source else { return Step::Nothing };
        self.loading = true;
        Step::Load { token: self.generation, source, last_track_id: self.runtime.last_track_id.clone() }
    }

    /// The load for `token` found a track. True means play it now as Auto-DJ's.
    pub fn loaded(&mut self, token: u64, track_id: &str, snapshot: Snapshot, now: i64) -> bool {
        if token != self.generation {
            return false;
        }
        self.loading = false;
        if !self.eligible(snapshot, now) || snapshot.busy {
            return false;
        }
        self.owned = true;
        self.started_at = now;
        self.runtime.last_track_id = Some(track_id.to_owned());
        self.state_changed = true;
        true
    }

    /// The load for `token` failed.
    pub fn load_failed(&mut self, token: u64, now: i64) {
        if token != self.generation {
            return;
        }
        self.invalidate(true);
        self.record_failure(now);
    }

    fn invalidate(&mut self, stop_owned: bool) {
        self.generation += 1;
        let was_owned = self.owned;
        self.owned = false;
        self.loading = false;
        if stop_owned && was_owned {
            self.stop_requested = true;
        }
    }

    fn reset_failures(&mut self) {
        self.runtime.failures = 0;
        self.runtime.retry_until = 0;
    }

    fn record_failure(&mut self, now: i64) {
        self.runtime.failures = (self.runtime.failures + 1).min(MAX_FAILURES);
        let backoff = self.settings.retry_base_seconds.saturating_mul(1i64 << (self.runtime.failures - 1).min(40));
        self.runtime.retry_until = now + backoff.min(self.settings.retry_max_seconds) * 1000;
        self.state_changed = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RADIO: AutomationTarget = AutomationTarget { kind: AutomationSource::Radio, id: 7 };
    const PLAYLIST: AutomationTarget = AutomationTarget { kind: AutomationSource::Playlist, id: 3 };
    const IDLE: Snapshot = Snapshot { connected: true, busy: false, has_requests: false, human_listeners: 0 };

    fn settings(source: AutomationTarget) -> AutomationSettings {
        AutomationSettings { enabled: true, source: Some(source), ..AutomationSettings::default() }
    }

    fn start(controller: &mut AutomationController, now: i64) -> u64 {
        let Step::Load { token, .. } = controller.reconcile(IDLE, now) else { panic!("should load") };
        assert!(controller.loaded(token, "t", IDLE, now));
        token
    }

    #[test]
    fn it_plays_when_idle_and_remembers_the_track() {
        let mut c = AutomationController::new(settings(PLAYLIST), None);
        let Step::Load { source, last_track_id, token } = c.reconcile(IDLE, 0) else { panic!() };
        assert_eq!((source, last_track_id), (PLAYLIST, None));
        assert_eq!(c.reconcile(IDLE, 0), Step::Nothing, "already loading");
        assert!(c.loaded(token, "12", IDLE, 0));
        assert!(c.active());
        assert_eq!(c.state().last_track_id.as_deref(), Some("12"));
    }

    #[test]
    fn it_stays_off_without_settings() {
        let mut c = AutomationController::new(AutomationSettings::default(), None);
        assert_eq!(c.reconcile(IDLE, 0), Step::Nothing);
        assert_eq!(c.summary(), None);
    }

    #[test]
    fn a_request_takes_over_at_once() {
        let mut c = AutomationController::new(settings(PLAYLIST), None);
        start(&mut c, 0);
        c.request_queued();
        assert!(c.take_stop());
        assert!(!c.take_stop(), "reported once");
        assert!(!c.active());
    }

    #[test]
    fn queued_requests_block_it_and_stop_what_it_plays() {
        let mut c = AutomationController::new(settings(PLAYLIST), None);
        start(&mut c, 0);
        let waiting = Snapshot { has_requests: true, busy: true, ..IDLE };
        assert_eq!(c.reconcile(waiting, 1), Step::Nothing);
        assert!(c.take_stop());
    }

    #[test]
    fn a_load_that_lost_the_race_does_not_play() {
        let mut c = AutomationController::new(settings(PLAYLIST), None);
        let Step::Load { token, .. } = c.reconcile(IDLE, 0) else { panic!() };
        c.request_queued();
        assert!(!c.loaded(token, "t", IDLE, 0));
        assert!(!c.active());
        assert!(!c.take_stop(), "nothing of its own was playing");
    }

    #[test]
    fn a_manual_stop_keeps_it_quiet_for_the_period() {
        let mut c = AutomationController::new(settings(PLAYLIST), None);
        start(&mut c, 0);
        c.manual_stop(1000);
        assert!(c.take_stop());
        assert_eq!(c.reconcile(IDLE, 1000 + 299_000), Step::Nothing);
        assert!(matches!(c.reconcile(IDLE, 1000 + 300_000), Step::Load { .. }));
    }

    #[test]
    fn failures_back_off_up_to_the_limit() {
        let mut c = AutomationController::new(settings(PLAYLIST), None);
        let mut now = 0;
        for expected in [30, 60, 120, 240, 480, 960, 1800, 1800] {
            let Step::Load { token, .. } = c.reconcile(IDLE, now) else { panic!("at {now}") };
            c.load_failed(token, now);
            assert_eq!(c.state().retry_until, now + expected * 1000);
            assert_eq!(c.reconcile(IDLE, now + expected * 1000 - 1), Step::Nothing);
            now += expected * 1000;
        }
    }

    #[test]
    fn a_radio_that_ends_quickly_counts_as_failed() {
        let mut c = AutomationController::new(settings(RADIO), None);
        start(&mut c, 0);
        c.source_ended(false, 10_000);
        assert_eq!(c.state().failures, 1);

        let mut c = AutomationController::new(settings(RADIO), None);
        start(&mut c, 0);
        c.source_ended(false, 31_000);
        assert_eq!(c.state().failures, 0);
    }

    #[test]
    fn a_playlist_track_ending_is_healthy_and_a_failed_one_is_not() {
        let mut c = AutomationController::new(settings(PLAYLIST), None);
        start(&mut c, 0);
        c.source_ended(false, 1000);
        assert_eq!(c.state().failures, 0);
        assert!(matches!(c.reconcile(IDLE, 1000), Step::Load { .. }));

        let mut c = AutomationController::new(settings(PLAYLIST), None);
        start(&mut c, 0);
        c.source_ended(true, 1000);
        assert_eq!(c.state().failures, 1);
        assert_eq!(c.reconcile(IDLE, 1000), Step::Nothing);
    }

    #[test]
    fn playing_healthily_clears_earlier_failures() {
        let stored = r#"{"failures":3,"retryUntil":0,"sourceKey":"radio:7"}"#;
        let mut c = AutomationController::new(settings(RADIO), Some(stored));
        assert_eq!(c.state().failures, 3);
        start(&mut c, 0);
        assert_eq!(c.reconcile(IDLE, 29_000), Step::Nothing);
        assert_eq!(c.state().failures, 3);
        assert_eq!(c.reconcile(IDLE, 30_000), Step::Nothing);
        assert_eq!(c.state().failures, 0);
    }

    #[test]
    fn listeners_can_be_required() {
        let mut settings = settings(PLAYLIST);
        settings.require_listeners = true;
        let mut c = AutomationController::new(settings, None);
        assert_eq!(c.reconcile(IDLE, 0), Step::Nothing);
        let token = match c.reconcile(Snapshot { human_listeners: 1, ..IDLE }, 0) {
            Step::Load { token, .. } => token,
            Step::Nothing => panic!(),
        };
        assert!(c.loaded(token, "t", Snapshot { human_listeners: 1, ..IDLE }, 0));
        // The last listener left.
        assert_eq!(c.reconcile(IDLE, 1), Step::Nothing);
        assert!(c.take_stop());
    }

    #[test]
    fn suspending_stops_it_and_hides_it_until_resumed() {
        let mut settings = settings(PLAYLIST);
        settings.stay_in_channel = true;
        let mut c = AutomationController::new(settings, None);
        assert!(c.keep_channel());
        start(&mut c, 0);
        c.suspend_presence();
        assert!(c.take_stop());
        assert!(!c.keep_channel());
        assert_eq!(c.summary(), None);
        assert_eq!(c.reconcile(IDLE, 1), Step::Nothing);
        c.resume_presence();
        assert!(matches!(c.reconcile(IDLE, 1), Step::Load { .. }));
    }

    #[test]
    fn it_remembers_the_channel_only_for_24_7() {
        let mut c = AutomationController::new(settings(PLAYLIST), None);
        c.remember_channel(5);
        assert_eq!(c.state().channel_id, None);
        let mut stay = settings(PLAYLIST);
        stay.stay_in_channel = true;
        let mut c = AutomationController::new(stay, None);
        c.remember_channel(5);
        assert_eq!(c.state().channel_id, Some(5));
        assert!(c.take_state_to_save().is_some());
        assert!(c.take_state_to_save().is_none(), "saved once");
    }

    #[test]
    fn changing_the_source_starts_clean() {
        let stored = r#"{"failures":3,"lastTrackId":"x","sourceKey":"radio:7"}"#;
        let mut c = AutomationController::new(settings(RADIO), Some(stored));
        start(&mut c, 0);
        c.configure(settings(PLAYLIST));
        assert!(c.take_stop());
        assert_eq!(c.state().failures, 0);
        assert_eq!(c.state().last_track_id, None);
        assert_eq!(c.state().source_key.as_deref(), Some("playlist:3"));

        let other = AutomationController::new(settings(PLAYLIST), Some(stored));
        assert_eq!((other.state().failures, other.state().last_track_id.clone()), (0, None));
    }

    #[test]
    fn stored_state_is_read_safely() {
        let c = AutomationController::new(settings(RADIO), Some(r#"{"quietUntil":-5,"failures":999,"channelId":-1,"lastTrackId":5}"#));
        let state = c.state();
        assert_eq!((state.quiet_until, state.failures, state.channel_id, state.last_track_id.clone()), (0, 30, None, None));
        assert_eq!(AutomationController::new(settings(RADIO), Some("nonsense")).state().failures, 0);
    }

    #[test]
    fn stored_state_keeps_the_old_field_names() {
        let state: Value = serde_json::from_str(&RuntimeState { quiet_until: 1, source_key: Some("radio:7".to_owned()), ..RuntimeState::default() }.to_stored()).unwrap();
        assert_eq!(state["quietUntil"], 1);
        assert_eq!(state["sourceKey"], "radio:7");
        assert_eq!(state["presenceSuppressed"], false);
        assert_eq!(state["lastTrackId"], Value::Null);
    }

    #[test]
    fn settings_are_strict() {
        let good = json!({"enabled": true, "source": {"kind": "radio", "id": 1}, "requireListeners": false, "quietPeriodSeconds": 0, "retryBaseSeconds": 1, "retryMaxSeconds": 1, "stayInChannel": true});
        assert_eq!(AutomationSettings::parse(&good).unwrap().source, Some(AutomationTarget { kind: AutomationSource::Radio, id: 1 }));
        for (field, value) in [
            ("enabled", json!("yes")),
            ("source", json!(null)),
            ("source", json!({"kind": "tv", "id": 1})),
            ("source", json!({"kind": "radio", "id": 0})),
            ("quietPeriodSeconds", json!(-1)),
            ("retryBaseSeconds", json!(0)),
            ("retryMaxSeconds", json!(0)),
            ("extra", json!(1)),
        ] {
            let mut bad = good.clone();
            bad[field] = value;
            assert!(AutomationSettings::parse(&bad).is_none(), "{field}");
        }
        assert_eq!(AutomationSettings::from_stored(Some("nonsense")), AutomationSettings::default());
        assert_eq!(AutomationSettings::from_stored(None), AutomationSettings::default());
    }

    #[test]
    fn the_summary_says_what_is_on() {
        assert_eq!(
            AutomationController::new(settings(RADIO), None).summary(),
            Some(AutomationSummary { auto_dj: Some(AutomationSource::Radio), stay_in_channel: false })
        );
    }
}
