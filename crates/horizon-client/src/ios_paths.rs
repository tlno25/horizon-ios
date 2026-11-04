//! Writable iOS game directory used by the native Horizon executable.

use std::path::{Path, PathBuf};

/// Return a writable location for game saves and prepared files. The caller
/// creates this path and changes the current directory before the Rust game
/// parses relative resource and save paths. Never use the immutable app bundle.
pub fn game_documents_dir(home: &Path) -> Result<PathBuf, &'static str> {
    if !home.is_absolute() || home.as_os_str().as_encoded_bytes().contains(&0) {
        return Err("iOS HOME must be an absolute, NUL-free container directory");
    }
    Ok(home.join("Documents"))
}

/// Use an app-visible log/crash-report location unless the caller explicitly
/// chose a different game home. This is evaluated before logging::init, so
/// every first-launch or graphics failure can be found through iOS Files.
/// Neither CLI --home nor HORIZON_HOME may be silently overridden.
pub fn default_log_home(
    documents: &Path,
    explicit_cli_home: bool,
    environment_home: bool,
) -> Option<PathBuf> {
    (!explicit_cli_home && !environment_home).then(|| documents.to_path_buf())
}

/// Files.app exposes the Documents/resources directory when file sharing is
/// enabled. Accept it only if prepared resources actually exist there;
/// legacy .local imports remain usable without silently migrating game data.
pub fn game_resources_dir(documents: &Path, version: &str) -> PathBuf {
    let visible = documents.join("resources");
    if visible.join("horizon-resources.json").is_file() {
        visible
    } else {
        documents.join(".local/horizon-client").join(version)
    }
}

/// Check that Files.app received the prepared core, not just its manifest.
/// iOS file copying is not atomic: a partially copied folder can already
/// contain horizon-resources.json. Avoid starting the expensive world loader
/// with missing data, and instead show the user an actionable import error.
/// This is a cheap structural preflight, NOT the preparer's checksum check.
pub fn validate_prepared_resources(root: &Path, version: &str) -> Result<(), String> {
    let manifest_path = root.join("horizon-resources.json");
    let metadata = std::fs::metadata(&manifest_path)
        .map_err(|_| "horizon-resources.json is missing from Documents/resources".to_owned())?;
    if !metadata.is_file() || metadata.len() > 1024 * 1024 {
        return Err("horizon-resources.json is not a valid manifest file".into());
    }
    let text = std::fs::read_to_string(&manifest_path)
        .map_err(|_| "Cannot read horizon-resources.json".to_owned())?;
    let manifest: serde_json::Value = serde_json::from_str(&text).map_err(|_| {
        "Invalid horizon-resources.json (copy the entire prepared folder)".to_owned()
    })?;
    if !manifest
        .get("schema")
        .and_then(serde_json::Value::as_u64)
        .is_some_and(|schema| schema > 0)
    {
        return Err("Invalid horizon-resources.json: missing preparation schema".into());
    }
    if manifest
        .get("minecraft")
        .and_then(serde_json::Value::as_str)
        != Some(version)
    {
        return Err(format!(
            "Prepared resources must be for Minecraft {version}"
        ));
    }
    // Keep this aligned with the game's server-side core preflight. Extra
    // packs, languages, sounds and panorama are optional at this stage.
    for name in [
        "facts.json",
        "block-tags.json",
        "registries.json",
        "commands.json",
    ] {
        if !root.join(name).is_file() {
            return Err(format!("Prepared resources are missing {name}"));
        }
    }
    for name in ["assets/minecraft/font", "assets/minecraft/textures"] {
        if std::fs::read_dir(root.join(name)).map_or(true, |mut entries| entries.next().is_none()) {
            return Err(format!("Prepared resources are missing {name}"));
        }
    }
    Ok(())
}

