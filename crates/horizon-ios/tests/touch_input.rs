use horizon_ios::{TouchInput, TouchPhase};

#[test]
fn tracks_multiple_fingers_and_end_events() {
    let mut input = TouchInput::default();
    assert!(input.handle(10, TouchPhase::Started, (10.0, 15.0), (400, 800)));
    assert!(input.handle(20, TouchPhase::Started, (30.0, 35.0), (400, 800)));
    assert_eq!(input.active_count(), 2);
    assert!(input.handle(10, TouchPhase::Moved, (14.0, 24.0), (400, 800)));
    assert_eq!(input.position(10), Some((14.0, 24.0)));
    assert!(input.handle(20, TouchPhase::Ended, (30.0, 35.0), (400, 800)));
    assert_eq!(input.active_count(), 1);
    assert!(input.handle(10, TouchPhase::Cancelled, (14.0, 24.0), (400, 800)));
    assert_eq!(input.active_count(), 0);
    assert_eq!(input.event_count(), 5);
}

#[test]
fn rejects_out_of_order_invalid_events_and_limits_fingers() {
    let mut input = TouchInput::default();
    assert!(!input.handle(1, TouchPhase::Moved, (20.0, 20.0), (320, 600)));
    assert!(!input.handle(1, TouchPhase::Started, (f32::NAN, 4.0), (320, 600)));
    assert!(!input.handle(1, TouchPhase::Started, (4.0, 4.0), (0, 600)));
    for id in 0..10 {
        assert!(input.handle(id, TouchPhase::Started, (10.0, 20.0), (320, 600)));
    }
    assert!(!input.handle(10, TouchPhase::Started, (10.0, 20.0), (320, 600)));
    assert_eq!(input.active_count(), 10);
    assert!(!input.handle(11, TouchPhase::Ended, (4.0, 4.0), (320, 600)));
    assert_eq!(input.event_count(), 10);
}

#[test]
fn clamps_out_of_bounds_coordinates_and_clears_on_suspend() {
    let mut input = TouchInput::default();
    assert!(input.handle(3, TouchPhase::Started, (-20.0, 1000.0), (400, 800)));
    assert_eq!(input.position(3), Some((0.0, 800.0)));
    input.clear();
    assert_eq!(input.active_count(), 0);
    assert_eq!(input.event_count(), 1);
    assert_eq!(input.position(3), None);
}

#[test]
fn ending_a_touch_always_releases_even_when_drawable_disappears() {
    let mut input = TouchInput::default();
    assert!(input.handle(42, TouchPhase::Started, (8.0, 8.0), (100, 200)));
    assert!(input.handle(42, TouchPhase::Cancelled, (f32::NAN, f32::NAN), (0, 0)));
    assert_eq!(input.active_count(), 0);
    assert!(!input.handle(42, TouchPhase::Cancelled, (0.0, 0.0), (0, 0)));
}

#[test]
fn duplicate_start_does_not_count_an_extra_finger_event() {
    let mut input = TouchInput::default();
    assert!(input.handle(42, TouchPhase::Started, (8.0, 8.0), (100, 200)));
    assert!(!input.handle(42, TouchPhase::Started, (11.0, 11.0), (100, 200)));
    assert_eq!(input.event_count(), 1);
    assert_eq!(input.position(42), Some((8.0, 8.0)));
}
