//! Reusable first CPU stage of Horizon's native game loader.
//!
//! The desktop/Android `loading_app` and iOS both read the same registry and
//! game data. This does not create a Winit event loop, a renderer or a world.
use crate::{Result, assets::Assets};
use std::path::Path;

/// Read-only metadata available before an iOS client has a gameplay loop.
/// A completed inspection is not permission to start gameplay or modify saves.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldInspection {
    pub folder: String,
    pub name: String,
    pub game_mode: i32,
    pub seed: i64,
    pub dimension: String,
    /// The last saved player feet position (not a safe respawn guarantee).
    pub saved_position: Option<[f64; 3]>,
    /// Yaw and pitch paired with that position; absent for unspawned players.
    pub saved_rotation: Option<[f32; 2]>,
    /// The first chunk to request once a real streaming world exists.
    /// This does not read, generate, load or render chunks.
    pub initial_chunk: Option<[i32; 2]>,
    /// Read-only first chunk probe; absent when there is no saved position or
    /// when a non-overworld dimension needs its own storage mapping.
    pub initial_chunk_disk: Option<InitialChunkDiskState>,
}

/// Preflight of a single saved Anvil chunk. A saved NBT record does not mean
/// that its block palette is decoded, meshed, or displayed on the GPU.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InitialChunkDiskState {
    Missing,
    Saved {
        data_version: Option<i32>,
        sections: usize,
        block_below_player: Option<String>,
    },
}

/// Read the first existing chunk using the game's read-only Anvil decoder.
/// Never generate missing regions, acquire a session lock or touch save data.
/// Reject symlinks in the region path, including external `.mcc` payloads.
pub fn inspect_saved_initial_chunk(
    world: &Path,
    chunk: [i32; 2],
    player_position: Option<[f64; 3]>,
) -> Result<InitialChunkDiskState> {
    use horizon_core::position::{BlockPos, ChunkPos};
    use horizon_world::{
        nbt::{Limits, Tag},
        region::RegionFile,
    };
    use std::{fs, io::ErrorKind};

    let directory = world.join("region");
    match fs::symlink_metadata(&directory) {
        Ok(meta) if !meta.is_dir() || meta.file_type().is_symlink() => {
            return Err("Invalid region directory".into());
        }
        Err(err) if err.kind() == ErrorKind::NotFound => {
            return Ok(InitialChunkDiskState::Missing);
        }
        Err(err) => return Err(err.into()),
        _ => {}
    }
    let [x, z] = chunk;
    let path = directory.join(format!("r.{}.{}.mca", x >> 5, z >> 5));
    match fs::symlink_metadata(&path) {
        Ok(meta) if !meta.is_file() || meta.file_type().is_symlink() => {
            return Err("Invalid or linked Anvil region file".into());
        }
        Err(err) if err.kind() == ErrorKind::NotFound => {
            return Ok(InitialChunkDiskState::Missing);
        }
        Err(err) => return Err(err.into()),
        _ => {}
    }
    // Large chunks may keep an external payload in `c.<x>.<z>.mcc`.
    // The region codec opens this file when the chunk header points at it.
    let external = directory.join(format!("c.{x}.{z}.mcc"));
    match fs::symlink_metadata(&external) {
        Ok(meta) if !meta.is_file() || meta.file_type().is_symlink() => {
            return Err("Invalid or linked external chunk payload".into());
        }
        Err(err) if err.kind() == ErrorKind::NotFound => {}
        Err(err) => return Err(err.into()),
        _ => {}
    }
    let mut region = RegionFile::open(path)?;
    let Some(record) = region.read(ChunkPos::new(x, z))? else {
        return Ok(InitialChunkDiskState::Missing);
    };
    let decoded = record.nbt(Limits::saved())?;
    // Validate the complete section structure with the shared Horizon world
    // decoder, not an iOS-specific palette parser. It also rejects old
    // DataVersions and duplicate or malformed section coordinates.
    let decoded = horizon_world::chunk::ChunkData::new(decoded)?;
    if decoded.position() != ChunkPos::new(x, z) {
        return Err("Anvil chunk has mismatched coordinates".into());
    }
    let root = &decoded.root().tag;
    let data_version = match root.get("DataVersion") {
        Some(Tag::Int(n)) => Some(*n),
        _ => None,
    };
    let sections = match root.get("sections") {
        Some(Tag::List(sections)) => sections.len(),
        _ => unreachable!("ChunkData validated sections"),
    };
    // Sample the block immediately below the saved player's feet, but only
    // when that position belongs to the first chunk being inspected. This
    // doesn't require loading the game registry or instantiating a world.
    let block_below_player = player_position
        .filter(|p| staged_player_chunk(Some(*p)) == Some(chunk))
        .map(|[px, py, pz]| {
            let pos = BlockPos::new(px.floor() as i32, py.floor() as i32 - 1, pz.floor() as i32);
            decoded.block(pos).map(|state| state.map(|s| s.name))
        })
        .transpose()?
        .flatten();
    Ok(InitialChunkDiskState::Saved {
        data_version,
        sections,
        block_below_player,
    })
}

