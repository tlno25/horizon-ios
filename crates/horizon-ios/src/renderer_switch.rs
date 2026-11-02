//! Platform-independent renderer ownership handoff policy.
//!
//! The only Winit event loop and Window are created by the iOS shell. This
//! state machine authorizes transfer of its single Metal surface from the
//! lightweight probe to the real Horizon boot renderer.
use std::sync::atomic::{AtomicU32, Ordering};

/// 0=probe, 1=requested, 2=loading, 3=presented actual renderer,
/// 4=failed (recovered probe). None of these mean Minecraft gameplay runs.
#[derive(Default, Debug)]
pub struct RendererSwitch(AtomicU32);

impl RendererSwitch {
    pub const fn new() -> Self {
        Self(AtomicU32::new(0))
    }

    #[must_use]
    pub fn state(&self) -> u32 {
        self.0.load(Ordering::Acquire)
    }

    /// Reject duplicate requests, and do not steal the window before the
    /// shared Minecraft block registry and Metal probe have initialized.
    pub fn request(&self, core_ready: bool, graphics_ready: bool) -> bool {
        if !core_ready || !graphics_ready {
            return false;
        }
        let expected = self.state();
        if !matches!(expected, 0 | 4) {
            return false;
        }
        self.0
            .compare_exchange(expected, 1, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    /// Called directly from the first successful Metal probe presentation.
    /// iOS may coalesce an additional `request_redraw` issued while handling
    /// `RedrawRequested`, so the handoff request is made in the same event.
    pub fn request_smoke_after_probe_frame(&self, enabled: bool, presented: bool) -> bool {
        enabled && presented && self.request(true, true)
    }

    pub fn start(&self) -> bool {
        self.0
            .compare_exchange(1, 2, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub fn finished(&self, success: bool) {
        self.0.store(if success { 3 } else { 4 }, Ordering::Release);
    }

    /// UIKit may destroy a drawable in the background. Rebuild the probe on
    /// the next resume rather than holding the real renderer's old surface.
    pub fn reset(&self) {
        self.0.store(0, Ordering::Release);
    }
}
