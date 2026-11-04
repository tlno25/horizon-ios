//! SwiftUI launcher requests on the native game's *existing* Winit event loop.
//! Never create a second UIApplicationMain, never block UIKit for a ZIP import.
use super::*;
use serde_json::{Value, json};
use std::sync::mpsc::{self, Receiver, TryRecvError};

pub(super) struct ImportJob {
    result: Receiver<std::result::Result<(), String>>,
    response: horizon_ios_launcher::Request,
}

/// A downloaded archive is named by the app, not an untrusted arbitrary path.
/// The game owns its Documents directory, so the caller cannot request a ZIP
/// outside that container through the bridge.
fn installed_archive_name(value: &str) -> Option<&str> {
    value
        .starts_with("horizon-download-")
        .then_some(value)
        .filter(|value| value.ends_with(".zip") && value.len() < 100)
        .filter(|value| {
            value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'.')
        })
}

impl Loading {
    /// A frame polls pending Swift requests and importer workers. All callbacks
    /// are completed here on Winit's main thread, which owns the Swift views.
    pub(super) fn poll_native_launcher(&mut self) {
        let mut index = 0;
        while index < self.native_imports.len() {
            match self.native_imports[index].result.try_recv() {
                Ok(result) => {
                    let job = self.native_imports.swap_remove(index);
                    match result {
                        Ok(()) => job.response.answer(r#"{"ok":true}"#),
                        Err(error) => job.response.answer(&json!({"error":error}).to_string()),
                    }
                }
                Err(TryRecvError::Disconnected) => {
                    let job = self.native_imports.swap_remove(index);
                    job.response
                        .answer(r#"{"error":"Import worker stopped unexpectedly"}"#);
                }
                Err(TryRecvError::Empty) => index += 1,
            }
        }
        for request in horizon_ios_launcher::take() {
            let line = request.line.trim();
            let command = line.split_whitespace().next().unwrap_or_default();
            match command {
                "launcher-state" => {
                    let home = std::env::current_dir().unwrap_or_default();
                    let prepared = horizon_client::ios_paths::game_resources_dir(
                        &home,
                        horizon_core::VANILLA_VERSION,
                    );
                    let ready = horizon_client::ios_paths::validate_prepared_resources(
                        &prepared,
                        horizon_core::VANILLA_VERSION,
                    )
                    .is_ok();
                    let state = json!({
                        "phase": if horizon_ios_launcher::waiting() { "waiting" } else { "loading" },
                        "ready": ready,
                        "version": horizon_core::VANILLA_VERSION,
                        "progress": self.progress.fraction(),
                    });
                    request.answer(&state.to_string());
                }
                "launcher-settings" => {
                    let answer =
                        std::env::current_dir()
                            .map_err(|e| e.to_string())
                            .and_then(|home| {
                                horizon_client::ios_launcher_settings::game_settings_json(&home)
                            });
                    match answer {
                        Ok(json) => request.answer(&json),
                        Err(error) => request.answer(&json!({"error":error}).to_string()),
                    }
                }
                "launcher-option" => {
                    let result = line
                        .strip_prefix("launcher-option ")
                        .and_then(|s| serde_json::from_str::<Value>(s).ok())
                        .and_then(|value| {
                            Some((
                                value.get("key")?.as_str()?.to_owned(),
                                value.get("value")?.as_str()?.to_owned(),
                            ))
                        })
                        .ok_or_else(|| "Expected option JSON with key and string value".to_owned())
                        .and_then(|(key, value)| {
                            let home = std::env::current_dir().map_err(|e| e.to_string())?;
                            horizon_client::ios_launcher_settings::set_game_setting(
                                &home, &key, &value,
                            )
                        });
                    match result {
                        Ok(()) => request.answer(r#"{"ok":true}"#),
                        Err(error) => request.answer(&json!({"error":error}).to_string()),
                    }
                }
                "launcher-player-get" => {
                    let name = std::env::current_dir()
                        .map_err(|e| e.to_string())
                        .map(|home| horizon_client::offline_name::load_or_create(&home));
                    match name {
                        Ok(name) => request.answer(&json!({"name": name}).to_string()),
                        Err(error) => request.answer(&json!({"error": error}).to_string()),
                    }
                }
                "launcher-player" => {
                    let name = line.strip_prefix("launcher-player ").unwrap_or_default();
                    let result =
                        std::env::current_dir()
                            .map_err(|e| e.to_string())
                            .and_then(|home| {
                                horizon_client::ios_launcher_settings::set_offline_name(&home, name)
                            });
                    match result {
                        Ok(()) => request.answer(r#"{"ok":true}"#),
                        Err(error) => request.answer(&json!({"error":error}).to_string()),
                    }
                }
                "launcher-import" => {
                    let name = line.strip_prefix("launcher-import ").unwrap_or_default();
                    let Some(name) = installed_archive_name(name) else {
                        request.answer(r#"{"error":"Invalid downloaded archive name"}"#);
                        continue;
                    };
                    if !self.native_imports.is_empty() {
                        request.answer(r#"{"error":"Another import is in progress"}"#);
                        continue;
                    }
                    let home = match std::env::current_dir() {
                        Ok(path) => path,
                        Err(error) => {
                            request.answer(&json!({"error":error.to_string()}).to_string());
                            continue;
                        }
                    };
                    let path = home.join(name);
                    let (tx, rx) = mpsc::channel();
                    match std::thread::Builder::new()
                        .name("ios-resource-import".into())
                        .spawn(move || {
                            let result = horizon_client::resource_install::import_archive_checked(
                                &path, &home,
                            )
                            .map_err(|e| e.to_string());
                            let _ = tx.send(result);
                        }) {
                        Ok(_) => self.native_imports.push(ImportJob {
                            result: rx,
                            response: request,
                        }),
                        Err(error) => {
                            request.answer(&json!({"error":error.to_string()}).to_string())
                        }
                    }
                }
                "launcher-failed" => {
                    self.error = Some("The iOS SwiftUI launcher could not be presented".into());
                    request.answer(r#"{"error":"Cannot display the iOS launcher"}"#);
                }
                "launcher-start" => {
                    let home = std::env::current_dir().unwrap_or_default();
                    let root = horizon_client::ios_paths::game_resources_dir(
                        &home,
                        horizon_core::VANILLA_VERSION,
                    );
                    match horizon_client::ios_paths::validate_prepared_resources(
                        &root,
                        horizon_core::VANILLA_VERSION,
                    ) {
                        Ok(()) => {
                            self.vanilla_root = root.clone();
                            if let Some(options) = self.options.as_mut() {
                                options.resources = root;
                            }
                            self.settings = Settings::load(&home.join("options.txt"));
                            #[cfg(target_os = "ios")]
                            if let Some(window) = self.window.as_deref() {
                                use winit::platform::ios::{ValidOrientations, WindowExtIOS};
                                window.set_valid_orientations(ValidOrientations::Landscape);
                            }
                            horizon_ios_launcher::start();
                            request.answer(r#"{"ok":true}"#);
                        }
                        Err(error) => request.answer(&json!({"error":error}).to_string()),
                    }
                }
                _ => request.answer(r#"{"error":"Unknown native launcher command"}"#),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sandbox_archive_names_only() {
        assert!(installed_archive_name("horizon-download-ABC123.zip").is_some());
        for value in [
            "../bad.zip",
            "a.zip",
            "horizon-download-../../bad.zip",
            "horizon-download-foo.txt",
            "horizon-download-%2F.zip",
        ] {
            assert!(installed_archive_name(value).is_none(), "{value}");
        }
    }
}
