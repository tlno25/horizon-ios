//! Platform-neutral menu gestures for the native iOS Winit window.
//! Input is transient: a finger cannot directly mutate a saved world.

use crate::touch_ui::TouchUi;
use winit::event::TouchPhase;

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
