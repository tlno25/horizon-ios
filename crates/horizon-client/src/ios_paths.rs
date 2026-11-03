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
