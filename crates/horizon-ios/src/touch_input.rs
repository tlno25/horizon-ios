//! Portable multitouch bookkeeping shared by the Swift/UIKit bridge and Winit.
//! The actual gameplay gesture bindings are not connected yet.
use std::collections::BTreeMap;

const MAX_TOUCHES: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchPhase {
    Started,
    Moved,
    Ended,
    Cancelled,
}

#[derive(Debug, Default)]
pub struct TouchInput {
    active: BTreeMap<u64, (f32, f32)>,
    events: u64,
}

impl TouchInput {
    #[must_use]
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    #[must_use]
    pub fn event_count(&self) -> u64 {
        self.events
    }

    #[must_use]
    pub fn position(&self, id: u64) -> Option<(f32, f32)> {
        self.active.get(&id).copied()
    }

    pub fn clear(&mut self) {
        self.active.clear();
    }

    /// Coordinates are in physical window pixels. A real touch is accepted
    /// only for an active, sized Winit window; unmatched end/move is ignored.
    pub fn handle(
        &mut self,
        id: u64,
        phase: TouchPhase,
        position: (f32, f32),
        size: (u32, u32),
    ) -> bool {
        match phase {
            TouchPhase::Started | TouchPhase::Moved => {
                if size.0 == 0 || size.1 == 0 || !position.0.is_finite() || !position.1.is_finite()
                {
                    return false;
                }
                let point = (
                    position.0.clamp(0.0, size.0 as f32),
                    position.1.clamp(0.0, size.1 as f32),
                );
                if phase == TouchPhase::Started {
                    if self.active.contains_key(&id) || self.active.len() >= MAX_TOUCHES {
                        return false;
                    }
                    self.active.insert(id, point);
                } else {
                    let Some(existing) = self.active.get_mut(&id) else {
                        return false;
                    };
                    *existing = point;
                }
            }
            // UIKit may cancel touches during rotation while the drawable has
            // zero dimensions; releasing must never depend on position data.
            TouchPhase::Ended | TouchPhase::Cancelled => {
                if self.active.remove(&id).is_none() {
                    return false;
                }
            }
        }
        self.events = self.events.saturating_add(1);
        true
    }
}
