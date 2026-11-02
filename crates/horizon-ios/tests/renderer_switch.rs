use horizon_ios::RendererSwitch;

#[test]
fn requires_ready_game_registry_and_presented_metal_probe() {
    let switching = RendererSwitch::new();
    assert!(!switching.request(false, false));
    assert!(!switching.request(true, false));
    assert!(!switching.request(false, true));
    assert_eq!(switching.state(), 0);
    assert!(switching.request(true, true));
    assert_eq!(switching.state(), 1);
    assert!(!switching.request(true, true));
    assert!(switching.start());
    assert!(!switching.start());
    assert_eq!(switching.state(), 2);
    switching.finished(true);
    assert_eq!(switching.state(), 3);
    assert!(
        !switching.request(true, true),
        "never replace a working renderer twice"
    );
}

#[test]
fn failures_can_retry_but_suspend_discards_current_surface_mode() {
    let switching = RendererSwitch::new();
    assert!(switching.request(true, true));
    assert!(switching.start());
    switching.finished(false);
    assert_eq!(switching.state(), 4);
    assert!(switching.request(true, true));
    switching.reset();
    assert_eq!(switching.state(), 0);
    assert!(switching.request(true, true));
}

#[test]
fn only_one_concurrent_thread_can_request_window_handoff() {
    use std::sync::Arc;
    let switching = Arc::new(RendererSwitch::new());
    let handles: Vec<_> = (0..20)
        .map(|_| {
            let switching = Arc::clone(&switching);
            std::thread::spawn(move || switching.request(true, true))
        })
        .collect();
    assert_eq!(
        handles
            .into_iter()
            .filter_map(|h| h.join().ok())
            .filter(|v| *v)
            .count(),
        1
    );
}

#[test]
fn smoke_switch_is_requested_in_the_same_successful_probe_frame() {
    let switching = RendererSwitch::new();
    assert!(!switching.request_smoke_after_probe_frame(false, true));
    assert!(!switching.request_smoke_after_probe_frame(true, false));
    assert_eq!(switching.state(), 0);
    assert!(switching.request_smoke_after_probe_frame(true, true));
    assert_eq!(switching.state(), 1);
    assert!(!switching.request_smoke_after_probe_frame(true, true));
    assert!(switching.start());
    switching.finished(true);
    assert_eq!(switching.state(), 3);
}
