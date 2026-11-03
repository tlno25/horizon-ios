//! Throttle the real title screen on a mobile GPU without a busy render loop.
use std::time::{Duration, Instant};

const FRAME_INTERVAL: Duration = Duration::from_millis(33);

#[derive(Debug, Default)]
pub struct TitleRefresh {
    next: Option<Instant>,
}

impl TitleRefresh {
    /// Render immediately on entry, then at most about 30 times per second.
    /// Suspending the drawable clears the deadline so resume is immediate.
    pub fn due(&mut self, now: Instant, drawable: bool) -> bool {
        if !drawable {
            self.next = None;
            return false;
        }
        if self.next.is_some_and(|deadline| now < deadline) {
            return false;
        }
        self.next = Some(now + FRAME_INTERVAL);
        true
    }

    #[must_use]
    pub fn next_deadline(&self) -> Option<Instant> {
        self.next
    }
}
