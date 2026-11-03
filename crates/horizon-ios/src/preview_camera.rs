//! Touch-driven, read-only first-person look for the static saved terrain view.
//! No movement, collision, gameplay or world mutations.

use crate::TouchPhase;
use std::collections::BTreeMap;

/// One-finger view rotation only. The player position and save are immutable.
/// Two-finger gestures are suppressed until every contact has lifted, which
/// prevents a camera jump when a second finger releases first.
#[derive(Debug)]
pub struct PreviewCamera {
    position: [f64; 3],
    rotation: [f32; 2],
    touches: BTreeMap<u64, (f32, f32)>,
    suppressed: bool,
}

impl PreviewCamera {
    pub fn new(position: [f64; 3], rotation: [f32; 2]) -> Self {
        let rotation = if rotation.iter().all(|v| v.is_finite()) {
            [rotation[0], rotation[1].clamp(-90.0, 90.0)]
        } else {
            [0.0, 0.0]
        };
        Self {
            position,
            rotation,
            touches: BTreeMap::new(),
            suppressed: false,
        }
    }

    #[must_use]
    pub fn position(&self) -> [f64; 3] {
        self.position
    }

    #[must_use]
    pub fn rotation(&self) -> [f32; 2] {
        self.rotation
    }

    /// Drops transient gesture state when UIKit cancels or resizes contacts.
    pub fn clear(&mut self) {
        self.touches.clear();
        self.suppressed = false;
    }

    /// Returns true only if the camera orientation actually changed.
    pub fn touch(
        &mut self,
        id: u64,
        phase: TouchPhase,
        point: (f32, f32),
        size: (u32, u32),
    ) -> bool {
        if matches!(phase, TouchPhase::Ended | TouchPhase::Cancelled) {
            self.touches.remove(&id);
            if self.touches.is_empty() {
                self.suppressed = false;
            }
            return false;
        }
        if size.0 == 0 || size.1 == 0 || !point.0.is_finite() || !point.1.is_finite() {
            return false;
        }
        let point = (
            point.0.clamp(0.0, size.0 as f32),
            point.1.clamp(0.0, size.1 as f32),
        );
        match phase {
            TouchPhase::Started => {
                if self.touches.contains_key(&id) || self.touches.len() >= 10 {
                    return false;
                }
                self.touches.insert(id, point);
                self.suppressed |= self.touches.len() > 1;
                false
            }
            TouchPhase::Moved => {
                let Some(previous) = self.touches.get_mut(&id) else {
                    return false;
                };
                let old = *previous;
                *previous = point;
                if self.suppressed || self.touches.len() != 1 || old == point {
                    return false;
                }
                // Bound individual events so an interrupted touch cannot
                // rotate the camera through a full revolution in one frame.
                let dx = (point.0 - old.0).clamp(-(size.0 as f32) * 0.25, (size.0 as f32) * 0.25);
                let dy = (point.1 - old.1).clamp(-(size.1 as f32) * 0.5, (size.1 as f32) * 0.5);
                self.rotation[0] =
                    (self.rotation[0] + dx * 180.0 / size.0 as f32).rem_euclid(360.0);
                self.rotation[1] =
                    (self.rotation[1] + dy * 90.0 / size.1 as f32).clamp(-90.0, 90.0);
                true
            }
            TouchPhase::Ended | TouchPhase::Cancelled => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PreviewCamera;
    use crate::TouchPhase;

    #[test]
    fn drag_changes_look_but_never_saved_position() {
        let mut view = PreviewCamera::new([128.5, 72.0, -64.25], [45.0, -15.0]);
        assert!(!view.touch(11, TouchPhase::Started, (100.0, 100.0), (1000, 500)));
        assert!(view.touch(11, TouchPhase::Moved, (200.0, 150.0), (1000, 500)));
        assert_eq!(view.position(), [128.5, 72.0, -64.25]);
        assert_eq!(view.rotation(), [63.0, -6.0]);
        assert!(!view.touch(11, TouchPhase::Ended, (200.0, 150.0), (1000, 500)));
        assert!(!view.touch(11, TouchPhase::Moved, (300.0, 150.0), (1000, 500)));
        assert_eq!(view.rotation(), [63.0, -6.0]);
    }

    #[test]
    fn multitouch_and_cancels_cannot_jump_the_camera() {
        let mut view = PreviewCamera::new([0.0, 64.0, 0.0], [0.0, 0.0]);
        view.touch(1, TouchPhase::Started, (100.0, 100.0), (1000, 500));
        view.touch(2, TouchPhase::Started, (150.0, 100.0), (1000, 500));
        assert!(!view.touch(1, TouchPhase::Moved, (400.0, 200.0), (1000, 500)));
        view.touch(2, TouchPhase::Ended, (150.0, 100.0), (1000, 500));
        assert!(!view.touch(1, TouchPhase::Moved, (500.0, 200.0), (1000, 500)));
        view.touch(1, TouchPhase::Cancelled, (500.0, 200.0), (0, 0));
        assert_eq!(view.rotation(), [0.0, 0.0]);
        view.touch(3, TouchPhase::Started, (10.0, 10.0), (1000, 500));
        assert!(view.touch(3, TouchPhase::Moved, (110.0, 10.0), (1000, 500)));
        assert_eq!(view.rotation(), [18.0, 0.0]);
    }

    #[test]
    fn invalid_events_and_large_swipes_are_bounded() {
        let mut view = PreviewCamera::new([1.0, 2.0, 3.0], [f32::NAN, f32::INFINITY]);
        assert_eq!(view.rotation(), [0.0, 0.0]);
        assert!(!view.touch(7, TouchPhase::Started, (f32::NAN, 10.0), (100, 100)));
        assert!(!view.touch(7, TouchPhase::Started, (10.0, 10.0), (0, 100)));
        assert!(!view.touch(7, TouchPhase::Moved, (20.0, 20.0), (100, 100)));
        view.touch(7, TouchPhase::Started, (0.0, 0.0), (100, 100));
        assert!(view.touch(7, TouchPhase::Moved, (100.0, 100.0), (100, 100)));
        assert_eq!(view.rotation(), [45.0, 45.0]);
        assert!(!view.touch(7, TouchPhase::Moved, (f32::INFINITY, 100.0), (100, 100)));
        assert_eq!(view.rotation(), [45.0, 45.0]);
        view.clear();
        assert!(!view.touch(7, TouchPhase::Moved, (50.0, 50.0), (100, 100)));
    }
}
