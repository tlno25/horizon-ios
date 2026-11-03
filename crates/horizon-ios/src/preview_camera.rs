//! Touch-driven, read-only first-person look for the static saved terrain view.
//! No movement, collision, gameplay or world mutations.

use crate::TouchPhase;
use std::collections::{BTreeMap, BTreeSet};

/// A read-only camera with one-finger look, two-finger pan and pinch-height.
/// Translation is confined to actually rendered columns in the initial 3x3
/// saved region. The saved player pose remains immutable. Three or more
/// fingers suppress all input until all contacts are released.
#[derive(Debug)]
pub struct PreviewCamera {
    saved_position: [f64; 3],
    saved_rotation: [f32; 2],
    position: [f64; 3],
    rotation: [f32; 2],
    touches: BTreeMap<u64, (f32, f32)>,
    suppressed: bool,
    pan_anchor: Option<(f32, f32)>,
    pinch_anchor: Option<f32>,
    /// Only the saved centre and nearby columns with actual CPU meshes.
    allowed_chunks: BTreeSet<(i32, i32)>,
}

impl PreviewCamera {
    pub fn new(position: [f64; 3], rotation: [f32; 2]) -> Self {
        let rotation = if rotation.iter().all(|v| v.is_finite()) {
            [rotation[0], rotation[1].clamp(-90.0, 90.0)]
        } else {
            [0.0, 0.0]
        };
        let centre = (
            (position[0].floor() as i32) >> 4,
            (position[2].floor() as i32) >> 4,
        );
        Self {
            allowed_chunks: BTreeSet::from([centre]),
            saved_position: position,
            saved_rotation: rotation,
            position,
            rotation,
            touches: BTreeMap::new(),
            suppressed: false,
            pan_anchor: None,
            pinch_anchor: None,
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

    #[must_use]
    pub fn saved_position(&self) -> [f64; 3] {
        self.saved_position
    }

    /// Allow travel to a saved neighbour only if that column produced an
    /// actual mesh. The 3x3 region is fixed until another world is selected.
    pub fn allow_rendered_chunk(&mut self, chunk: [i32; 2]) -> bool {
        let centre = (
            (self.saved_position[0].floor() as i32) >> 4,
            (self.saved_position[2].floor() as i32) >> 4,
        );
        if (chunk[0] - centre.0).abs() > 1 || (chunk[1] - centre.1).abs() > 1 {
            return false;
        }
        self.allowed_chunks.insert((chunk[0], chunk[1]))
    }

    /// Walk one axis only when the destination has rendered blocks. Otherwise
    /// stop just inside the edge of the current column, never in unloaded void.
    fn constrain_axis(&self, proposed: f64, along_x: bool) -> f64 {
        let current = if along_x {
            self.position[0]
        } else {
            self.position[2]
        };
        let (x, z) = if along_x {
            (proposed, self.position[2])
        } else {
            (self.position[0], proposed)
        };
        let destination = ((x.floor() as i32) >> 4, (z.floor() as i32) >> 4);
        if self.allowed_chunks.contains(&destination) {
            proposed
        } else {
            let origin = ((current.floor() as i32) >> 4) as f64 * 16.0;
            proposed.clamp(origin + 0.1, origin + 15.9)
        }
    }

    /// Restore only the preview camera; never write the player's save.
    pub fn reset_view(&mut self) {
        self.position = self.saved_position;
        self.rotation = self.saved_rotation;
        self.clear();
    }

    /// Centre point of a two-finger gesture in physical pixels.
    fn touch_midpoint(&self) -> Option<(f32, f32)> {
        if self.touches.len() != 2 {
            return None;
        }
        let mut points = self.touches.values();
        let a = points.next()?;
        let b = points.next()?;
        Some(((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5))
    }

    /// Distance between two fingers, in physical pixels.
    fn touch_separation(&self) -> Option<f32> {
        if self.touches.len() != 2 {
            return None;
        }
        let mut points = self.touches.values();
        let a = points.next()?;
        let b = points.next()?;
        Some((a.0 - b.0).hypot(a.1 - b.1))
    }

    /// Drops transient gesture state when UIKit cancels or resizes contacts.
    pub fn clear(&mut self) {
        self.touches.clear();
        self.suppressed = false;
        self.pan_anchor = None;
        self.pinch_anchor = None;
    }

    /// Returns true only when the view position or orientation changed.
    pub fn touch(
        &mut self,
        id: u64,
        phase: TouchPhase,
        point: (f32, f32),
        size: (u32, u32),
    ) -> bool {
        if matches!(phase, TouchPhase::Ended | TouchPhase::Cancelled) {
            self.touches.remove(&id);
            self.pan_anchor = None;
            self.pinch_anchor = None;
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
                if self.touches.len() >= 3 {
                    self.pan_anchor = None;
                    self.pinch_anchor = None;
                } else if self.touches.len() == 2 && !self.suppressed {
                    self.pan_anchor = self.touch_midpoint();
                    self.pinch_anchor = self.touch_separation();
                }
                self.suppressed |= self.touches.len() > 1;
                false
            }
            TouchPhase::Moved => {
                let Some(previous) = self.touches.get_mut(&id) else {
                    return false;
                };
                let old = *previous;
                *previous = point;
                if self.touches.len() == 2 && self.pan_anchor.is_some() {
                    let Some(midpoint) = self.touch_midpoint() else {
                        return false;
                    };
                    let previous = self
                        .pan_anchor
                        .replace(midpoint)
                        .expect("pan anchor checked");
                    let altitude_changed = if let (Some(distance), Some(previous_distance)) =
                        (self.touch_separation(), self.pinch_anchor)
                    {
                        self.pinch_anchor = Some(distance);
                        let delta = (distance - previous_distance).clamp(
                            -(size.0.min(size.1) as f32) * 0.5,
                            size.0.min(size.1) as f32 * 0.5,
                        );
                        let new_y = (self.position[1]
                            + f64::from(delta) * 16.0 / f64::from(size.0.min(size.1)))
                        .clamp(self.saved_position[1] - 16.0, self.saved_position[1] + 16.0);
                        let changed = new_y != self.position[1];
                        self.position[1] = new_y;
                        changed
                    } else {
                        false
                    };
                    if !self.position.iter().all(|v| v.is_finite()) {
                        return false;
                    }
                    // Pan only across saved columns with retained GPU meshes.
                    // Pinch adjusts the transient view height separately.
                    let dx = (midpoint.0 - previous.0)
                        .clamp(-(size.0 as f32) * 0.25, size.0 as f32 * 0.25);
                    let dy = (midpoint.1 - previous.1)
                        .clamp(-(size.1 as f32) * 0.5, size.1 as f32 * 0.5);
                    if dx == 0.0 && dy == 0.0 {
                        return altitude_changed;
                    }
                    let right = f64::from(dx) * 8.0 / f64::from(size.0);
                    let forward = -f64::from(dy) * 8.0 / f64::from(size.1);
                    let (yaw_sin, yaw_cos) = f64::from(self.rotation[0].to_radians()).sin_cos();
                    let next_x = self.position[0] + yaw_cos * right - yaw_sin * forward;
                    let next_z = self.position[2] + yaw_sin * right + yaw_cos * forward;
                    let origin_x = self.saved_position[0].floor().div_euclid(16.0) * 16.0;
                    let origin_z = self.saved_position[2].floor().div_euclid(16.0) * 16.0;
                    let next_x = next_x.clamp(origin_x - 15.9, origin_x + 31.9);
                    let next_z = next_z.clamp(origin_z - 15.9, origin_z + 31.9);
                    let next_x = self.constrain_axis(next_x, true);
                    let changed_x = next_x != self.position[0];
                    self.position[0] = next_x;
                    let next_z = self.constrain_axis(next_z, false);
                    let changed = altitude_changed || changed_x || next_z != self.position[2];
                    self.position[2] = next_z;
                    return changed;
                }
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
        // Two fingers now intentionally pan, but must never rotate the view.
        assert!(view.touch(1, TouchPhase::Moved, (400.0, 200.0), (1000, 500)));
        let panned = view.position();
        view.touch(2, TouchPhase::Ended, (150.0, 100.0), (1000, 500));
        assert!(!view.touch(1, TouchPhase::Moved, (500.0, 200.0), (1000, 500)));
        assert_eq!(view.position(), panned);
        view.touch(1, TouchPhase::Cancelled, (500.0, 200.0), (0, 0));
        assert_eq!(view.rotation(), [0.0, 0.0]);
        view.touch(3, TouchPhase::Started, (10.0, 10.0), (1000, 500));
        assert!(view.touch(3, TouchPhase::Moved, (110.0, 10.0), (1000, 500)));
        assert_eq!(view.rotation(), [18.0, 0.0]);
    }

    #[test]
    fn two_finger_pan_moves_preview_camera_without_changing_yaw_or_save() {
        let mut camera = PreviewCamera::new([8.0, 72.0, 8.0], [0.0, 0.0]);
        camera.touch(1, TouchPhase::Started, (200.0, 200.0), (1000, 500));
        camera.touch(2, TouchPhase::Started, (400.0, 200.0), (1000, 500));
        assert!(camera.touch(1, TouchPhase::Moved, (300.0, 150.0), (1000, 500)));
        assert!(camera.touch(2, TouchPhase::Moved, (500.0, 150.0), (1000, 500)));
        assert_eq!(camera.rotation(), [0.0, 0.0]);
        assert!((camera.position()[0] - 8.8).abs() < 0.001);
        assert!((camera.position()[2] - 8.8).abs() < 0.001);
        assert_eq!(camera.position()[1], 72.0);
        assert_eq!(camera.saved_position(), [8.0, 72.0, 8.0]);
        camera.reset_view();
        assert_eq!(camera.position(), [8.0, 72.0, 8.0]);
    }

    #[test]
    fn two_finger_pan_stays_inside_saved_chunk_at_negative_coordinates() {
        let mut camera = PreviewCamera::new([128.5, 72.0, -64.25], [90.0, 20.0]);
        camera.touch(1, TouchPhase::Started, (200.0, 200.0), (1000, 500));
        camera.touch(2, TouchPhase::Started, (400.0, 200.0), (1000, 500));
        for _ in 0..200 {
            camera.touch(1, TouchPhase::Moved, (1000.0, 500.0), (1000, 500));
            camera.touch(2, TouchPhase::Moved, (1000.0, 500.0), (1000, 500));
            camera.touch(1, TouchPhase::Moved, (0.0, 0.0), (1000, 500));
            camera.touch(2, TouchPhase::Moved, (0.0, 0.0), (1000, 500));
        }
        let [x, y, z] = camera.position();
        assert!((128.1..=143.9).contains(&x));
        assert!((-79.9..=-64.1).contains(&z));
        assert_eq!(y, 72.0);
        assert_eq!(camera.rotation(), [90.0, 20.0]);
    }

    #[test]
    fn pan_can_enter_a_rendered_neighbour_but_not_an_unmeshed_chunk() {
        let mut camera = PreviewCamera::new([15.7, 64.0, 8.0], [0.0, 0.0]);
        camera.touch(1, TouchPhase::Started, (200.0, 200.0), (1000, 500));
        camera.touch(2, TouchPhase::Started, (400.0, 200.0), (1000, 500));
        assert!(camera.touch(1, TouchPhase::Moved, (300.0, 200.0), (1000, 500)));
        assert!(camera.position()[0] <= 15.9);
        camera.reset_view();
        assert!(camera.allow_rendered_chunk([1, 0]));
        assert!(!camera.allow_rendered_chunk([10, 0]));
        camera.touch(1, TouchPhase::Started, (200.0, 200.0), (1000, 500));
        camera.touch(2, TouchPhase::Started, (400.0, 200.0), (1000, 500));
        assert!(camera.touch(1, TouchPhase::Moved, (300.0, 200.0), (1000, 500)));
        assert!(camera.position()[0] > 16.0);
        assert_eq!(camera.position()[2], 8.0);
        assert_eq!(camera.saved_position(), [15.7, 64.0, 8.0]);
    }

    #[test]
    fn pinch_changes_only_preview_altitude_with_safe_bounds_and_reset() {
        let mut camera = PreviewCamera::new([8.0, 64.0, 8.0], [0.0, 0.0]);
        camera.touch(1, TouchPhase::Started, (300.0, 200.0), (1000, 500));
        camera.touch(2, TouchPhase::Started, (500.0, 200.0), (1000, 500));
        assert!(camera.touch(1, TouchPhase::Moved, (200.0, 200.0), (1000, 500)));
        assert!((camera.position()[1] - 67.2).abs() < 0.001);
        assert_eq!(camera.rotation(), [0.0, 0.0]);
        assert_eq!(camera.saved_position(), [8.0, 64.0, 8.0]);
        for _ in 0..200 {
            camera.touch(1, TouchPhase::Moved, (0.0, 0.0), (1000, 500));
            camera.touch(1, TouchPhase::Moved, (1000.0, 500.0), (1000, 500));
        }
        assert!((48.0..=80.0).contains(&camera.position()[1]));
        camera.reset_view();
        assert_eq!(camera.position(), [8.0, 64.0, 8.0]);
    }

    #[test]
    fn third_finger_suppresses_camera_movement_until_all_release() {
        let mut camera = PreviewCamera::new([8.0, 64.0, 8.0], [0.0, 0.0]);
        camera.touch(1, TouchPhase::Started, (200.0, 200.0), (1000, 500));
        camera.touch(2, TouchPhase::Started, (400.0, 200.0), (1000, 500));
        camera.touch(3, TouchPhase::Started, (500.0, 200.0), (1000, 500));
        assert!(!camera.touch(1, TouchPhase::Moved, (300.0, 200.0), (1000, 500)));
        camera.touch(3, TouchPhase::Ended, (500.0, 200.0), (1000, 500));
        assert!(!camera.touch(2, TouchPhase::Moved, (500.0, 200.0), (1000, 500)));
        assert_eq!(camera.position(), [8.0, 64.0, 8.0]);
        camera.touch(1, TouchPhase::Ended, (300.0, 200.0), (1000, 500));
        camera.touch(2, TouchPhase::Ended, (500.0, 200.0), (1000, 500));
        camera.touch(1, TouchPhase::Started, (200.0, 200.0), (1000, 500));
        camera.touch(2, TouchPhase::Started, (400.0, 200.0), (1000, 500));
        assert!(camera.touch(1, TouchPhase::Moved, (300.0, 200.0), (1000, 500)));
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
