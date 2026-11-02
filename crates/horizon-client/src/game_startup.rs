//! Reusable first CPU stage of Horizon's native game loader.
//!
//! The desktop/Android `loading_app` and iOS both read the same registry and
//! game data. This does not create a Winit event loop, a renderer or a world.
use crate::{Result, assets::Assets};
use std::path::Path;

/// Load the pinned block registry, block facts and vanilla block tags.
/// The native game loader calls this before building meshes and gameplay tables.
/// `Assets::load` leaks one immutable registry for the process lifetime, so
/// platform hosts must call this at most once per completed startup.
pub fn load_core_registry(root: &Path) -> Result<Assets> {
    Assets::load(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, time::SystemTime};

    #[test]
    fn rejects_missing_or_mismatched_game_data_before_creating_a_world() {
        let dir = std::env::temp_dir().join(format!(
            "horizon-game-startup-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        assert!(load_core_registry(&dir).is_err());
        fs::write(
            dir.join("horizon-resources.json"),
            br#"{"data_version":-999}"#,
        )
        .unwrap();
        let err = load_core_registry(&dir)
            .err()
            .expect("invalid data must be rejected");
        assert!(err.to_string().contains("data version"), "{err}");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn valid_manifest_alone_does_not_count_as_a_loaded_game_registry() {
        let dir = std::env::temp_dir().join(format!(
            "horizon-game-missing-facts-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("horizon-resources.json"),
            format!(r#"{{"data_version":{}}}"#, horizon_world::DATA_VERSION),
        )
        .unwrap();
        assert!(
            load_core_registry(&dir).is_err(),
            "missing facts.json must fail actual game startup"
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
