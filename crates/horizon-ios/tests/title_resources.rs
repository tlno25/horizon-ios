use horizon_ios::title_resources_root;
use std::path::Path;

#[test]
fn normal_preview_uses_prepared_resource_directory_when_registry_is_loaded() {
    let home = Path::new("/sandbox/Horizon");
    assert_eq!(
        title_resources_root(Some(home), true, false),
        Some(
            home.join(".local/horizon-client")
                .join(horizon_core::VANILLA_VERSION)
        )
    );
}

#[test]
fn smoke_mode_never_reads_private_minecraft_resources() {
    let home = Path::new("/sandbox/Horizon");
    assert!(title_resources_root(Some(home), true, true).is_none());
    assert!(title_resources_root(None, true, true).is_none());
}

#[test]
fn missing_registry_or_unprepared_home_keeps_lightweight_boot_atlas() {
    let home = Path::new("/sandbox/Horizon");
    assert!(title_resources_root(Some(home), false, false).is_none());
    assert!(title_resources_root(None, true, false).is_none());
}
