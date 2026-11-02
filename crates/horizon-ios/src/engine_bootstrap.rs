//! Platform-independent filesystem preflight, before the game runtime exists.
//!
//! The shared client owns profile persistence and resource detection. The iOS
//! host owns the sandbox directory; neither side starts another event loop.
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnginePreparation {
    pub player_name: String,
    pub resources_ready: bool,
}

/// Ensure a writable sandbox home and run the shared client's profile/resource
/// probes against that exact directory. This does **not** launch gameplay.
pub fn prepare_engine_directory(
    home: &Path,
    load_profile: impl FnOnce(&Path) -> Result<String, String>,
    resources_ready: impl FnOnce(&Path) -> bool,
) -> Result<EnginePreparation, String> {
    if !home.is_absolute() || home.as_os_str().as_encoded_bytes().contains(&0) {
        return Err("Expected an absolute, NUL-free application support path".into());
    }
    std::fs::create_dir_all(home).map_err(|error| format!("Cannot create engine home: {error}"))?;
    let player_name = load_profile(home)?;
    if player_name.is_empty() || player_name.as_bytes().contains(&0) {
        return Err("Invalid player name returned by the game profile".into());
    }
    Ok(EnginePreparation {
        player_name,
        resources_ready: resources_ready(home),
    })
}

/// A single-flight gate for the first real game CPU resource loading stage.
/// A failed attempt can retry after the user replaces their resource files.
#[derive(Debug, Default)]
pub struct CoreLoadGate(std::sync::atomic::AtomicU32);

impl CoreLoadGate {
    pub const fn new() -> Self {
        Self(std::sync::atomic::AtomicU32::new(0))
    }

    #[must_use]
    pub fn state(&self) -> u32 {
        self.0.load(std::sync::atomic::Ordering::Acquire)
    }

    pub fn try_begin(&self) -> bool {
        use std::sync::atomic::Ordering;
        self.0
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
            || self
                .0
                .compare_exchange(3, 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
    }

    pub fn finish(&self, success: bool) {
        self.0.store(
            if success { 2 } else { 3 },
            std::sync::atomic::Ordering::Release,
        );
    }
}
