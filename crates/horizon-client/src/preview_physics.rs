//! Opt-in, read-only iOS movement probe. This uses the shared native living
//! jump/gravity and block collision resolver, not a separate gameplay engine.
//! It never mutates World, runs block/entity ticks or writes save files.
use crate::game_startup::ReadOnlyWorldStage;
use horizon_core::geometry::Vec3;
use horizon_world::{
    block_collision::BlockCollisionRegistry,
    living_motion::{self, LivingMotionWorld},
};

/// A temporary standing camera body; it does not represent a saved player.
#[derive(Clone, Debug)]
pub struct PreviewPhysics {
    position: [f64; 3],
    velocity: Vec3,
    grounded: bool,
}

impl PreviewPhysics {
    pub fn new(position: [f64; 3]) -> Self {
        Self {
            position,
            velocity: Vec3::ZERO,
            grounded: false,
        }
    }

    #[must_use]
    pub fn position(&self) -> [f64; 3] {
        self.position
    }

    #[must_use]
    pub fn grounded(&self) -> bool {
        self.grounded
    }

    /// Reconcile a camera touch-pan with this simulated body. This clears
    /// accumulated momentum so gestures cannot turn into uncontrolled falls.
    pub fn sync_camera(&mut self, position: [f64; 3]) {
        if position.iter().all(|v| v.is_finite()) {
            self.position = position;
            self.velocity = Vec3::ZERO;
            self.grounded = false;
        }
    }

    /// One 20 TPS simulation tick; the shell owns its fixed-step scheduling.
    /// Vanilla living jump/gravity are reused. Other motion rules (fluids,
    /// abilities, sprinting, fall damage, step-up) are not yet simulated.
    pub fn tick(
        &mut self,
        stage: &ReadOnlyWorldStage,
        collisions: &BlockCollisionRegistry<'_>,
        jump: bool,
    ) -> crate::Result<bool> {
        let old = self.position;
        let below =
            stage.resolve_preview_movement(collisions, old, [old[0], old[1] - 0.05, old[2]])?;
        self.grounded = (below[1] - old[1]).abs() < 1.0e-7;
        if self.grounded {
            self.velocity.y = 0.0;
            if jump {
                living_motion::jump_from_ground(self)?;
                self.grounded = false;
            }
        }
        let target = [
            old[0] + self.velocity.x,
            old[1] + self.velocity.y,
            old[2] + self.velocity.z,
        ];
        let moved = stage.resolve_preview_movement(collisions, old, target)?;
        if !moved.iter().all(|v| v.is_finite()) {
            return Err("Nonfinite native movement result".into());
        }
        self.position = moved;
        let stopped_up = self.velocity.y > 0.0 && moved[1] + 1.0e-7 < target[1];
        let stopped_down = self.velocity.y < 0.0 && moved[1] > target[1] + 1.0e-7;
        if stopped_down {
            self.grounded = true;
        }
        if stopped_up || stopped_down {
            self.velocity.y = 0.0;
        }
        if !self.grounded {
            living_motion::apply_gravity(self)?;
            self.velocity.y *= 0.98;
        }
        Ok(self.position != old)
    }
}

/// Values for an unmodified standing player without effects or equipment.
/// Delegating to the shared living motion module keeps vanilla jump/gravity
/// arithmetic consistent with the existing Rust desktop implementation.
impl LivingMotionWorld for PreviewPhysics {
    type Error = std::io::Error;
    fn velocity(&self) -> Vec3 {
        self.velocity
    }
    fn set_velocity(&mut self, velocity: Vec3) -> Result<(), Self::Error> {
        if ![velocity.x, velocity.y, velocity.z]
            .iter()
            .all(|v| v.is_finite())
        {
            return Err(std::io::Error::other("Nonfinite preview velocity"));
        }
        self.velocity = velocity;
        Ok(())
    }
    fn jump_strength(&mut self) -> Result<f64, Self::Error> {
        Ok(0.42)
    }
    fn block_jump_factor(&mut self) -> Result<f32, Self::Error> {
        Ok(1.0)
    }
    fn has_jump_boost(&mut self) -> Result<bool, Self::Error> {
        Ok(false)
    }
    fn jump_boost_amplifier(&mut self) -> Result<i32, Self::Error> {
        Ok(0)
    }
    fn sprinting(&mut self) -> Result<bool, Self::Error> {
        Ok(false)
    }
    fn yaw(&mut self) -> Result<f32, Self::Error> {
        Ok(0.0)
    }
    fn mark_needs_sync(&mut self) {}
    fn no_gravity(&mut self) -> Result<bool, Self::Error> {
        Ok(false)
    }
    fn gravity_attribute(&mut self) -> Result<f64, Self::Error> {
        Ok(0.08)
    }
    fn has_slow_falling(&mut self) -> Result<bool, Self::Error> {
        Ok(false)
    }
    fn controlling_passenger_is_player(&mut self) -> Result<bool, Self::Error> {
        Ok(false)
    }
    fn speed(&mut self) -> Result<f32, Self::Error> {
        Ok(0.1)
    }
    fn omnidirectional_air_mover(&mut self) -> Result<bool, Self::Error> {
        Ok(false)
    }
    fn air_drag_modifier(&mut self) -> Result<f64, Self::Error> {
        Ok(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn touch_repositions_body_without_creating_unbounded_momentum() {
        let mut physics = PreviewPhysics::new([8.0, 65.0, 8.0]);
        physics.velocity = Vec3::new(0.0, 0.4, 0.0);
        physics.sync_camera([9.0, 66.0, 8.0]);
        assert_eq!(physics.position(), [9.0, 66.0, 8.0]);
        assert_eq!(physics.velocity, Vec3::ZERO);
        physics.sync_camera([f64::NAN, 80.0, 0.0]);
        assert_eq!(physics.position(), [9.0, 66.0, 8.0]);
    }
}
