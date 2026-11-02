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