/// The native game first tries its last saved player position before finding
/// a valid spawn. Only stage finite, in-bounds coordinates for future chunk
/// streaming; never invent a valid spawn from untrusted level data.
pub fn staged_player_chunk(position: Option<[f64; 3]>) -> Option<[i32; 2]> {
    let [x, y, z] = position?;
    if !x.is_finite()
        || !y.is_finite()
        || !z.is_finite()
        || x.abs() > 30_000_000.0
        || z.abs() > 30_000_000.0
        || y.abs() > 20_000.0
    {
        return None;
    }
    Some([(x.floor() as i32) >> 4, (z.floor() as i32) >> 4])
}

/// Mirror the read/compatibility stages of the desktop's existing-world loader
/// without taking a session lock, performing file fixes, or writing to disk.
/// The iOS shell calls this on a worker thread, never on UIKit's event loop.
pub fn inspect_existing_world(saves: &Path, folder: &str) -> Result<WorldInspection> {
    use std::path::Component;

    if !matches!(
        &std::path::Path::new(folder)
            .components()
            .collect::<Vec<_>>()[..],
        [Component::Normal(_)]
    ) {
        return Err("World must be a single folder beneath saves".into());
    }
    let base = std::fs::canonicalize(saves)?;
    let candidate = saves.join(folder);
    if std::fs::symlink_metadata(&candidate)?
        .file_type()
        .is_symlink()
    {
        return Err("World folder cannot be a symbolic link".into());
    }
    let world = std::fs::canonicalize(&candidate)?;
    if world.parent() != Some(base.as_path()) || !world.is_dir() {
        return Err("World is outside the game's saves directory".into());
    }
    if std::fs::symlink_metadata(world.join("level.dat"))?
        .file_type()
        .is_symlink()
    {
        return Err("World level.dat cannot be a symbolic link".into());
    }
    if crate::save::incompatible_version(&world).is_some() {
        return Err("World was saved in an incompatible game series".into());
    }
    if crate::save::needs_file_fixes(&world) {
        return Err("World requires an upgrade and backup before opening".into());
    }
    if crate::save::downgraded_from(&world).is_some() {
        return Err("World was saved in a newer version".into());
    }
    let level =
        crate::save::Level::read_summary(&world).ok_or("Cannot read the world's level.dat")?;
    let game_mode = level.mode();
    let saved_position = level
        .position
        .filter(|p| staged_player_chunk(Some(*p)).is_some());
    let saved_rotation = saved_position.map(|_| level.rotation);
    let initial_chunk = staged_player_chunk(saved_position);
    // Only the vanilla overworld layout is supported by the first disk probe.
    // Other dimensions keep their metadata but await their storage mapping.
    let initial_chunk_disk = if level.dimension == "minecraft:overworld" {
        initial_chunk
            .map(|chunk| inspect_saved_initial_chunk(&world, chunk, saved_position))
            .transpose()?
    } else {
        None
    };
    Ok(WorldInspection {
        folder: folder.to_owned(),
        name: level.name,
        game_mode,
        seed: level.seed,
        dimension: level.dimension,
        saved_position,
        saved_rotation,
        initial_chunk,
        initial_chunk_disk,
    })
}

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

    /// Synthetic Anvil region for tests only; never touches player saves.
    fn write_stone_region(root: &Path) -> std::path::PathBuf {
        use horizon_core::position::ChunkPos;
        use horizon_world::{
            compression::Compression,
            nbt::{Limits, NamedTag, Tag},
            region::{ChunkRecord, RegionFile},
        };
        let region_dir = root.join("region");
        fs::create_dir_all(&region_dir).unwrap();
        let file = region_dir.join("r.0.-1.mca");
        let chunk = ChunkPos::new(8, -5);
        let record = ChunkRecord::from_nbt(
            &NamedTag {
                name: "".into(),
                tag: Tag::compound([
                    ("xPos", Tag::Int(8)),
                    ("zPos", Tag::Int(-5)),
                    ("DataVersion", Tag::Int(horizon_world::DATA_VERSION)),
                    (
                        "sections",
                        Tag::List(vec![Tag::compound([
                            ("Y", Tag::Byte(4)),
                            (
                                "block_states",
                                Tag::compound([(
                                    "palette",
                                    Tag::List(vec![Tag::compound([(
                                        "id",
                                        Tag::String("minecraft:stone".into()),
                                    )])]),
                                )]),
                            ),
                        ])]),
                    ),
                ]),
            },
            Compression::Zlib,
            0,
            Limits::default(),
        )
        .unwrap();
        {
            let mut region = RegionFile::open_writable(&file, true).unwrap();
            region.write(chunk, &record).unwrap();
        }
        file
    }

    #[test]
    fn ios_world_inspection_reads_valid_level_data_without_creating_a_lock() {
        let root =
            std::env::temp_dir().join(format!("horizon-ios-inspect-{}-ok", std::process::id()));
        let saves = root.join("saves");
        let world = saves.join("Test");
        fs::create_dir_all(&world).unwrap();
        let source = world.join("level.dat");
        crate::save::write_dat(
            &source,
            horizon_world::nbt::Tag::compound([(
                "Data",
                horizon_world::nbt::Tag::compound([
                    (
                        "LevelName",
                        horizon_world::nbt::Tag::String("My World".into()),
                    ),
                    (
                        "DataVersion",
                        horizon_world::nbt::Tag::Int(horizon_world::DATA_VERSION),
                    ),
                    ("GameType", horizon_world::nbt::Tag::Int(1)),
                    (
                        "Player",
                        horizon_world::nbt::Tag::compound([
                            (
                                "Pos",
                                horizon_world::nbt::Tag::List(
                                    [128.5_f64, 72.0, -64.25]
                                        .into_iter()
                                        .map(|v| horizon_world::nbt::Tag::Double(v.to_bits()))
                                        .collect(),
                                ),
                            ),
                            (
                                "Rotation",
                                horizon_world::nbt::Tag::List(
                                    [45.0_f64, -15.0]
                                        .into_iter()
                                        .map(|v| horizon_world::nbt::Tag::Double(v.to_bits()))
                                        .collect(),
                                ),
                            ),
                        ]),
                    ),
                    (
                        "WorldGenSettings",
                        horizon_world::nbt::Tag::compound([(
                            "seed",
                            horizon_world::nbt::Tag::Long(12345),
                        )]),
                    ),
                    (
                        "Version",
                        horizon_world::nbt::Tag::compound([(
                            "Name",
                            horizon_world::nbt::Tag::String("26.3".into()),
                        )]),
                    ),
                ]),
            )]),
        )
        .unwrap();
        let region_path = write_stone_region(&world);
        let original_region = fs::read(&region_path).unwrap();
        let original = fs::read(&source).unwrap();
        let inspected = inspect_existing_world(&saves, "Test").unwrap();
        assert_eq!(inspected.folder, "Test");
        assert_eq!(inspected.name, "My World");
        assert_eq!(inspected.game_mode, 1);
        assert_eq!(inspected.seed, 12345);
        assert_eq!(inspected.saved_position, Some([128.5, 72.0, -64.25]));
        assert_eq!(inspected.saved_rotation, Some([45.0, -15.0]));
        assert_eq!(inspected.initial_chunk, Some([8, -5]));
        assert_eq!(
            inspected.initial_chunk_disk,
            Some(InitialChunkDiskState::Saved {
                data_version: Some(horizon_world::DATA_VERSION),
                sections: 1,
                block_below_player: Some("minecraft:stone".to_owned()),
            })
        );
        assert_eq!(original, fs::read(&source).unwrap());
        assert_eq!(original_region, fs::read(&region_path).unwrap());
        assert!(!world.join("session.lock").exists());
        #[cfg(unix)]
        {
            let external = root.join("external-level.dat");
            fs::rename(&source, &external).unwrap();
            std::os::unix::fs::symlink(&external, &source).unwrap();
            assert!(
                inspect_existing_world(&saves, "Test").is_err(),
                "a symlinked level.dat may escape the app's world directory"
            );
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ios_preflight_reads_saved_anvil_chunk_without_modifying_any_file() {
        let root = std::env::temp_dir().join(format!("horizon-ios-anvil-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let file = write_stone_region(&root);
        let original = fs::read(&file).unwrap();
        assert_eq!(
            inspect_saved_initial_chunk(&root, [8, -5], Some([128.5, 72.0, -64.25])).unwrap(),
            InitialChunkDiskState::Saved {
                data_version: Some(horizon_world::DATA_VERSION),
                sections: 1,
                block_below_player: Some("minecraft:stone".to_owned()),
            }
        );
        // Never sample a position from a different chunk into this palette.
        assert_eq!(
            inspect_saved_initial_chunk(&root, [8, -5], Some([144.0, 72.0, -64.25])).unwrap(),
            InitialChunkDiskState::Saved {
                data_version: Some(horizon_world::DATA_VERSION),
                sections: 1,
                block_below_player: None,
            }
        );
        assert_eq!(
            inspect_saved_initial_chunk(&root, [9, -5], None).unwrap(),
            InitialChunkDiskState::Missing
        );
        assert_eq!(fs::read(&file).unwrap(), original);
        assert!(!root.join("session.lock").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ios_preflight_rejects_symlinked_and_corrupt_region_files() {
        let root =
            std::env::temp_dir().join(format!("horizon-ios-anvil-bad-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("region")).unwrap();
        assert_eq!(
            inspect_saved_initial_chunk(&root, [0, 0], None).unwrap(),
            InitialChunkDiskState::Missing
        );
        fs::write(root.join("region/r.0.0.mca"), b"bad").unwrap();
        // Anvil reader treats a truncated header as an empty (nonexistent) chunk.
        assert_eq!(
            inspect_saved_initial_chunk(&root, [0, 0], None).unwrap(),
            InitialChunkDiskState::Missing
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            fs::remove_file(root.join("region/r.0.0.mca")).unwrap();
            let external = root.join("outside.mca");
            fs::write(&external, b"bad").unwrap();
            symlink(&external, root.join("region/r.0.0.mca")).unwrap();
            assert!(inspect_saved_initial_chunk(&root, [0, 0], None).is_err());
            fs::remove_dir_all(root.join("region")).unwrap();
            symlink("../..", root.join("region")).unwrap();
            assert!(inspect_saved_initial_chunk(&root, [0, 0], None).is_err());
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ios_world_player_start_location_ignores_invalid_coordinates() {
        let position = |coords| staged_player_chunk(Some(coords));
        assert_eq!(position([0.0, 70.0, -0.01]), Some([0, -1]));
        assert_eq!(position([31.99, 65.0, 48.0]), Some([1, 3]));
        assert_eq!(staged_player_chunk(None), None);
        assert_eq!(position([f64::NAN, 64.0, 0.0]), None);
        assert_eq!(position([0.0, f64::INFINITY, 0.0]), None);
        assert_eq!(position([50_000_000.0, 64.0, 0.0]), None);
    }

    #[test]
    fn ios_world_inspection_rejects_traversal_missing_and_invalid_worlds() {
        let root =
            std::env::temp_dir().join(format!("horizon-ios-inspect-{}-bad", std::process::id()));
        let saves = root.join("saves");
        fs::create_dir_all(saves.join("Unreadable")).unwrap();
        for folder in ["../escape", ".", "", "Unreadable", "Missing", "/tmp"] {
            assert!(
                inspect_existing_world(&saves, folder).is_err(),
                "unexpectedly accepted {folder:?}"
            );
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&root, saves.join("Linked")).unwrap();
            assert!(inspect_existing_world(&saves, "Linked").is_err());
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mobile_preview_accepts_only_safe_title_navigation() {
        let root =
            std::env::temp_dir().join(format!("horizon-ios-title-tap-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let mut title = crate::title::Title::new(root.join("saves"), &root);
        let size = (500.0, 350.0);
        assert!(
            !title.preview_tap(300.0, 241.0, size),
            "Quit must not leave UIKit"
        );
        assert!(matches!(title.mode, crate::title::Mode::Main));
        assert!(title.preview_tap(250.0, 145.0, size));
        assert!(matches!(title.mode, crate::title::Mode::Create { .. }));
        assert!(
            !title.preview_tap(174.0, 333.0, size),
            "world creation must remain disabled"
        );
        assert!(title.preview_tap(328.0, 333.0, size));
        assert!(matches!(title.mode, crate::title::Mode::Main));
        // World selection Back reuses the native hit targets.
        title.mode = crate::title::Mode::Select {
            worlds: Vec::new(),
            selected: None,
        };
        assert!(
            !title.preview_tap(175.0, 309.0, size),
            "Play must not start a world"
        );
        assert!(title.preview_tap(372.0, 332.0, size));
        assert!(matches!(title.mode, crate::title::Mode::Main));
        fs::remove_dir_all(root).unwrap();
    }

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
