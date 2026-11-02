//! Verifies the same exported ABI that the Swift app calls.
use std::ffi::CStr;

#[test]
fn exposes_version_and_idempotent_shell_lifecycle() {
    assert_eq!(horizon_ios::horizon_ios_abi_version(), 1);
    // Metal is unavailable on Linux and is never initialized by this FFI test.
    assert_eq!(horizon_ios::horizon_ios_graphics_state(), 0);
    assert_eq!(horizon_ios::horizon_ios_renderer_preview_state(), 0);
    assert_eq!(horizon_ios::horizon_ios_engine_state(), 0);
    assert_eq!(horizon_ios::horizon_ios_core_load_state(), 0);
    assert_eq!(horizon_ios::horizon_ios_touch_count(), 0);
    assert_eq!(horizon_ios::horizon_ios_touch_events(), 0);
    assert!(horizon_ios::horizon_ios_player_name().is_null());
    let version = unsafe { CStr::from_ptr(horizon_ios::horizon_ios_game_version()) };
    assert_eq!(
        version.to_str().unwrap(),
        horizon_core::version::VANILLA_VERSION
    );

    assert_eq!(horizon_ios::horizon_ios_shutdown(), 0);
    assert_eq!(horizon_ios::horizon_ios_state(), 0);
    assert_eq!(horizon_ios::horizon_ios_initialize(), 0);
    assert_eq!(horizon_ios::horizon_ios_initialize(), 0);
    assert_eq!(horizon_ios::horizon_ios_state(), 1);
    assert_eq!(horizon_ios::horizon_ios_shutdown(), 0);
    assert_eq!(horizon_ios::horizon_ios_state(), 0);
}