/// iOS's UIKit window must use its native screen dimensions. The desktop
/// defaults (1280×720 in points) would crop the phone's Winit surface.
pub fn preferred_window_size(size: [f64; 2], is_ios: bool) -> Option<[f64; 2]> {
    (!is_ios).then_some(size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn finds_documents_beneath_the_absolute_ios_container_home() {
        let home = Path::new("/private/var/mobile/Containers/Data/Application/UUID");
        assert_eq!(game_documents_dir(home).unwrap(), home.join("Documents"),);
    }

    #[test]
    fn ios_logs_are_visible_by_default_but_respect_home_overrides() {
        let documents = Path::new("/private/var/mobile/Containers/Data/Application/UUID/Documents");
        assert_eq!(
            default_log_home(documents, false, false),
            Some(documents.to_path_buf())
        );
        assert_eq!(default_log_home(documents, true, false), None);
        assert_eq!(default_log_home(documents, false, true), None);
        assert_eq!(default_log_home(documents, true, true), None);
    }

    #[test]
    fn prefers_visible_documents_resources_when_prepared_then_falls_back() {
        let dir =
            std::env::temp_dir().join(format!("horizon-ios-resources-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(
            game_resources_dir(&dir, "26.3"),
            dir.join(".local/horizon-client/26.3")
        );
        std::fs::create_dir_all(dir.join("resources")).unwrap();
        assert_eq!(
            game_resources_dir(&dir, "26.3"),
            dir.join(".local/horizon-client/26.3")
        );
        std::fs::write(dir.join("resources/horizon-resources.json"), "{}").unwrap();
        assert_eq!(game_resources_dir(&dir, "26.3"), dir.join("resources"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A mobile copy may finish writing the manifest before its large assets.
    /// iOS must not start the game loader until the core prepared files exist.
    #[test]
    fn resource_preflight_rejects_partial_or_wrong_version_imports() {
        let dir = std::env::temp_dir().join(format!(
            "horizon-ios-incomplete-preflight-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(
            validate_prepared_resources(&dir, "26.3")
                .unwrap_err()
                .contains("horizon-resources.json")
        );
        std::fs::write(dir.join("horizon-resources.json"), "{").unwrap();
        assert!(
            validate_prepared_resources(&dir, "26.3")
                .unwrap_err()
                .contains("Invalid")
        );
        std::fs::write(
            dir.join("horizon-resources.json"),
            r#"{"schema": 4, "minecraft": "1.20.4"}"#,
        )
        .unwrap();
        assert!(
            validate_prepared_resources(&dir, "26.3")
                .unwrap_err()
                .contains("26.3")
        );
        std::fs::write(
            dir.join("horizon-resources.json"),
            r#"{"schema": 4, "minecraft": "26.3"}"#,
        )
        .unwrap();
        assert!(
            validate_prepared_resources(&dir, "26.3")
                .unwrap_err()
                .contains("facts.json")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resource_preflight_accepts_a_complete_minimal_core_without_optional_assets() {
        let dir = std::env::temp_dir().join(format!(
            "horizon-ios-complete-preflight-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("horizon-resources.json"),
            r#"{"schema": 4, "minecraft": "26.3"}"#,
        )
        .unwrap();
        for name in [
            "facts.json",
            "block-tags.json",
            "registries.json",
            "commands.json",
        ] {
            std::fs::write(dir.join(name), "{}").unwrap();
        }
        for folder in ["assets/minecraft/font", "assets/minecraft/textures"] {
            let root = dir.join(folder);
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(
                root.join("fixture"),
                "private files not needed in a unit test",
            )
            .unwrap();
        }
        assert_eq!(validate_prepared_resources(&dir, "26.3"), Ok(()));
        std::fs::remove_file(dir.join("assets/minecraft/textures/fixture")).unwrap();
        assert!(
            validate_prepared_resources(&dir, "26.3")
                .unwrap_err()
                .contains("textures")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ios_uses_native_screen_size_instead_of_desktop_default() {
        assert_eq!(preferred_window_size([1280.0, 720.0], true), None);
        assert_eq!(
            preferred_window_size([1280.0, 720.0], false),
            Some([1280.0, 720.0])
        );
    }

    #[test]
    fn rejects_missing_relative_and_null_home_before_disk_changes() {
        for home in ["", ".", "relative/path", "home\0poison"] {
            assert!(game_documents_dir(Path::new(home)).is_err(), "{home:?}");
        }
    }
}
