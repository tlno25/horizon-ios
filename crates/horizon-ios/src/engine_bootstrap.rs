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
