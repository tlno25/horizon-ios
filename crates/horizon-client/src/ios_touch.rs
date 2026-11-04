//! Platform-neutral menu gestures for the native iOS Winit window.
//! Input is transient: a finger cannot directly mutate a saved world.

use crate::{gamepad::PadState, touch::TouchControls, touch_ui::TouchUi};
use winit::event::TouchPhase;

/// Committed iOS keyboard text may contain IME control characters that are
/// not valid title-field data. Unicode letters, emoji and composition survive.
pub fn committed_ime_text(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_control())
        .collect()
}

/// Which text field owns iOS keyboard composition during loading/menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadingTextTarget {
    None,
    Title,
    Options,
}

/// An open Options dialog always takes focus precedence over the title
/// underneath. Typing in one must never mutate the other.
pub fn loading_text_target(
    options_open: bool,
    options_typing: bool,
    title_focused: bool,
) -> LoadingTextTarget {
    if options_open {
        if options_typing {
            LoadingTextTarget::Options
        } else {
            LoadingTextTarget::None
        }
    } else if title_focused {
        LoadingTextTarget::Title
    } else {
        LoadingTextTarget::None
    }
}

/// The same controls used by Android feed the game's native controller
/// mapping. A short tap becomes a single frame of attack/use.
pub fn poll_gamepad(controls: &mut TouchControls) -> PadState {
    let mut state = controls.poll_state();
    if let Some(use_item) = controls.take_tap() {
        state.triggers[if use_item { 0 } else { 1 }] = 1.0;
    }
    state
}

/// UIKit touch coordinates are physical window pixels. Reject invalid events
/// instead of forwarding NaN, infinity or zero viewport sizes to physics.
pub fn normalized_point(point: [f64; 2], size: [u32; 2]) -> Option<[f32; 2]> {
    if size.contains(&0) || !point.iter().all(|value| value.is_finite()) {
        return None;
    }
    Some([
        (point[0] / f64::from(size[0])).clamp(0.0, 1.0) as f32,
        (point[1] / f64::from(size[1])).clamp(0.0, 1.0) as f32,
    ])
}

/// UIKit does not implement an OS mouse pointer lock. Gameplay still owns
/// the touch surface logically while no menu is open. Desktop platforms must
/// only claim capture if the native pointer grab actually succeeded.
pub fn touchscreen_capture(native_grab_succeeded: bool, ios_touch_surface: bool) -> bool {
    ios_touch_surface || native_grab_succeeded
}

