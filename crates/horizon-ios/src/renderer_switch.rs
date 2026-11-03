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

/// Preserve the actual assertion/validation message when WGPU or UIKit panics
/// in the offscreen preparation worker, rather than showing an opaque failure.
/// Bound the text so it remains safe to hand across the Swift diagnostics ABI.
#[must_use]
pub fn describe_renderer_panic(payload: &(dyn std::any::Any + Send)) -> String {
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("unknown non-string panic payload");
    // Swift's graphics-error bridge accepts fewer than 4096 UTF-8 bytes.
    // 950 Unicode scalars occupy at most 3800 bytes, including even a full
    // four-byte scalar per character plus this error prefix.
    let message: String = message.chars().take(950).collect();
    format!("Horizon GPU worker panicked: {message}")
}

/// Native GPU progress is meaningful during boot pipeline creation or
/// static terrain replacement, never after those states have completed.
#[must_use]
pub fn renderer_stage_is_active(boot_state: u32, terrain_state: u32) -> bool {
    boot_state == 2 || terrain_state == 3
}

#[cfg(test)]
mod preview_stage_tests {
    use super::renderer_stage_is_active;

    #[test]
    fn terrain_worker_progress_is_logged_after_title_renderer_is_ready() {
        assert!(renderer_stage_is_active(2, 0));
        assert!(renderer_stage_is_active(3, 3));
        assert!(!renderer_stage_is_active(3, 4));
        assert!(!renderer_stage_is_active(3, 0));
        assert!(!renderer_stage_is_active(4, 5));
    }
}
