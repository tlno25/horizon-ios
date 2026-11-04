//! Launcher-facing options persisted with the shared Horizon options.txt format.
//! Unknown settings and user-created lines must survive a focused edit.
use crate::options_file::OptionsFile;
use std::{fs, io::Write, path::Path};

/// Only options exposed by the small iOS launcher can be changed here.
/// The shared Horizon parser performs final type, range and step validation.
const SETTINGS: &[&str] = &[
    "renderDistance",
    "simulationDistance",
    "maxFps",
    "fov",
    "soundCategory_master",
    "autoJump",
    "mouseSensitivity",
    "enableVsync",
];

pub fn game_settings_json(home: &Path) -> Result<String, String> {
    let file = read_options(&home.join("options.txt"))?;
    let options = OptionsFile::parse(&file);
    let mut values = serde_json::Map::new();
    for key in SETTINGS {
        let value = options
            .describe(key)
            .ok_or(format!("Unsupported option {key}"))?;
        values.insert((*key).to_owned(), value["value"].clone());
    }
    serde_json::to_string(&values).map_err(|error| error.to_string())
}

pub fn set_game_setting(home: &Path, key: &str, value: &str) -> Result<(), String> {
    if !SETTINGS.contains(&key) || value.len() > 64 {
        return Err("Unsupported launcher setting".into());
    }
    let path = home.join("options.txt");
    let original = read_options(&path)?;
    let mut file = OptionsFile::parse(&original);
    file.set_text(key, value)?;
    // Use the game's canonical encoding (notably FOV's encoded float), while
    // preserving every unrelated line, including unknown options and mods.
    let serialized = file.serialize();
    let line = serialized
        .lines()
        .find(|line| line.starts_with(&format!("{key}:")))
        .ok_or_else(|| "Cannot serialize the selected option".to_owned())?;
    let mut replaced = false;
    let mut result = String::new();
    for old in original.lines() {
        if old.split_once(':').is_some_and(|(name, _)| name == key) {
            if !replaced {
                result.push_str(line);
                result.push('\n');
                replaced = true;
            }
        } else {
            result.push_str(old);
            result.push('\n');
        }
    }
    if !replaced {
        result.push_str(line);
        result.push('\n');
    }
    atomic_replace(&path, result.as_bytes())
}

pub fn set_offline_name(home: &Path, name: &str) -> Result<(), String> {
    if !(3..=16).contains(&name.len())
        || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
        || !crate::offline_name::is_valid(name)
    {
        return Err("Player name must be 3–16 ASCII letters, numbers or underscores".into());
    }
    atomic_replace(
        &home.join(crate::offline_name::FILE),
        format!("name:{name}\n").as_bytes(),
    )
}

fn read_options(path: &Path) -> Result<String, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.is_file() || metadata.len() > 2 * 1024 * 1024 => {
            return Err("options.txt must be a regular file at most 2 MiB".into());
        }
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
        _ => {}
    }
    match fs::read_to_string(path) {
        Ok(data) => Ok(data),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e.to_string()),
    }
}

fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let folder = path.parent().ok_or("Missing launcher settings directory")?;
    fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    let pending = folder.join(format!(".horizon-settings-{}.tmp", std::process::id()));
    let operation = (|| -> std::io::Result<()> {
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending)?;
        output.write_all(bytes)?;
        output.sync_all()?;
        fs::rename(&pending, path)
    })();
    if operation.is_err() {
        let _ = fs::remove_file(&pending);
    }
    operation.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(case: &str) -> std::path::PathBuf {
        let d =
            std::env::temp_dir().join(format!("horizon-launcher-{case}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn reads_defaults_and_persists_validated_options_without_losing_other_lines() {
        let d = dir("settings");
        let old = "version:4000\nrenderDistance:12\nmyCustomOption:leave_me\nmaxFps:120\n";
        std::fs::write(d.join("options.txt"), old).unwrap();
        set_game_setting(&d, "renderDistance", "8").unwrap();
        let raw = std::fs::read_to_string(d.join("options.txt")).unwrap();
        assert!(raw.contains("renderDistance:8\n"));
        assert!(raw.contains("maxFps:120\n"));
        assert!(raw.contains("myCustomOption:leave_me\n"));
        let settings: serde_json::Value =
            serde_json::from_str(&game_settings_json(&d).unwrap()).unwrap();
        assert_eq!(settings["renderDistance"], 8);
        assert_eq!(settings["maxFps"], 120);
        assert!(settings["fov"].as_i64().is_some());
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn rejects_unknown_or_invalid_values_without_writing() {
        let d = dir("invalid");
        let existing = "renderDistance:12\n";
        std::fs::write(d.join("options.txt"), existing).unwrap();
        for (key, value) in [
            ("renderDistance", "-5"),
            ("fov", "500"),
            ("badKey", "true"),
            ("renderDistance", "eight"),
        ] {
            assert!(set_game_setting(&d, key, value).is_err(), "{key} {value}");
            assert_eq!(
                std::fs::read_to_string(d.join("options.txt")).unwrap(),
                existing
            );
        }
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn saves_and_validates_offline_name_without_destroying_previous_identity() {
        let d = dir("player");
        set_offline_name(&d, "teluno_25").unwrap();
        assert_eq!(
            std::fs::read_to_string(d.join("horizon-player.txt")).unwrap(),
            "name:teluno_25\n"
        );
        for name in [
            "",
            "x",
            "!bad",
            "has space",
            "日本語",
            "this-name-is-way-too-long",
        ] {
            assert!(set_offline_name(&d, name).is_err());
        }
        assert_eq!(
            std::fs::read_to_string(d.join("horizon-player.txt")).unwrap(),
            "name:teluno_25\n"
        );
        std::fs::remove_dir_all(d).unwrap();
    }
}
