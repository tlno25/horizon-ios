use horizon_ios::{TitleTap, TouchPhase};

const SIZE: (u32, u32) = (1170, 2532);

#[test]
fn only_a_short_release_of_the_initial_finger_activates_a_title_button() {
    let mut tap = TitleTap::default();
    assert_eq!(
        tap.handle(5, TouchPhase::Started, (520.0, 700.0), SIZE),
        None
    );
    assert_eq!(
        tap.handle(6, TouchPhase::Started, (530.0, 710.0), SIZE),
        None
    );
    assert_eq!(tap.handle(6, TouchPhase::Ended, (530.0, 710.0), SIZE), None);
    assert_eq!(tap.handle(5, TouchPhase::Moved, (526.0, 703.0), SIZE), None);
    assert_eq!(
        tap.handle(5, TouchPhase::Ended, (525.0, 703.0), SIZE),
        Some((525.0, 703.0))
    );
    assert_eq!(tap.handle(5, TouchPhase::Ended, (525.0, 703.0), SIZE), None);
}

#[test]
fn drag_or_cancellation_never_activates_any_title_button() {
    let mut tap = TitleTap::default();
    tap.handle(1, TouchPhase::Started, (100.0, 200.0), SIZE);
    tap.handle(1, TouchPhase::Moved, (250.0, 200.0), SIZE);
    assert_eq!(tap.handle(1, TouchPhase::Ended, (100.0, 200.0), SIZE), None);
    tap.handle(2, TouchPhase::Started, (100.0, 200.0), SIZE);
    assert_eq!(
        tap.handle(2, TouchPhase::Cancelled, (100.0, 200.0), SIZE),
        None
    );
    assert_eq!(tap.handle(2, TouchPhase::Ended, (100.0, 200.0), SIZE), None);
}

#[test]
fn tapping_is_safely_reset_on_background_and_invalid_coordinates() {
    let mut tap = TitleTap::default();
    tap.handle(1, TouchPhase::Started, (100.0, 200.0), SIZE);
    tap.clear();
    assert_eq!(tap.handle(1, TouchPhase::Ended, (100.0, 200.0), SIZE), None);
    tap.handle(2, TouchPhase::Started, (100.0, 200.0), SIZE);
    assert_eq!(
        tap.handle(2, TouchPhase::Ended, (f32::NAN, 200.0), SIZE),
        None
    );
    assert_eq!(
        tap.handle(3, TouchPhase::Started, (10.0, 20.0), (0, 0)),
        None
    );
    assert_eq!(tap.handle(3, TouchPhase::Ended, (10.0, 20.0), SIZE), None);
}
