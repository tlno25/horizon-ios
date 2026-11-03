//! Native iOS multi-touch inputs, routed into Horizon's shared renderer and
//! controller actions. This does not fabricate a separate mobile game loop.
use super::*;
use horizon_client::ios_touch::{MenuTouchAction, menu_action, normalized_point};
use winit::event::{Touch, TouchPhase};

impl Game<'_> {
    pub(super) fn ios_handle_touch(
        &mut self,
        event_loop: &dyn event_loop::EventLoopCtl,
        id: WindowId,
        touch: &Touch,
    ) -> Result<()> {
        let Some(size) = self.window.as_ref().map(|window| window.inner_size()) else {
            return Ok(());
        };
        let Some(point) = normalized_point(
            [touch.location.x, touch.location.y],
            [size.width, size.height],
        ) else {
            return Ok(());
        };
        if touch.phase == TouchPhase::Cancelled || !self.focused {
            // Unlike a keyboard, iOS can cancel a gesture mid-hold during a
            // call or app suspension. Never allow a stuck movement/attack.
            self.pad.touch.clear();
            self.ios_menu_touch.cancel();
            return Ok(());
        }
        self.last_input = Instant::now();
        self.pad.in_use = true;

        if self.pad_in_menu() {
            self.pad.touch.clear();
            // Discrete tap on release; a scrolling list must not open a world.
            let action = menu_action(
                &mut self.ios_menu_touch,
                touch.id,
                [touch.location.x as f32, touch.location.y as f32],
                touch.phase,
            );
            self.handle_window_event(
                event_loop,
                id,
                WindowEvent::CursorMoved {
                    device_id: touch.device_id,
                    position: touch.location,
                },
            );
            match action {
                MenuTouchAction::Click => {
                    for state in [ElementState::Pressed, ElementState::Released] {
                        self.handle_window_event(
                            event_loop,
                            id,
                            WindowEvent::MouseInput {
                                device_id: touch.device_id,
                                button: MouseButton::Left,
                                state,
                            },
                        );
                    }
                }
                MenuTouchAction::Scroll(lines) => {
                    self.handle_window_event(
                        event_loop,
                        id,
                        WindowEvent::MouseWheel {
                            device_id: touch.device_id,
                            delta: MouseScrollDelta::LineDelta(0.0, lines),
                            phase: TouchPhase::Moved,
                        },
                    );
                }
                MenuTouchAction::None => {}
            }
            return Ok(());
        }

        self.ios_menu_touch.cancel();
        self.pad.touch.viewport(size.width, size.height);
        self.pad.touch.settings = self.settings.touch.clone();
        self.pad.touch.force_crosshair = true;
        self.pad.touch.sneaking = self.keys.contains(&KeyCode::ShiftLeft);
        self.pad.touch.vertical_movement = self.player.as_ref().is_some_and(|player| player.flying);
        self.pad.touch.swimming = self.player.as_ref().is_some_and(|player| player.swimming);
        self.pad.touch.mounted = self.riding.is_some();

        if touch.phase == TouchPhase::Started {
            let batch = HudBatch::new([size.width, size.height]);
            let scaled = [
                touch.location.x as f32 / batch.scale,
                touch.location.y as f32 / batch.scale,
            ];
            if let Some(slot) =
                horizon_client::touch::hotbar_slot(batch.width, batch.height, scaled)
                && !self.pad.touch.control_at_point(point)
            {
                self.selected = slot;
                self.pad.touch.ignore(touch.id);
                return Ok(());
            }
        }

        let delta = self.pad.touch.update(
            touch.id,
            point,
            touch.phase == TouchPhase::Started,
            touch.phase == TouchPhase::Ended,
        );
        let delta = horizon_client::touch::camera_delta(
            delta,
            size.width,
            size.height,
            &self.pad.touch.settings,
        );
        self.tutorial
            .mouse(f64::from(delta[0]), f64::from(delta[1]));
        // Use the same native turn path as mouse and controller input: this
        // maintains the avatar's previous pitch and boat rotation constraints.
        // camera_delta returns degrees while Entity.turn accepts mouse units.
        if delta != [0.0, 0.0] {
            self.turn_player(f64::from(delta[0]) / 0.15, f64::from(delta[1]) / 0.15);
        }

        if std::mem::take(&mut self.pad.touch.chat_requested) {
            self.key_pressed(KeyCode::KeyT)?;
        }
        if std::mem::take(&mut self.pad.touch.settings_requested) {
            self.open_options(false);
        }
        if std::mem::take(&mut self.pad.touch.perspective_requested) {
            self.key_pressed(KeyCode::F5)?;
        }
        if std::mem::take(&mut self.pad.touch.debug_requested) {
            self.key_pressed(KeyCode::F3)?;
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
        Ok(())
    }
}
