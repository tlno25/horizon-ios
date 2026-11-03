//! Translate a physical UIKit finger contact into a safe single-button tap.
//! Swipes, multiple fingers, cancels and background transitions never click.
use crate::TouchPhase;

const MAX_MOVEMENT_PIXELS: f32 = 36.0;

#[derive(Debug, Default)]
pub struct TitleTap {
    pressed: Option<(u64, (f32, f32))>,
    moved: bool,
}

impl TitleTap {
    pub fn clear(&mut self) {
        self.pressed = None;
        self.moved = false;
    }

    /// Returns the released point in *physical* drawable pixels only for a tap.
    pub fn handle(
        &mut self,
        id: u64,
        phase: TouchPhase,
        point: (f32, f32),
        size: (u32, u32),
    ) -> Option<(f32, f32)> {
        if !point.0.is_finite()
            || !point.1.is_finite()
            || size.0 == 0
            || size.1 == 0
            || point.0 < 0.0
            || point.1 < 0.0
            || point.0 >= size.0 as f32
            || point.1 >= size.1 as f32
        {
            if self.pressed.is_some_and(|(active, _)| active == id) {
                self.clear();
            }
            return None;
        }
        match phase {
            TouchPhase::Started => {
                if self.pressed.is_none() {
                    self.pressed = Some((id, point));
                    self.moved = false;
                }
            }
            TouchPhase::Moved | TouchPhase::Ended | TouchPhase::Cancelled => {
                let Some((active, start)) = self.pressed else {
                    return None;
                };
                if active != id {
                    return None;
                }
                let dx = point.0 - start.0;
                let dy = point.1 - start.1;
                self.moved |= dx * dx + dy * dy > MAX_MOVEMENT_PIXELS * MAX_MOVEMENT_PIXELS;
                if matches!(phase, TouchPhase::Ended | TouchPhase::Cancelled) {
                    let valid = phase == TouchPhase::Ended && !self.moved;
                    self.clear();
                    return valid.then_some(point);
                }
            }
        }
        None
    }
}
