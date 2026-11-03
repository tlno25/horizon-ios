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
    fn rejects_missing_relative_and_null_home_before_disk_changes() {
        for home in ["", ".", "relative/path", "home\0poison"] {
            assert!(game_documents_dir(Path::new(home)).is_err(), "{home:?}");
        }
    }
}
