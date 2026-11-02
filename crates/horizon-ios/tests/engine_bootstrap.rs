use horizon_ios::{EnginePreparation, prepare_engine_directory};
use std::path::Path;

#[test]
fn creates_a_private_engine_home_before_loading_profile() {
    let root = std::env::temp_dir().join(format!("horizon-ios-engine-home-{}", std::process::id()));
    let home = root.join("Application Support").join("Horizon");
    let _ = std::fs::remove_dir_all(&root);
    let outcome = prepare_engine_directory(
        &home,
        |dir| {
            assert!(dir.is_dir());
            assert_eq!(dir, home);
            Ok("Player00042".to_string())
        },
        |_dir| false,
    )
    .expect("prepare game home");
    assert_eq!(
        outcome,
        EnginePreparation {
            player_name: "Player00042".to_string(),
            resources_ready: false
        }
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn identifies_prepared_resources_without_starting_game() {
    let root =
        std::env::temp_dir().join(format!("horizon-ios-engine-present-{}", std::process::id()));
    let home = root.join("Horizon");
    let _ = std::fs::remove_dir_all(&root);
    let outcome = prepare_engine_directory(
        &home,
        |_dir| Ok("Alex".to_string()),
        |dir| dir == home && Path::new(dir).exists(),
    )
    .unwrap();
    assert!(outcome.resources_ready);
    assert_eq!(outcome.player_name, "Alex");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_relative_paths_and_missing_player_names() {
    assert!(
        prepare_engine_directory(Path::new("relative"), |_| Ok("Alice".into()), |_| false).is_err()
    );
    assert!(
        prepare_engine_directory(
            Path::new("/tmp/horizon-ios-invalid\0path"),
            |_| Ok("Alice".into()),
            |_| false
        )
        .is_err()
    );
    assert!(
        prepare_engine_directory(
            Path::new("/tmp/horizon-ios-invalid"),
            |_| Ok("".into()),
            |_| false
        )
        .is_err()
    );
}

#[test]
fn core_registry_load_gate_rejects_parallel_work_and_allows_retry() {
    let gate = horizon_ios::CoreLoadGate::default();
    assert_eq!(gate.state(), 0);
    assert!(gate.try_begin());
    assert!(
        !gate.try_begin(),
        "only one worker may initialize the registry"
    );
    assert_eq!(gate.state(), 1);
    gate.finish(false);
    assert_eq!(gate.state(), 3);
    assert!(gate.try_begin(), "failed resource data can be retried");
    gate.finish(true);
    assert_eq!(gate.state(), 2);
    assert!(
        !gate.try_begin(),
        "successful initialization must not leak another registry"
    );
}
