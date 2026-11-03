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

use crate::{
    gfx::{Frame, Renderer},
    hud::HudBatch,
};
use std::sync::Arc;
use winit::window::Window;

/// Construct the same lightweight boot renderer used by the native
/// `loading_app`. The caller owns the Winit window and event loop; this does
/// not create either, and it does not start a Minecraft world.
pub fn loading_renderer(
    window: Arc<Window>,
    offscreen: bool,
    atlas: Option<&crate::hud::HudAtlas>,
    panorama: Option<&crate::gfx::panorama::Faces>,
    previous: Option<&Renderer>,
    api: crate::graphics::GraphicsApi,
) -> Result<Renderer> {
    static FORCEFIELD: [u8; 4] = [0; 4];
    static EMPTY_HUD: [u8; 4] = [0, 0, 0, 255];
    static EMPTY_GLINT: [u8; 4] = [0; 4];
    let empty_panorama = crate::gfx::panorama::Faces {
        size: 1,
        pixels: [0u8; 4].repeat(6),
    };
    let panorama = panorama.unwrap_or(&empty_panorama);
    let (pixels, width, height) = atlas.map_or((EMPTY_HUD.as_slice(), 1, 1), |a| {
        (a.pixels.as_slice(), a.width, a.height)
    });
    let (glint_pixels, glint_width, glint_height) = atlas
        .map_or((EMPTY_GLINT.as_slice(), 1, 1), |a| {
            (a.glint.0.as_slice(), a.glint.1, a.glint.2)
        });
    // A glyph rasterized into an earlier texture is not in these pixels; the
    // new texture gets it again when the character is drawn (see
    // `build_renderer`).
    if let Some(atlas) = atlas {
        atlas.font.reset_cache();
    }
    let mut renderer = pollster::block_on(Renderer::new(
        window,
        offscreen,
        &[255, 255, 255, 255],
        1,
        1,
        1,
        &[],
        &[],
        (pixels, width, height),
        (glint_pixels, glint_width, glint_height),
        (glint_pixels, glint_width, glint_height),
        (&FORCEFIELD[..], 1),
        panorama,
        None,
        previous,
        api,
    ))?;
    if let Some(atlas) = atlas {
        renderer.hud.sprite_animations = atlas
            .sprite_animations
            .iter()
            .map(|(rect, animation)| (*rect, animation.clone(), (usize::MAX, 0, 0)))
            .collect();
    }
    Ok(renderer)
}

/// Same empty-world title/loading frame used by the native client, with a
/// caller-owned HUD batch so it can be rendered from a platform host.
pub fn loading_frame(hud: &HudBatch) -> Frame<'_> {
    Frame {
        weather: &[],
        weather_time: crate::weather::Time::new(0, 0.0),
        camera: glam::DVec3::new(0.5, 100.0, 0.5),
        yaw: 0.0,
        pitch: 10.0,
        fov_degrees: 85.0,
        eye: glam::DVec3::new(0.5, 100.0, 0.5),
        eye_rotation: [0.0, 10.0],
        first_person: true,
        bob: glam::Mat4::IDENTITY,
        spin: glam::Mat4::IDENTITY,
        sky: crate::environment::Sky::default(),
        shadow_scene_revision: (0, 0),
        shadow_cache: false,
        distance: 12.0,
        detail_distance: 12.0,
        depth_far: 1000.0,
        underwater: false,
        water_plane: None,
        cave_culling: false,
        title: true,
        sky_blocked: false,
        skybox: false,
        end_sky: false,
        end_sky_sprite: None,
        environment_fog: None,
        water_fog: [0.0; 3],
        fog: None,
        dark_disc: false,
        outline: Vec::new(),
        outline_after_terrain: false,
        outline_high_contrast: false,
        debug_lines: &[],
        debug_lines_on_top: &[],
        debug_crosshair: &[],
        hud,
        glint_alpha: 0.75,
        glint_ticks: 0.0,
        nether_lighting: false,
        entities: &[],
        entity_spans: &[],
        glowing: &[],
        dynamic_lights: &[],
        colored_lights: &[],
        emissive_layers: &[],
        clouds: None,
        lightning: Default::default(),
        additive: &[],
        see_through: &[],
        shadow_only: &[],
        celestial: &[],
        stars: None,
        hand: &[],
        hand_fov_degrees: 70.0,
        screen: &[],
        pack: Default::default(),
        border: None,
        game_time: 0.0,
        shader_game_time: 0.0,
        end_portal_sprites: [[0.0; 4]; 2],
        crumbling_sprite: [0.0; 4],
        ticks: 0,
        darkness: (0.0, 0.0),
        night_vision: 0.0,
        block_flicker: 0.0,
        boss_darken: 0.0,
        chunk_fade: 0.0,
        post_effects: crate::post_chain::Request::none(),
        menu_blur: None,
    }
}

/// Draw the same native Minecraft/Horizon title widgets used by the desktop
/// loading menu, then wrap them in the shared renderer's title frame. The
/// platform host still owns all input, window events and world startup.
/// A resource-free diagnostic keeps using `loading_frame` instead.
pub fn loading_title_frame<'a>(
    hud: &'a mut HudBatch,
    title: &crate::title::Title,
    atlas: &crate::hud::HudAtlas,
    rotation: &mut crate::gfx::panorama::Rotation,
    elapsed_seconds: f32,
) -> Frame<'a> {
    title.draw(atlas, hud, (-1.0, -1.0), false);
    // The native loading menu publishes its widgets after each frame too.
    // Without this, the collected buttons stay in a stale per-thread buffer.
    crate::widgets::publish_frame();
    let mut frame = loading_frame(hud);
    frame.yaw = rotation.update(elapsed_seconds, 1.0);
    frame
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, time::SystemTime};

    #[test]
    fn prepared_title_assets_can_cross_the_gpu_worker_boundary() {
        fn assert_send<T: Send>() {}
        assert_send::<crate::hud::HudAtlas>();
        assert_send::<crate::title::Title>();
    }

    #[test]
    fn prepared_title_frame_draws_existing_horizon_buttons_and_rotates_panorama() {
        let root =
            std::env::temp_dir().join(format!("horizon-ios-title-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let title = crate::title::Title::new(root.join("saves"), &root);
        let atlas = crate::hud::HudAtlas::boot();
        let mut hud = HudBatch::new([1000, 700]);
        let mut rotation = crate::gfx::panorama::Rotation::default();
        let frame = loading_title_frame(&mut hud, &title, &atlas, &mut rotation, 2.0);
        assert!(frame.title);
        assert!(
            frame.hud.vertices.len() > 0,
            "the native title must draw HUD geometry"
        );
        assert_eq!(frame.yaw, -4.0, "reuse the native panorama rotation rate");
        assert!(
            !crate::widgets::last().is_empty(),
            "publish the existing title buttons for future native input routing"
        );
        drop(frame);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preview_frame_matches_the_native_game_loading_state() {
        let batch = HudBatch::new([320, 240]);
        let frame = loading_frame(&batch);
        assert!(frame.title);
        assert!(!frame.skybox);
        assert!(frame.entities.is_empty());
        assert!(frame.weather.is_empty());
        assert!(frame.debug_lines.is_empty());
    }

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
