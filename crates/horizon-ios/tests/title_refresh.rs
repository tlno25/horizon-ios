//! Title-screen refreshes run at a bounded cadence and stop in the background.
use horizon_ios::TitleRefresh;
use std::time::{Duration, Instant};

#[test]
fn title_redraw_is_bounded_and_disables_without_a_drawable() {
    let mut frames = TitleRefresh::default();
    let start = Instant::now();
    assert!(frames.due(start, true));
    assert_eq!(
        frames.next_deadline(),
        Some(start + Duration::from_millis(33))
    );
    assert!(!frames.due(start + Duration::from_millis(10), true));
    assert!(frames.due(start + Duration::from_millis(33), true));
    assert!(!frames.due(start + Duration::from_millis(40), false));
    assert_eq!(frames.next_deadline(), None);
    assert!(frames.due(start + Duration::from_millis(41), true));
}
