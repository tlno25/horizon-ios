//! Platform independent checks for resize and suspend/resume policy.
use horizon_ios::ProbeLifecycle;

#[test]
fn probe_never_draws_without_a_valid_size_and_foreground() {
    let mut probe = ProbeLifecycle::default();
    assert_eq!(probe.drawable_size(), None);
    probe.resize(320, 240);
    assert_eq!(probe.drawable_size(), None);
    probe.resume();
    assert_eq!(probe.drawable_size(), Some((320, 240)));
    probe.resize(0, 240);
    assert_eq!(probe.drawable_size(), None);
    probe.resize(320, 0);
    assert_eq!(probe.drawable_size(), None);
    probe.resize(768, 1024);
    assert_eq!(probe.drawable_size(), Some((768, 1024)));
    probe.suspend();
    assert_eq!(probe.drawable_size(), None);
    probe.resume();
    assert_eq!(probe.drawable_size(), Some((768, 1024)));
}

#[test]
fn graphics_are_ready_only_after_a_frame_was_presented() {
    let mut probe = ProbeLifecycle::default();
    probe.resume();
    probe.resize(1170, 2532);
    assert!(!probe.has_presented_frame());
    assert!(probe.present_frame());
    assert!(probe.has_presented_frame());

    // Resizing invalidates the last successfully displayed frame.
    probe.resize(2532, 1170);
    assert!(!probe.has_presented_frame());
    assert!(probe.present_frame());
    assert!(probe.has_presented_frame());

    // Suspending or creating a zero-size view clears readiness.
    probe.suspend();
    assert!(!probe.has_presented_frame());
    assert!(!probe.present_frame());
    probe.resume();
    assert!(!probe.has_presented_frame());
    probe.resize(0, 1170);
    assert!(!probe.present_frame());
    assert!(!probe.has_presented_frame());
}
