//! Platform-independent state for an opt-in read-only terrain GPU preview.
//! The UIKit host owns its one active drawable; the worker only prepares an
//! offscreen renderer with the original title renderer still visible.

use std::sync::atomic::{AtomicU32, Ordering};

/// 0=not ready, 1=CPU geometry ready, 2=requested, 3=GPU preparing,
/// 4=presented, 5=failed (title restored), 6=exit requested.
/// None of these states represents a running Minecraft game.
#[derive(Default, Debug)]
pub struct TerrainPreviewSwitch(AtomicU32);

impl TerrainPreviewSwitch {
    pub const fn new() -> Self {
        Self(AtomicU32::new(0))
    }

    #[must_use]
    pub fn state(&self) -> u32 {
        self.0.load(Ordering::Acquire)
    }

    pub fn ready(&self) {
        self.0.store(1, Ordering::Release);
    }

    pub fn request(&self) -> bool {
        let previous = self.state();
        if !matches!(previous, 1 | 5) {
            return false;
        }
        self.0
            .compare_exchange(previous, 2, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub fn start(&self) -> bool {
        self.0
            .compare_exchange(2, 3, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    /// The worker may complete after a suspension. Only the current loading
    /// request is allowed to publish a visible state.
    pub fn presented(&self) -> bool {
        self.0
            .compare_exchange(3, 4, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub fn failed(&self) {
        self.0.store(5, Ordering::Release);
    }

    pub fn exit_requested(&self) -> bool {
        self.0
            .compare_exchange(4, 6, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    /// The Winit proxy can disappear during UIKit teardown. Restore the
    /// visible status when the exit event could not be delivered.
    pub fn cancel_exit(&self) -> bool {
        self.0
            .compare_exchange(6, 4, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub fn exit_finished(&self, success: bool) {
        self.0.store(if success { 1 } else { 5 }, Ordering::Release);
    }

    /// Invalidate any previously completed worker when the drawable or the
    /// saved world changes. It will be discarded on the event thread.
    pub fn reset(&self) {
        self.0.store(0, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::TerrainPreviewSwitch;

    #[test]
    fn preview_requires_ready_resources_and_single_request() {
        let state = TerrainPreviewSwitch::new();
        assert_eq!(state.state(), 0);
        assert!(!state.request());
        state.ready();
        assert_eq!(state.state(), 1);
        assert!(state.request());
        assert!(!state.request());
        assert!(state.start());
        assert!(!state.start());
        state.presented();
        assert_eq!(state.state(), 4);
        assert!(!state.request());
        assert!(state.exit_requested());
        assert_eq!(state.state(), 6);
        state.exit_finished(true);
        assert_eq!(state.state(), 1);
        assert!(state.request());
    }

    #[test]
    fn stale_completed_worker_cannot_resurrect_a_suspended_preview() {
        let state = TerrainPreviewSwitch::new();
        state.ready();
        assert!(!state.presented());
        assert!(state.request());
        assert!(state.start());
        state.reset();
        assert!(!state.presented());
        assert_eq!(state.state(), 0);
        state.ready();
        assert!(state.request());
        assert!(state.start());
        assert!(state.presented());
        assert!(state.exit_requested());
        assert!(state.cancel_exit());
        assert_eq!(state.state(), 4);
    }

    #[test]
    fn failed_preview_can_retry_and_suspend_invalidates_worker() {
        let state = TerrainPreviewSwitch::new();
        state.ready();
        assert!(state.request());
        assert!(state.start());
        state.failed();
        assert_eq!(state.state(), 5);
        assert!(state.request());
        state.reset();
        assert_eq!(state.state(), 0);
        assert!(!state.start());
        assert!(!state.request());
    }
}
