//! The 20 ms voice clock.

use std::time::{Duration, Instant};

const FRAME: Duration = Duration::from_millis(teamhub_media::FRAME_MS);
/// Time for the decoder to fill its buffer before the first frame.
const START_DELAY: Duration = Duration::from_millis(200);
/// Being late by up to this much is made up with quicker frames; beyond it the clock restarts,
/// because a long burst would overflow the listeners' jitter buffers.
const MAX_CATCH_UP: Duration = Duration::from_millis(5 * teamhub_media::FRAME_MS);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameClock {
    next_due: Instant,
}

impl FrameClock {
    pub fn starting(now: Instant) -> Self {
        Self { next_due: now + START_DELAY }
    }

    pub fn resuming(now: Instant) -> Self {
        Self { next_due: now + FRAME }
    }

    pub fn next_due(&self) -> Instant {
        self.next_due
    }

    /// One frame slot was used at `now`.
    pub fn advance(&mut self, now: Instant) {
        self.next_due += FRAME;
        if now.saturating_duration_since(self.next_due) > MAX_CATCH_UP {
            self.next_due = now + FRAME;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_starts_after_the_buffer_delay() {
        let now = Instant::now();
        assert_eq!(FrameClock::starting(now).next_due(), now + START_DELAY);
        assert_eq!(FrameClock::resuming(now).next_due(), now + FRAME);
    }

    #[test]
    fn slots_stay_on_the_grid_when_a_timer_wakes_late() {
        // Coarse timers wake up late every time; the average must stay 20 ms.
        let start = Instant::now();
        let mut clock = FrameClock::starting(start);
        let first = clock.next_due();
        for _ in 0..1000 {
            let woke = clock.next_due() + Duration::from_millis(9);
            clock.advance(woke);
        }
        assert_eq!(clock.next_due(), first + FRAME * 1000);
    }

    #[test]
    fn a_short_stall_is_made_up() {
        let now = Instant::now();
        let mut clock = FrameClock::resuming(now);
        let due = clock.next_due();
        clock.advance(due + Duration::from_millis(60));
        assert_eq!(clock.next_due(), due + FRAME, "still behind, so the next frame goes at once");
    }

    #[test]
    fn a_long_stall_does_not_burst() {
        let now = Instant::now();
        let mut clock = FrameClock::resuming(now);
        let late = clock.next_due() + Duration::from_secs(2);
        clock.advance(late);
        assert_eq!(clock.next_due(), late + FRAME);
    }
}