/// Movement on mobile must not require a separate gamepad to be enabled.
/// Menu and focus checks still apply to prevent stuck movement.
pub fn movement_input_enabled(
    controller_enabled: bool,
    mobile_touchscreen: bool,
    menu_open: bool,
    focused: bool,
) -> bool {
    (controller_enabled || mobile_touchscreen) && !menu_open && focused
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MenuTouchAction {
    None,
    Click,
    Scroll(f32),
}

/// Reuse Horizon's existing gesture thresholds so a scrolling touch can never
/// accidentally open a saved world. The caller owns hit-testing/rendering.
pub fn menu_action(
    ui: &mut TouchUi,
    id: u64,
    physical_point: [f32; 2],
    phase: TouchPhase,
) -> MenuTouchAction {
    match ui.gesture(id, physical_point, phase) {
        crate::touch_ui::Gesture::None => MenuTouchAction::None,
        crate::touch_ui::Gesture::Tap => MenuTouchAction::Click,
        crate::touch_ui::Gesture::Scroll(lines) => MenuTouchAction::Scroll(lines),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ios_gameplay_capture_is_logical_without_os_pointer_lock() {
        assert!(touchscreen_capture(false, true));
        assert!(touchscreen_capture(true, true));
        assert!(touchscreen_capture(true, false));
        assert!(!touchscreen_capture(false, false));
    }

    #[test]
    fn touchscreen_movement_remains_available_with_hardware_pads_disabled() {
        assert!(movement_input_enabled(false, true, false, true));
        assert!(movement_input_enabled(true, false, false, true));
        assert!(!movement_input_enabled(false, false, false, true));
        assert!(!movement_input_enabled(false, true, true, true));
        assert!(!movement_input_enabled(false, true, false, false));
    }

    #[test]
    fn title_ime_never_leaks_keystrokes_into_a_hidden_world_field() {
        assert_eq!(
            loading_text_target(true, false, true),
            LoadingTextTarget::None,
        );
        assert_eq!(
            loading_text_target(true, true, true),
            LoadingTextTarget::Options,
        );
        assert_eq!(
            loading_text_target(false, false, true),
            LoadingTextTarget::Title,
        );
        assert_eq!(
            loading_text_target(false, false, false),
            LoadingTextTarget::None,
        );
    }

    #[test]
    fn committed_ime_text_preserves_unicode_but_excludes_control_input() {
        assert_eq!(committed_ime_text("テスト\n🙂\0abc\r"), "テスト🙂abc");
        assert_eq!(committed_ime_text(""), "");
    }

    #[test]
    fn shared_touch_button_reaches_gamepad_for_one_poll_only() {
        use crate::gamepad::Button;
        use crate::touch::TouchControls;
        let mut controls = TouchControls::default();
        controls.tap(Button::South);
        let first = poll_gamepad(&mut controls);
        assert!(
            first.held(Button::South),
            "jump must reach the shared gamepad"
        );
        assert!(
            !poll_gamepad(&mut controls).held(Button::South),
            "tap must not stick"
        );
    }

    #[test]
    fn normalized_point_rejects_invalid_dimensions_and_nonfinite_coordinates() {
        assert_eq!(
            normalized_point([160.0, 90.0], [320, 180]),
            Some([0.5, 0.5])
        );
        assert_eq!(
            normalized_point([-50.0, 400.0], [320, 180]),
            Some([0.0, 1.0])
        );
        assert_eq!(normalized_point([10.0, 10.0], [0, 180]), None);
        assert_eq!(normalized_point([f64::NAN, 10.0], [320, 180]), None);
        assert_eq!(normalized_point([10.0, f64::INFINITY], [320, 180]), None);
    }

    #[test]
    fn tap_clicks_only_after_release() {
        let mut ui = TouchUi::default();
        assert_eq!(
            menu_action(&mut ui, 7, [100.0, 100.0], TouchPhase::Started),
            MenuTouchAction::None
        );
        assert_eq!(
            menu_action(&mut ui, 7, [102.0, 100.0], TouchPhase::Moved),
            MenuTouchAction::None
        );
        assert_eq!(
            menu_action(&mut ui, 7, [102.0, 100.0], TouchPhase::Ended),
            MenuTouchAction::Click
        );
    }

    #[test]
    fn scrolling_or_cancel_never_activates_a_menu_button() {
        let mut ui = TouchUi::default();
        assert_eq!(
            menu_action(&mut ui, 1, [100.0, 100.0], TouchPhase::Started),
            MenuTouchAction::None
        );
        assert_eq!(
            menu_action(&mut ui, 2, [100.0, 100.0], TouchPhase::Ended),
            MenuTouchAction::None
        );
        assert_eq!(
            menu_action(&mut ui, 1, [100.0, 160.0], TouchPhase::Moved),
            MenuTouchAction::Scroll(2.0)
        );
        assert_eq!(
            menu_action(&mut ui, 1, [100.0, 160.0], TouchPhase::Ended),
            MenuTouchAction::None
        );
        assert_eq!(
            menu_action(&mut ui, 3, [100.0, 100.0], TouchPhase::Started),
            MenuTouchAction::None
        );
        assert_eq!(
            menu_action(&mut ui, 3, [100.0, 100.0], TouchPhase::Cancelled),
            MenuTouchAction::None
        );
    }
}
