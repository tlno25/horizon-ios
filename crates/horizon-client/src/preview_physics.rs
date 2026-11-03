//! Opt-in, read-only iOS movement probe. This uses the shared native living
//! jump/gravity and block collision resolver, not a separate gameplay engine.
//! It never mutates World, runs block/entity ticks or writes save files.
use crate::game_startup::ReadOnlyWorldStage;
use horizon_core::geometry::Vec3;
use horizon_world::{
    block_collision::BlockCollisionRegistry,
    living_air,
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

    /// Camera-relative walking impulse using native Entity.getInputVector.
    /// The legacy dry-stone fixture uses standard 0.6 ground friction.
    pub fn apply_walk_input(&mut self, axes: [f32; 2], yaw: f32) -> crate::Result<()> {
        self.apply_walk_input_on_surface(axes, yaw, 0.6)
    }

    /// Native ground speed vs. friction curve, using the currently sampled
    /// saved support block. This does not yet cover fluids or status effects.
    pub fn apply_walk_input_on_surface(
        &mut self,
        axes: [f32; 2],
        yaw: f32,
        friction: f32,
    ) -> crate::Result<()> {
        if !axes.iter().all(|v| v.is_finite() && v.abs() <= 1.0)
            || !yaw.is_finite()
            || !friction.is_finite()
            || !(0.0..=1.0).contains(&friction)
        {
            return Err("Invalid preview movement input".into());
        }
        let friction = living_air::modified_friction(friction, 1.0);
        let speed = if self.grounded {
            if friction > 0.6 {
                0.1 * (0.21600002f32 / (friction * friction * friction))
            } else {
                0.1
            }
        } else {
            0.02
        };
        let delta = living_air::input_vector(
            Vec3::new(f64::from(axes[0]), 0.0, f64::from(axes[1])),
            speed,
            yaw,
        );
        self.velocity.x += delta.x;
        self.velocity.z += delta.z;
        Ok(())
    }

    /// One 20 TPS simulation tick; the shell owns its fixed-step scheduling.
    /// Vanilla living jump/gravity and native grounded step-up are reused.
    /// Fluids, abilities, sprinting and fall damage are not yet simulated.
    pub fn tick(
        &mut self,
        stage: &ReadOnlyWorldStage,
        collisions: &BlockCollisionRegistry<'_>,
        jump: bool,
    ) -> crate::Result<bool> {
        self.tick_with_input(stage, collisions, jump, [0.0, 0.0], 0.0)
    }

    /// One 20 TPS step of read-only walking, gravity and jumping. The camera
    /// yaw is sampled on the Winit thread at the start of each physics tick.
    pub fn tick_with_input(
        &mut self,
        stage: &ReadOnlyWorldStage,
        collisions: &BlockCollisionRegistry<'_>,
        jump: bool,
        axes: [f32; 2],
        yaw: f32,
    ) -> crate::Result<bool> {
        self.tick_with_materials(stage, collisions, jump, axes, yaw, None)
    }

    /// World-backed preview tick. Real prepared assets provide the native
    /// movement-properties table; old headless test fixtures can omit it.
    pub fn tick_with_materials(
        &mut self,
        stage: &ReadOnlyWorldStage,
        collisions: &BlockCollisionRegistry<'_>,
        jump: bool,
        axes: [f32; 2],
        yaw: f32,
        materials: Option<&horizon_world::movement_properties::BoundMovementProperties>,
    ) -> crate::Result<bool> {
        // Reject bad input before modifying grounded state or velocity.
        if !axes.iter().all(|v| v.is_finite() && v.abs() <= 1.0) || !yaw.is_finite() {
            return Err("Invalid preview movement input".into());
        }
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
        let surface = if self.grounded {
            materials.map_or(Ok(0.6), |properties| {
                stage.support_friction(properties, old)
            })?
        } else {
            0.6
        };
        self.apply_walk_input_on_surface(axes, yaw, surface)?;
        let target = [
            old[0] + self.velocity.x,
            old[1] + self.velocity.y,
            old[2] + self.velocity.z,
        ];
        let moved = stage.resolve_preview_movement_with_step(
            collisions,
            old,
            target,
            if self.grounded { 0.6 } else { 0.0 },
            self.grounded,
        )?;
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
        // Native sampled surface slipperiness (ice, packed ice, slime etc.)
        // drives ground drag. Full PlayerMotion also handles special effects.
        let drag = if self.grounded {
            surface as f64 * 0.91
        } else {
            0.91
        };
        self.velocity.x *= drag;
        self.velocity.z *= drag;
        if self.velocity.x.abs() < 1.0e-5 {
            self.velocity.x = 0.0;
        }
        if self.velocity.z.abs() < 1.0e-5 {
            self.velocity.z = 0.0;
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
    fn walking_input_obeys_camera_yaw_native_direction_and_friction() {
        let mut forward = PreviewPhysics::new([8.0, 64.0, 8.0]);
        forward.grounded = true;
        forward.apply_walk_input([0.0, 1.0], 0.0).unwrap();
        assert!(forward.velocity.z > 0.09);
        assert!(forward.velocity.x.abs() < 0.00001);
        let mut rotated = PreviewPhysics::new([8.0, 64.0, 8.0]);
        rotated.grounded = true;
        rotated.apply_walk_input([0.0, 1.0], 90.0).unwrap();
        assert!(rotated.velocity.x < -0.09);
        assert!(rotated.velocity.z.abs() < 0.00001);
        let mut diagonal = PreviewPhysics::new([8.0, 64.0, 8.0]);
        diagonal.grounded = true;
        diagonal.apply_walk_input([1.0, 1.0], 0.0).unwrap();
        let speed = diagonal.velocity.x.hypot(diagonal.velocity.z);
        assert!(speed <= 0.100001, "diagonals may not move faster: {speed}");
        assert!(
            diagonal
                .apply_walk_input([f32::INFINITY, 0.0], 0.0)
                .is_err()
        );
        assert!(diagonal.apply_walk_input([0.0, 1.0], f32::NAN).is_err());
        assert!(diagonal.apply_walk_input([3.0, 0.0], 0.0).is_err());
        let before = diagonal.velocity;
        diagonal.apply_walk_input([0.0, 0.0], 0.0).unwrap();
        assert_eq!(diagonal.velocity, before);
    }

    #[test]
    fn ice_uses_native_surface_slipperiness_and_dry_ground_speed_scaling() {
        let mut stone = PreviewPhysics::new([8.0, 64.0, 8.0]);
        stone.grounded = true;
        stone
            .apply_walk_input_on_surface([0.0, 1.0], 0.0, 0.6)
            .unwrap();
        let mut ice = PreviewPhysics::new([8.0, 64.0, 8.0]);
        ice.grounded = true;
        ice.apply_walk_input_on_surface([0.0, 1.0], 0.0, 0.98)
            .unwrap();
        assert!(stone.velocity.z > ice.velocity.z * 3.5);
        assert!(
            ice.apply_walk_input_on_surface([0.0, 1.0], f32::NAN, 0.98)
                .is_err()
        );
        assert!(
            ice.apply_walk_input_on_surface([0.0, 1.0], 0.0, f32::INFINITY)
                .is_err()
        );
    }

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
