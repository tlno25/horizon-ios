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
    /// Optional read-only resident conversion using the prepared block and
    /// biome tables. The actual decoded column is not retained or rendered.
    pub resident_chunk_preview: Option<ResidentChunkPreview>,
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

/// Preview metadata for a native decoded saved column. A worker may retain
/// the in-memory column for CPU meshing, but does not start a running World.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResidentChunkPreview {
    pub chunk: [i32; 2],
    pub resident_sections: usize,
    pub block_state_id: u32,
    pub biome_id: u16,
    pub heap_bytes: usize,
}

/// A read-only prepared world keeps exactly one fully decoded initial column
/// for the future native terrain renderer. No ticks, session lock, save writer
/// or gameplay state has been created.
pub struct PreparedWorld {
    pub inspection: WorldInspection,
    pub initial_column: Option<crate::chunk::Chunk>,
}

impl std::fmt::Debug for PreparedWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedWorld")
            .field("inspection", &self.inspection)
            .field("has_initial_column", &self.initial_column.is_some())
            .finish()
    }
}

/// Compatibility metadata wrapper: callers not retaining the resident column
/// receive only the preview metadata and release the decoded chunk.
pub fn decode_saved_initial_chunk_preview(
    world: &Path,
    chunk: [i32; 2],
    player_position: [f64; 3],
    registry: &horizon_world::registry::BlockRegistry,
    biomes: &crate::biome::Biomes,
) -> Result<Option<ResidentChunkPreview>> {
    Ok(
        load_saved_initial_chunk_preview(world, chunk, player_position, registry, biomes)?
            .map(|(preview, _column)| preview),
    )
}

/// Only compile sections that contain saved, non-air block states. Empty
/// chunks may have many uniform air sections, which the native Mesher can
/// return cheaply but should not allocate GPU buffers for.
pub fn mesh_section_candidates(column: &crate::chunk::Chunk) -> Vec<i32> {
    column
        .sections
        .iter()
        .enumerate()
        .filter(|(_, section)| !section.is_uniform(0))
        .map(|(i, _)| i as i32)
        .collect()
}

/// CPU assets for the same native mesher that desktop and Android use.
/// Loaded only when the player selects a saved world, not at title startup.
/// The prepared texture atlas is also needed by the eventual Metal renderer.
pub struct TerrainMeshingResources {
    pub render: crate::assets::RenderAssets,
    pub tables: crate::mesher::MeshTables,
    /// Canonical block collision shapes, shared with the native gameplay world.
    pub collisions: horizon_world::block_collision::BlockCollisionRegistry<'static>,
    /// Native block friction/jump coefficients, bound once to the prepared registry.
    pub movement: horizon_world::movement_properties::BoundMovementProperties,
}

impl TerrainMeshingResources {
    pub fn load(root: &Path, core: &Assets) -> Result<Self> {
        let render = core.reload(root)?;
        let facts = crate::blocks::BlockFacts::load(root, core.registry)?;
        let visibility = crate::models::load_visibility(root, core.registry)?;
        let tables = crate::mesher::MeshTables::new(core, &render, &facts, visibility)?;
        let collisions = crate::collisions::load(root, core.registry)?;
        let file = horizon_world::registry::BlockTableFile::open(
            &root.join("movement-properties.nbt"),
            core.registry,
        )?;
        let movement = horizon_world::movement_properties::MovementProperties::from_table(
            file.table("movement")?,
        )?
        .bind(&Default::default())?;
        Ok(Self {
            render,
            tables,
            collisions,
            movement,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerrainMeshSummary {
    /// Number of sections that actually contain visible mesh vertices.
    pub sections: usize,
    pub solid_quads: usize,
    pub translucent_quads: usize,
}

/// Count real quads, not padded or uniform-air sections.
pub fn terrain_mesh_summary(
    meshes: &[((i32, i32, i32), crate::mesher::SectionMesh)],
) -> TerrainMeshSummary {
    let mut summary = TerrainMeshSummary::default();
    for (_, mesh) in meshes {
        if !mesh.solid.is_empty() || !mesh.translucent.is_empty() {
            summary.sections += 1;
        }
        summary.solid_quads += mesh.solid.len() / 4;
        summary.translucent_quads += mesh.translucent.len() / 4;
    }
    summary
}

/// Read-only GPU staging for up to 3x3 existing saved columns. Each mesh
/// has its own (chunk x, section y, chunk z) address for Renderer::upload.
/// No generation, ticks, locks or save modification are performed.
pub struct PreparedTerrain {
    pub meshes: Vec<((i32, i32, i32), crate::mesher::SectionMesh)>,
    pub summary: TerrainMeshSummary,
    /// Number of nonempty saved columns included within the vertex budget.
    pub meshed_columns: usize,
    /// Saved columns skipped when the aggregate vertex budget is exhausted.
    pub skipped_columns: usize,
    /// Saved neighbour columns actually found on disk (0..=8).
    pub saved_neighbors: usize,
}

/// One bounded (3x3) read-only neighbourhood for native face occlusion.
/// Missing neighbouring saves remain absent; no generation or writes occur.
/// Every present neighbour is checked with the same Anvil guard as the first.
pub fn load_saved_initial_region(
    world: &Path,
    center: &crate::chunk::Chunk,
    registry: &horizon_world::registry::BlockRegistry,
    biomes: &crate::biome::Biomes,
) -> Result<crate::mesher::Region> {
    let mut region: crate::mesher::Region = std::array::from_fn(|_| std::array::from_fn(|_| None));
    for dz in -1..=1 {
        for dx in -1..=1 {
            let entry = if dx == 0 && dz == 0 {
                Some(center.clone())
            } else {
                let x = center
                    .x
                    .checked_add(dx)
                    .ok_or("Neighbour X coordinate overflow")?;
                let z = center
                    .z
                    .checked_add(dz)
                    .ok_or("Neighbour Z coordinate overflow")?;
                read_existing_saved_column(world, [x, z], registry, biomes)?
            };
            region[(dz + 1) as usize][(dx + 1) as usize] = entry.map(std::sync::Arc::new);
        }
    }
    Ok(region)
}

/// A resident read-only subset of the real native `World`. It shares the
/// exact Arc-backed decoded chunk columns that the native mesher uses; no
/// block edit, entity spawn, tick scheduling or disk save can occur here.
/// This is the base for wiring iOS input to the normal world simulation.
pub struct ReadOnlyWorldStage {
    pub world: crate::world::World,
    saved_columns: usize,
}

impl ReadOnlyWorldStage {
    pub fn from_region(
        region: &crate::mesher::Region,
        seed: i64,
        air: horizon_world::registry::BlockStateId,
    ) -> Result<Self> {
        let center = region[1][1].as_ref().ok_or("Missing native centre chunk")?;
        let mut world = crate::world::World::new(air, seed);
        let mut saved_columns = 0;
        for (z, row) in region.iter().enumerate() {
            for (x, column) in row.iter().enumerate() {
                let Some(column) = column else { continue };
                let expected_x = center
                    .x
                    .checked_add(x as i32 - 1)
                    .ok_or("Chunk X overflow")?;
                let expected_z = center
                    .z
                    .checked_add(z as i32 - 1)
                    .ok_or("Chunk Z overflow")?;
                if column.x != expected_x || column.z != expected_z {
                    return Err("Read-only world chunk coordinates mismatch".into());
                }
                world
                    .chunks
                    .insert((column.x, column.z), std::sync::Arc::clone(column));
                saved_columns += 1;
            }
        }
        Ok(Self {
            world,
            saved_columns,
        })
    }

    #[must_use]
    pub fn saved_columns(&self) -> usize {
        self.saved_columns
    }

    /// Read the same per-block friction table the full motion engine uses.
    /// Sample just below the feet so partial blocks (slabs) are included.
    /// The game runtime additionally tracks exact supporting block positions,
    /// which this bounded read-only preview does not yet store.
    pub fn support_friction(
        &self,
        movement: &horizon_world::movement_properties::BoundMovementProperties,
        feet: [f64; 3],
    ) -> Result<f32> {
        use horizon_core::position::BlockPos;
        if !feet.iter().all(|v| v.is_finite()) {
            return Err("Invalid preview feet position".into());
        }
        let below = BlockPos::new(
            feet[0].floor() as i32,
            (feet[1] - f64::from(horizon_world::movement_support::ON_SUPPORT_OFFSET)).floor()
                as i32,
            feet[2].floor() as i32,
        );
        let friction = movement
            .get(self.world.block(below.x, below.y, below.z))?
            .coefficients
            .friction;
        if !friction.is_finite() || !(0.0..=1.0).contains(&friction) {
            return Err("Invalid native block friction".into());
        }
        Ok(friction)
    }

    /// Use native voxel shapes and `resolve_world_movement`, without changing
    /// the world or saved player pose. Maximum one block of motion per event.
    /// Missing chunks remain absent and the caller additionally clamps to
    /// columns whose meshes reached the GPU.
    pub fn resolve_preview_movement(
        &self,
        binding: &horizon_world::block_collision::BlockCollisionRegistry<'_>,
        from: [f64; 3],
        target: [f64; 3],
    ) -> Result<[f64; 3]> {
        self.resolve_preview_movement_with_step(binding, from, target, 0.0, false)
    }

    /// Native Entity step candidate selection. A maximum 0.6-block step is
    /// permitted only for grounded movement; collision probes that sample
    /// support or gravity continue to use the strict non-stepping variant.
    pub fn resolve_preview_movement_with_step(
        &self,
        binding: &horizon_world::block_collision::BlockCollisionRegistry<'_>,
        from: [f64; 3],
        target: [f64; 3],
        max_step: f32,
        on_ground: bool,
    ) -> Result<[f64; 3]> {
        use horizon_core::geometry::Vec3;
        if !max_step.is_finite() || !(0.0..=0.6).contains(&max_step) {
            return Err("Preview step height outside safe bounds".into());
        }
        let body = preview_player_bounds(from)?;
        if !target.iter().all(|v| v.is_finite()) {
            return Err("Preview movement target is not finite".into());
        }
        let requested = Vec3::new(
            (target[0] - from[0]).clamp(-1.0, 1.0),
            (target[1] - from[1]).clamp(-1.0, 1.0),
            (target[2] - from[2]).clamp(-1.0, 1.0),
        );
        if requested.length_squared() == 0.0 {
            return Ok(from);
        }
        let mut collisions = crate::collisions::SceneCollisions {
            world: &self.world,
            binding,
            subject: None,
        };
        let resolved = collisions.resolve(body, requested, max_step, on_ground)?;
        Ok([
            from[0] + resolved.x,
            from[1] + resolved.y,
            from[2] + resolved.z,
        ])
    }
}

/// Standard standing-player 0.6×1.8 block volume at the saved feet pose.
/// This is a passive collision probe, not a living simulated entity.
pub fn preview_player_bounds(position: [f64; 3]) -> Result<horizon_core::geometry::Aabb> {
    use horizon_core::geometry::{Aabb, Vec3};
    if !position.iter().all(|v| v.is_finite()) {
        return Err("Preview camera has nonfinite coordinates".into());
    }
    let [x, y, z] = position;
    Ok(Aabb::new(
        Vec3::new(x - 0.3, y, z - 0.3),
        Vec3::new(x + 0.3, y + 1.8, z + 0.3),
    ))
}

/// Shared read-only Anvil path for both the centre column and its neighbours.
/// This does not apply data fixes, create regions, or acquire session locks.
fn read_existing_saved_column(
    world: &Path,
    chunk: [i32; 2],
    registry: &horizon_world::registry::BlockRegistry,
    biomes: &crate::biome::Biomes,
) -> Result<Option<crate::chunk::Chunk>> {
    use horizon_core::position::ChunkPos;
    use horizon_world::{nbt::Limits, region::RegionFile};

    let Some(path) = verified_region_path(world, chunk)? else {
        return Ok(None);
    };
    let [cx, cz] = chunk;
    let mut region = RegionFile::open(path)?;
    let Some(record) = region.read(ChunkPos::new(cx, cz))? else {
        return Ok(None);
    };
    let validated = horizon_world::chunk::ChunkData::new(record.nbt(Limits::saved())?)?;
    if validated.position() != ChunkPos::new(cx, cz) {
        return Err("Anvil chunk has mismatched coordinates".into());
    }
    let tag = validated.root();
    let column = crate::save::decode_with(
        &tag,
        registry,
        biomes,
        None,
        crate::chunk::ColumnLayout::VANILLA,
    )
    .ok_or("Horizon could not decode a full saved chunk with the loaded registry")?;
    if [column.x, column.z] != chunk {
        return Err("Resident chunk coordinates do not match the requested chunk".into());
    }
    Ok(Some(column))
}

/// Centre-first ordering keeps at least the saved player's own column
/// visible when the bounded mobile preview runs out of mesh budget.
/// The four adjacent columns precede the diagonals.
pub fn saved_terrain_mesh_targets(region: &crate::mesher::Region) -> Vec<(usize, usize)> {
    const ORDER: [(usize, usize); 9] = [
        (1, 1),
        (1, 0),
        (0, 1),
        (2, 1),
        (1, 2),
        (0, 0),
        (2, 0),
        (0, 2),
        (2, 2),
    ];
    ORDER
        .into_iter()
        .filter(|&(x, z)| region[z][x].is_some())
        .collect()
}

/// Run the game's native CPU mesher over the already validated initial column
/// with vanilla models, lighting, palette IDs and biome tint logic.
/// This performs no GPU operations or save writes and can run on a worker.
pub fn mesh_prepared_column(
    region: &crate::mesher::Region,
    seed: i64,
    core: &Assets,
    biomes: &crate::biome::Biomes,
    resources: &TerrainMeshingResources,
) -> Result<PreparedTerrain> {
    use crate::mesher::Mesher;
    use horizon_core::biome::{BiomeZoom, obfuscate_seed};

    if region[1][1].is_none() {
        return Err("No centre chunk for terrain meshing".into());
    }
    let mut mesher = Mesher {
        nether_shading: false,
        shader_pack_lighting: false,
        assets: core,
        render: &resources.render,
        tables: &resources.tables,
        biomes,
        visibility: resources.tables.visibility.clone(),
        zoom: BiomeZoom::new(obfuscate_seed(seed)),
        blend_radius: 2,
        smooth_lighting: true,
        cutout_leaves: true,
    };
    // The same cap applies to all nine columns combined, not per column.
    const MAX_PREVIEW_VERTICES: usize = 1_048_576;
    let mut meshes = Vec::new();
    let mut emitted_vertices = 0usize;
    let mut meshed_columns = 0;
    let mut skipped_columns = 0;
    let mut exhausted = false;
    for (ix, iz) in saved_terrain_mesh_targets(region) {
        let column = region[iz][ix].as_deref().expect("saved mesh target");
        if exhausted {
            skipped_columns += 1;
            continue;
        }
        // Reframe the stored 3x3 snapshot so each saved column is the native
        // mesher's centre. Off-grid neighbours are absent, never generated.
        let local = std::array::from_fn(|z| {
            std::array::from_fn(|x| {
                let rx = ix as isize + x as isize - 1;
                let rz = iz as isize + z as isize - 1;
                if (0..3).contains(&rx) && (0..3).contains(&rz) {
                    region[rz as usize][rx as usize].as_deref()
                } else {
                    None
                }
            })
        });
        let mut column_meshes = Vec::new();
        let mut column_vertices = 0usize;
        for sy in mesh_section_candidates(column) {
            let mut mesh = mesher.mesh(&local, sy);
            if mesh.solid.is_empty() && mesh.translucent.is_empty() {
                continue;
            }
            column_vertices = column_vertices
                .saturating_add(mesh.solid.len())
                .saturating_add(mesh.translucent.len());
            if emitted_vertices.saturating_add(column_vertices) > MAX_PREVIEW_VERTICES {
                if ix == 1 && iz == 1 {
                    return Err("Saved centre column exceeds the mobile terrain mesh budget".into());
                }
                // Avoid leaving incomplete neighbour columns floating in the
                // world: discard the entire neighbour if it cannot fit.
                exhausted = true;
                skipped_columns += 1;
                column_meshes.clear();
                break;
            }
            mesh.sort_solid();
            column_meshes.push(((column.x, sy, column.z), mesh));
        }
        if !column_meshes.is_empty() {
            emitted_vertices += column_vertices;
            meshed_columns += 1;
            meshes.extend(column_meshes);
        }
    }
    let summary = terrain_mesh_summary(&meshes);
    let saved_neighbors = region
        .iter()
        .flatten()
        .filter(|column| column.is_some())
        .count()
        .saturating_sub(1);
    Ok(PreparedTerrain {
        meshes,
        summary,
        meshed_columns,
        skipped_columns,
        saved_neighbors,
    })
}

/// Construct a resident `Chunk` with the same decoder used for gameplay,
/// then return only bounded diagnostic metadata. The Anvil path has already
/// been verified by `inspect_saved_initial_chunk` below, preventing linked
/// region and external payload files from being silently followed.
pub fn load_saved_initial_chunk_preview(
    world: &Path,
    chunk: [i32; 2],
    player_position: [f64; 3],
    registry: &horizon_world::registry::BlockRegistry,
    biomes: &crate::biome::Biomes,
) -> Result<Option<(ResidentChunkPreview, crate::chunk::Chunk)>> {
    if staged_player_chunk(Some(player_position)) != Some(chunk) {
        return Err("Saved player position does not belong to the requested chunk".into());
    }
    let Some(column) = read_existing_saved_column(world, chunk, registry, biomes)? else {
        return Ok(None);
    };
    let [x, y, z] = player_position;
    let feet_y = y.floor() as i32 - 1;
    if !(column.min_y()..column.max_y()).contains(&feet_y) {
        return Err("Saved feet are outside the resident column's vertical range".into());
    }
    let x = x.floor() as i32;
    let z = z.floor() as i32;
    Ok(Some((
        ResidentChunkPreview {
            chunk,
            resident_sections: column.sections.len(),
            block_state_id: column.block(x, feet_y, z).0,
            biome_id: column.biome(x, feet_y, z),
            heap_bytes: column.heap_bytes(),
        },
        column,
    )))
}

/// Validate the shared Anvil region path before any reader opens it.
/// Reject symlinks for both region files and oversized external payloads.
fn verified_region_path(world: &Path, chunk: [i32; 2]) -> Result<Option<std::path::PathBuf>> {
    use std::{fs, io::ErrorKind};
    let directory = world.join("region");
    match fs::symlink_metadata(&directory) {
        Ok(meta) if !meta.is_dir() || meta.file_type().is_symlink() => {
            return Err("Invalid region directory".into());
        }
        Err(err) if err.kind() == ErrorKind::NotFound => {
            return Ok(None);
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
            return Ok(None);
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
    Ok(Some(path))
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
    let Some(path) = verified_region_path(world, chunk)? else {
        return Ok(InitialChunkDiskState::Missing);
    };
    let [x, z] = chunk;
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
    let saved_rotation = saved_position.map(|_| safe_preview_rotation(level.rotation));
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
        resident_chunk_preview: None,
    })
}

/// Extend a verified preflight with one resident column for a future terrain
/// pipeline. Existing save/region compatibility checks run before decoding.
pub fn prepare_existing_world_with_registry(
    saves: &Path,
    folder: &str,
    registry: &horizon_world::registry::BlockRegistry,
    biomes: &crate::biome::Biomes,
) -> Result<PreparedWorld> {
    let mut inspection = inspect_existing_world(saves, folder)?;
    let mut initial_column = None;
    if inspection.dimension == "minecraft:overworld"
        && matches!(
            inspection.initial_chunk_disk,
            Some(InitialChunkDiskState::Saved { .. })
        )
        && let (Some(chunk), Some(position)) = (inspection.initial_chunk, inspection.saved_position)
        && let Some((summary, column)) = load_saved_initial_chunk_preview(
            &saves.join(folder),
            chunk,
            position,
            registry,
            biomes,
        )?
    {
        inspection.resident_chunk_preview = Some(summary);
        initial_column = Some(column);
    }
    Ok(PreparedWorld {
        inspection,
        initial_column,
    })
}

/// Read-only metadata compatibility API: decode and immediately release the
/// in-memory column (the iOS shell uses `prepare_existing_world_with_registry`).
pub fn inspect_existing_world_with_registry(
    saves: &Path,
    folder: &str,
    registry: &horizon_world::registry::BlockRegistry,
    biomes: &crate::biome::Biomes,
) -> Result<WorldInspection> {
    Ok(prepare_existing_world_with_registry(saves, folder, registry, biomes)?.inspection)
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
    loading_renderer_with_sprites(
        window, offscreen, atlas, panorama, previous, api, None, None,
    )
}

/// Native preview renderer with the same prepared block sprite arrays the
/// desktop game uses. Replaces the placeholder one-pixel block texture;
/// it must be created on a worker before uploading or drawing terrain meshes.
pub fn loading_terrain_renderer(
    window: Arc<Window>,
    offscreen: bool,
    atlas: Option<&crate::hud::HudAtlas>,
    panorama: Option<&crate::gfx::panorama::Faces>,
    previous: Option<&Renderer>,
    api: crate::graphics::GraphicsApi,
    terrain: &crate::assets::RenderAssets,
) -> Result<Renderer> {
    loading_renderer_with_sprites(
        window,
        offscreen,
        atlas,
        panorama,
        previous,
        api,
        Some(terrain),
        None,
    )
}

/// Prepare a textured replacement while the old title renderer continues to
/// present frames on the same device. No Winit surface is borrowed or moved
/// across threads; UIKit transfers the surface after the worker finishes.
pub fn loading_terrain_renderer_with_context(
    window: Arc<Window>,
    terrain: &crate::assets::RenderAssets,
    context: crate::gfx::ReplacementContext,
) -> Result<Renderer> {
    loading_renderer_with_sprites(
        window,
        true,
        None,
        None,
        None,
        crate::graphics::GraphicsApi::Auto,
        Some(terrain),
        Some(context),
    )
}

fn loading_renderer_with_sprites(
    window: Arc<Window>,
    offscreen: bool,
    atlas: Option<&crate::hud::HudAtlas>,
    panorama: Option<&crate::gfx::panorama::Faces>,
    previous: Option<&Renderer>,
    api: crate::graphics::GraphicsApi,
    terrain: Option<&crate::assets::RenderAssets>,
    background: Option<crate::gfx::ReplacementContext>,
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
    let (sprite_pixels, sprite_size, sprite_minimum_size, layers, sprite_mips, animations) =
        match terrain {
            Some(t) => (
                t.pixels.as_slice(),
                t.sprite_size,
                t.sprite_minimum_size,
                t.texture_layers,
                t.sprite_mips.as_slice(),
                t.animations.as_slice(),
            ),
            None => (
                &[255, 255, 255, 255][..],
                1,
                1,
                1,
                &[][..] as &[crate::mipmap::Settings],
                &[][..] as &[crate::sprites::SpriteAnimation],
            ),
        };
    let mut renderer = if let Some(context) = background {
        pollster::block_on(Renderer::new_with_context(
            window,
            offscreen,
            sprite_pixels,
            sprite_size,
            sprite_minimum_size,
            layers,
            sprite_mips,
            animations,
            (pixels, width, height),
            (glint_pixels, glint_width, glint_height),
            (glint_pixels, glint_width, glint_height),
            (&FORCEFIELD[..], 1),
            panorama,
            None,
            Some(context),
            api,
        ))?
    } else {
        pollster::block_on(Renderer::new(
            window,
            offscreen,
            sprite_pixels,
            sprite_size,
            sprite_minimum_size,
            layers,
            sprite_mips,
            animations,
            (pixels, width, height),
            (glint_pixels, glint_width, glint_height),
            (glint_pixels, glint_width, glint_height),
            (&FORCEFIELD[..], 1),
            panorama,
            None,
            previous,
            api,
        ))?
    };
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

/// A saved world can contain nonfinite camera angles. Do not send NaN to
/// WGPU's view matrix, and clamp pitch to the vanilla first-person range.
pub fn safe_preview_rotation(rotation: [f32; 2]) -> [f32; 2] {
    if !rotation[0].is_finite() || !rotation[1].is_finite() {
        [0.0, 0.0]
    } else {
        [rotation[0], rotation[1].clamp(-90.0, 90.0)]
    }
}

/// Upload sections after `loading_terrain_renderer` created a GPU atlas from
/// the same `TerrainMeshingResources`. This must run on Winit/UIKit's thread
/// while the game's display surface is valid. Only saved static terrain is
/// staged: no world ticks or save modifications take place.
pub fn upload_prepared_terrain(renderer: &mut Renderer, prepared: &PreparedTerrain) {
    for (key, mesh) in &prepared.meshes {
        renderer.upload(*key, mesh);
    }
}

/// The smallest native world-camera frame the shared GPU renderer can draw.
/// Used only once prepared terrain has been uploaded with real sprite arrays;
/// this is not an active gameplay loop and does not tick or save a world.
pub fn terrain_preview_frame(
    hud: &HudBatch,
    saved_position: [f64; 3],
    rotation: [f32; 2],
) -> Frame<'_> {
    let mut frame = loading_frame(hud);
    // The saved position is the player's feet. First-person view is 1.62 m up.
    let camera = glam::DVec3::new(
        saved_position[0],
        saved_position[1] + 1.62,
        saved_position[2],
    );
    let rotation = safe_preview_rotation(rotation);
    frame.title = false;
    frame.camera = camera;
    frame.eye = camera;
    frame.yaw = rotation[0];
    frame.pitch = rotation[1];
    frame.eye_rotation = rotation;
    frame.skybox = true;
    frame.distance = 96.0;
    frame.detail_distance = 96.0;
    frame.cave_culling = false; // Incomplete saved neighbours need no graph.
    frame
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
                    ("Status", Tag::String("minecraft:full".into())),
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
                            (
                                "biomes",
                                Tag::compound([(
                                    "palette",
                                    Tag::List(vec![Tag::String("minecraft:plains".into())]),
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

    /// Minimal public test resources; no Mojang binaries or private packs.
    fn synthetic_native_tables(
        root: &Path,
    ) -> (horizon_world::registry::BlockRegistry, crate::biome::Biomes) {
        use horizon_world::nbt::Tag;
        let resource_root = root.join("resource-fixture");
        for name in ["grass", "foliage", "dry_foliage"] {
            let file = resource_root.join(format!("assets/minecraft/textures/colormap/{name}.png"));
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            let mut output = Vec::new();
            {
                let mut encoder = png::Encoder::new(&mut output, 1, 1);
                encoder.set_color(png::ColorType::Rgb);
                encoder.set_depth(png::BitDepth::Eight);
                let mut png = encoder.write_header().unwrap();
                png.write_image_data(&[60, 120, 80]).unwrap();
            }
            fs::write(file, output).unwrap();
        }
        let biome_dir = resource_root.join("data/minecraft/worldgen/biome");
        fs::create_dir_all(&biome_dir).unwrap();
        for biome in ["desert", "plains"] {
            fs::write(
                biome_dir.join(format!("{biome}.json")),
                r#"{"temperature":0.8,"downfall":0.4,"has_precipitation":true}"#,
            )
            .unwrap();
        }
        let biomes = crate::biome::Biomes::load(&resource_root).unwrap();
        assert_eq!(biomes.id("minecraft:plains"), 1);
        let registry = horizon_world::registry::BlockRegistry::from_nbt(&Tag::compound([
            ("DataVersion", Tag::Int(horizon_world::DATA_VERSION)),
            (
                "states",
                Tag::List(vec![
                    Tag::compound([("id", Tag::String("minecraft:air".into()))]),
                    Tag::compound([("id", Tag::String("minecraft:stone".into()))]),
                    Tag::compound([("id", Tag::String("minecraft:stone_slab".into()))]),
                    Tag::compound([("id", Tag::String("minecraft:ice".into()))]),
                ]),
            ),
            (
                "defaults",
                Tag::compound([
                    ("minecraft:air", Tag::Int(0)),
                    ("minecraft:stone", Tag::Int(1)),
                    ("minecraft:stone_slab", Tag::Int(2)),
                    ("minecraft:ice", Tag::Int(3)),
                ]),
            ),
        ]))
        .unwrap();
        (registry, biomes)
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
        let (registry, biomes) = synthetic_native_tables(&root);
        let preview =
            inspect_existing_world_with_registry(&saves, "Test", &registry, &biomes).unwrap();
        assert_eq!(
            preview
                .resident_chunk_preview
                .as_ref()
                .map(|c| c.block_state_id),
            Some(1)
        );
        assert_eq!(
            preview.resident_chunk_preview.as_ref().map(|c| c.biome_id),
            Some(1)
        );
        let prepared =
            prepare_existing_world_with_registry(&saves, "Test", &registry, &biomes).unwrap();
        assert_eq!(
            prepared.inspection.resident_chunk_preview,
            preview.resident_chunk_preview
        );
        // The filesystem worker transports this to the Winit/UIKit thread.
        fn assert_worker_send<T: Send>() {}
        assert_worker_send::<PreparedWorld>();
        let column = prepared
            .initial_column
            .expect("retains decoded native column");
        assert_eq!([column.x, column.z], [8, -5]);
        assert_eq!(column.block(128, 71, -65).0, 1);
        assert_eq!(column.biome(128, 71, -65), 1);
        let region = load_saved_initial_region(&world, &column, &registry, &biomes).unwrap();
        assert!(region[1][1].is_some());
        assert_eq!(region[1][1].as_ref().unwrap().block(128, 71, -65).0, 1);
        assert_eq!(region.iter().flatten().filter(|c| c.is_some()).count(), 1);
        assert!(region[0][0].is_none());
        // The initial column has one non-air section in the synthetic save;
        // the CPU mesher must not scan all 24 empty sections.
        assert_eq!(mesh_section_candidates(&column), vec![8]);
        assert_eq!(
            mesh_section_candidates(&crate::chunk::Chunk::empty(8, -5)),
            Vec::<i32>::new()
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
    fn ios_resident_preview_uses_shared_block_ids_and_biomes_without_writing() {
        // Build a tiny public-domain registry and biome table entirely in a
        // temporary directory; no private game resource files are involved.
        let root =
            std::env::temp_dir().join(format!("horizon-ios-resident-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let region = write_stone_region(&root);
        let (registry, biomes) = synthetic_native_tables(&root);
        let original = fs::read(&region).unwrap();
        let result = decode_saved_initial_chunk_preview(
            &root,
            [8, -5],
            [128.5, 72.0, -64.25],
            &registry,
            &biomes,
        )
        .unwrap()
        .unwrap();
        assert_eq!(result.block_state_id, 1);
        assert_eq!(result.biome_id, 1);
        assert_eq!(result.chunk, [8, -5]);
        assert_eq!(result.resident_sections, 24);
        // Uniform sections can be shared without allocating any heap bytes.
        assert!(result.heap_bytes < 2 * 1024 * 1024);
        assert_eq!(fs::read(&region).unwrap(), original);
        assert!(!root.join("session.lock").exists());
        assert!(
            load_saved_initial_chunk_preview(
                &root,
                [9, -5],
                [144.0, 72.0, -64.25],
                &registry,
                &biomes,
            )
            .unwrap()
            .is_none()
        );
        assert!(
            decode_saved_initial_chunk_preview(
                &root,
                [9, -5],
                [144.0, 72.0, -64.25],
                &registry,
                &biomes
            )
            .unwrap()
            .is_none()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ios_cpu_mesh_neighbourhood_decodes_existing_saves_and_never_writes() {
        use horizon_core::position::ChunkPos;
        use horizon_world::{
            compression::Compression,
            nbt::{Limits, Tag},
            region::{ChunkRecord, RegionFile},
        };
        let root =
            std::env::temp_dir().join(format!("horizon-ios-neighbours-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let file = write_stone_region(&root);
        let mut original_region = RegionFile::open(&file).unwrap();
        let mut neighbor_nbt = original_region
            .read(ChunkPos::new(8, -5))
            .unwrap()
            .unwrap()
            .nbt(Limits::saved())
            .unwrap();
        *neighbor_nbt.tag.get_mut("xPos").unwrap() = Tag::Int(9);
        let neighbor =
            ChunkRecord::from_nbt(&neighbor_nbt, Compression::Zlib, 0, Limits::default()).unwrap();
        RegionFile::open_writable(&file, false)
            .unwrap()
            .write(ChunkPos::new(9, -5), &neighbor)
            .unwrap();
        let before = fs::read(&file).unwrap();
        let (registry, biomes) = synthetic_native_tables(&root);
        let center = read_existing_saved_column(&root, [8, -5], &registry, &biomes)
            .unwrap()
            .unwrap();
        let nearby = load_saved_initial_region(&root, &center, &registry, &biomes).unwrap();
        assert_eq!(nearby.iter().flatten().filter(|c| c.is_some()).count(), 2);
        assert_eq!(nearby[1][1].as_ref().unwrap().block(128, 71, -65).0, 1);
        assert_eq!(nearby[1][2].as_ref().unwrap().block(144, 71, -65).0, 1);
        assert!(nearby[0][1].is_none());
        // Mesh the centre first, then saved orthogonal neighbours. Never
        // include chunks which are absent on disk or outside the 3x3 bounds.
        assert_eq!(saved_terrain_mesh_targets(&nearby), vec![(1, 1), (2, 1)]);
        let mut complete = nearby.clone();
        for row in &mut complete {
            for column in row {
                *column = nearby[1][1].clone();
            }
        }
        assert_eq!(
            saved_terrain_mesh_targets(&complete),
            vec![
                (1, 1),
                (1, 0),
                (0, 1),
                (2, 1),
                (1, 2),
                (0, 0),
                (2, 0),
                (0, 2),
                (2, 2)
            ]
        );
        assert_eq!(before, fs::read(&file).unwrap());
        assert!(!root.join("session.lock").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ios_read_only_preview_uses_native_collision_shapes_to_stop_at_saved_stone() {
        use horizon_core::geometry::{Axis, VoxelShape};
        use horizon_world::{
            block_collision::BlockCollisionRegistry, collision::CachedCollisionRegistry, nbt::Tag,
            registry::BlockStateId,
        };
        let root =
            std::env::temp_dir().join(format!("horizon-ios-collision-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let region_path = write_stone_region(&root);
        let bytes = fs::read(&region_path).unwrap();
        let (registry, biomes) = synthetic_native_tables(&root);
        let centre = read_existing_saved_column(&root, [8, -5], &registry, &biomes)
            .unwrap()
            .unwrap();
        let region = load_saved_initial_region(&root, &centre, &registry, &biomes).unwrap();
        let stage = ReadOnlyWorldStage::from_region(&region, 23, BlockStateId(0)).unwrap();
        // Public, synthetic collision table: canonical empty and solid cube,
        // in the same native NBT wire format the exporter normally writes.
        let make_shape = |shape: VoxelShape, canonical: i8| {
            let axes = shape.cube_coordinate_axes();
            let axes = axes.iter().enumerate().fold(0i8, |mask, (i, enabled)| {
                mask | if *enabled { 1 << i } else { 0 }
            });
            let (minimum, maximum) = shape.discrete_bounds();
            let [xs, ys, zs] = shape.sizes();
            let mut occupancy = Vec::with_capacity(xs * ys * zs);
            for x in 0..xs {
                for y in 0..ys {
                    for z in 0..zs {
                        occupancy.push(u8::from(shape.is_full(x as i32, y as i32, z as i32)));
                    }
                }
            }
            Tag::compound([
                ("canonical", Tag::Byte(canonical)),
                (
                    "x",
                    Tag::LongArray(
                        shape
                            .coordinates(Axis::X)
                            .iter()
                            .map(|n| n.to_bits() as i64)
                            .collect(),
                    ),
                ),
                (
                    "y",
                    Tag::LongArray(
                        shape
                            .coordinates(Axis::Y)
                            .iter()
                            .map(|n| n.to_bits() as i64)
                            .collect(),
                    ),
                ),
                (
                    "z",
                    Tag::LongArray(
                        shape
                            .coordinates(Axis::Z)
                            .iter()
                            .map(|n| n.to_bits() as i64)
                            .collect(),
                    ),
                ),
                ("occupied", Tag::ByteArray(occupancy)),
                ("cube_axes", Tag::Byte(axes)),
                ("cube", Tag::Byte(i8::from(shape.is_cube()))),
                ("discrete_min", Tag::IntArray(minimum.to_vec())),
                ("discrete_max", Tag::IntArray(maximum.to_vec())),
            ])
        };
        let mut state_shapes = vec![0; registry.len()];
        state_shapes[1] = 1;
        state_shapes[2] = 2;
        state_shapes[3] = 1;
        let data = Tag::compound([
            ("DataVersion", Tag::Int(horizon_world::DATA_VERSION)),
            (
                "states",
                Tag::List(
                    (0..registry.len())
                        .map(|i| registry.state(BlockStateId(i as u32)).unwrap().to_nbt())
                        .collect(),
                ),
            ),
            (
                "shapes",
                Tag::List(vec![
                    make_shape(VoxelShape::empty(), 1),
                    make_shape(VoxelShape::block(), 2),
                    make_shape(
                        VoxelShape::from_box(horizon_core::geometry::Aabb::new(
                            horizon_core::geometry::Vec3::ZERO,
                            horizon_core::geometry::Vec3::new(1.0, 0.5, 1.0),
                        ))
                        .unwrap(),
                        0,
                    ),
                ]),
            ),
            ("cached_shapes", Tag::IntArray(state_shapes)),
        ]);
        let cached = CachedCollisionRegistry::from_nbt(&data, &registry).unwrap();
        let collisions = BlockCollisionRegistry::new(std::sync::Arc::new(cached)).unwrap();
        let below = stage
            .resolve_preview_movement(&collisions, [128.5, 80.5, -64.25], [128.5, 79.7, -64.25])
            .unwrap();
        assert!(
            (below[1] - 80.0).abs() < 1.0e-6,
            "stone top must stop feet: {below:?}"
        );
        let side = stage
            .resolve_preview_movement(&collisions, [127.5, 70.0, -64.25], [128.5, 70.0, -64.25])
            .unwrap();
        assert!(
            (side[0] - 127.7).abs() < 1.0e-6,
            "stone wall must stop native player box: {side:?}"
        );
        // Only this synthetic in-memory test snapshot gets a half-height slab
        // and an adjacent full stone wall. The real Anvil region is unchanged.
        let mut steps = stage;
        let column = steps.world.chunks.get_mut(&(8, -5)).unwrap();
        let chunk = std::sync::Arc::make_mut(column);
        chunk.set(138, 80, -66, 2); // slab, top at y=80.5
        chunk.set(140, 80, -66, 1); // full cube, cannot step without jumping
        chunk.set(136, 80, -74, 3); // full ice block, high friction coefficient
        let before = [137.5, 80.0, -65.5];
        let target = [138.5, 80.0, -65.5];
        let blocked = steps
            .resolve_preview_movement(&collisions, before, target)
            .unwrap();
        assert!(blocked[0] < 137.8);
        let stepped = steps
            .resolve_preview_movement_with_step(&collisions, before, target, 0.6, true)
            .unwrap();
        assert!(stepped[0] > 138.4, "slab should be cleared: {stepped:?}");
        assert!((stepped[1] - 80.5).abs() < 1e-6);
        let airborne = steps
            .resolve_preview_movement_with_step(&collisions, before, target, 0.6, false)
            .unwrap();
        assert!(airborne[0] < 137.8, "airborne players must not step up");
        let wall = steps
            .resolve_preview_movement_with_step(
                &collisions,
                [139.4, 80.0, -65.5],
                [140.4, 80.0, -65.5],
                0.6,
                true,
            )
            .unwrap();
        assert!(wall[0] < 139.8, "full-height wall should block: {wall:?}");
        assert!(
            steps
                .resolve_preview_movement_with_step(&collisions, before, target, 0.61, true,)
                .is_err()
        );
        assert!(
            steps
                .resolve_preview_movement_with_step(&collisions, before, target, f32::NAN, true,)
                .is_err()
        );
        let properties = |friction: f32| {
            Tag::compound([
                ("friction", Tag::Float(friction.to_bits())),
                ("speedFactor", Tag::Float(1.0f32.to_bits())),
                ("jumpFactor", Tag::Float(1.0f32.to_bits())),
                ("bounceRestitution", Tag::Float(0.0f32.to_bits())),
                ("fallDistanceReduction", Tag::Float(0.0f32.to_bits())),
                ("fenceGate", Tag::Byte(0)),
            ])
        };
        let material_table = Tag::compound([
            ("DataVersion", Tag::Int(horizon_world::DATA_VERSION)),
            (
                "states",
                Tag::List(
                    (0..registry.len())
                        .map(|i| registry.state(BlockStateId(i as u32)).unwrap().to_nbt())
                        .collect(),
                ),
            ),
            (
                "blocks",
                Tag::compound([
                    ("minecraft:air", properties(0.6)),
                    ("minecraft:stone", properties(0.6)),
                    ("minecraft:stone_slab", properties(0.6)),
                    ("minecraft:ice", properties(0.98)),
                ]),
            ),
        ]);
        let material = horizon_world::movement_properties::MovementProperties::from_nbt(
            &material_table,
            &registry,
        )
        .unwrap()
        .bind(&Default::default())
        .unwrap();
        assert!(
            (steps
                .support_friction(&material, [128.5, 80.0, -64.25])
                .unwrap()
                - 0.6)
                .abs()
                < 1e-6
        );
        assert!(
            (steps
                .support_friction(&material, [136.5, 81.0, -73.5])
                .unwrap()
                - 0.98)
                .abs()
                < 1e-6
        );
        assert!(
            steps
                .support_friction(&material, [f64::NAN, 80.0, 0.0])
                .is_err()
        );
        // Prove the opt-in 20 TPS movement path actually invokes the native
        // step resolver, rather than only the standalone collision probe.
        let mut walker = crate::preview_physics::PreviewPhysics::new([137.3, 80.0, -65.5]);
        for _ in 0..10 {
            walker
                .tick_with_materials(
                    &steps,
                    &collisions,
                    false,
                    [0.0, 1.0],
                    270.0,
                    Some(&material),
                )
                .unwrap();
            if walker.position()[1] > 80.49 {
                break;
            }
        }
        assert!(
            walker.position()[0] > 137.7,
            "walking must climb slab: {:?}",
            walker.position()
        );
        assert!((walker.position()[1] - 80.5).abs() < 1e-6);
        assert!(steps.world.modified.is_empty());
        assert_eq!(fs::read(&region_path).unwrap(), bytes);
        // An opt-in 20 TPS read-only motion probe starts above the same
        // saved stone. Native gravity/voxel collisions land on its top,
        // and the shared jump impulse lifts the camera without save writes.
        let mut motion = crate::preview_physics::PreviewPhysics::new([128.5, 83.0, -64.25]);
        for _ in 0..35 {
            motion.tick(&steps, &collisions, false).unwrap();
        }
        assert!(
            (motion.position()[1] - 80.0).abs() < 1.0e-6,
            "must land on stone: {:?}",
            motion.position()
        );
        assert!(motion.grounded(), "must detect solid ground");
        motion.tick(&steps, &collisions, true).unwrap();
        assert!(
            motion.position()[1] > 80.3,
            "native jump impulse moves upward"
        );
        let first = motion.position()[1];
        motion.tick(&steps, &collisions, false).unwrap();
        assert!(
            motion.position()[1] > first,
            "upward velocity continues on next tick"
        );
        for _ in 0..45 {
            motion.tick(&steps, &collisions, false).unwrap();
        }
        assert!(motion.grounded());
        assert!((motion.position()[1] - 80.0).abs() < 1.0e-6);
        // Walk across a flat real saved stone layer using Horizon's own
        // yaw-based input transform. The player save stays untouched.
        let mut walking = crate::preview_physics::PreviewPhysics::new([132.5, 80.0, -72.5]);
        let start = walking.position();
        for _ in 0..12 {
            walking
                .tick_with_input(&steps, &collisions, false, [0.0, 1.0], 0.0)
                .unwrap();
        }
        assert!(walking.position()[2] > start[2] + 0.7);
        assert!((walking.position()[1] - 80.0).abs() < 1.0e-6);
        let turned = walking.position();
        for _ in 0..12 {
            walking
                .tick_with_input(&steps, &collisions, false, [0.0, 1.0], 90.0)
                .unwrap();
        }
        assert!(walking.position()[0] < turned[0] - 0.7);
        let released = walking.position();
        for _ in 0..25 {
            walking
                .tick_with_input(&steps, &collisions, false, [0.0, 0.0], 90.0)
                .unwrap();
        }
        assert!(
            (walking.position()[0] - released[0]).abs() < 0.6,
            "walking should decelerate when the stick is released"
        );
        assert!(steps.world.modified.is_empty());
        assert_eq!(fs::read(&region_path).unwrap(), bytes);
        assert!(!root.join("session.lock").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ios_native_preview_player_body_uses_saved_feet_with_vanilla_dimensions() {
        let bounds = preview_player_bounds([128.5, 72.0, -64.25]).unwrap();
        assert_eq!(
            bounds.min_position(),
            horizon_core::geometry::Vec3::new(128.2, 72.0, -64.55)
        );
        assert_eq!(
            bounds.max_position(),
            horizon_core::geometry::Vec3::new(128.8, 73.8, -63.95)
        );
        assert!(preview_player_bounds([0.0, f64::INFINITY, 0.0]).is_err());
        assert!(preview_player_bounds([f64::NAN, 64.0, 0.0]).is_err());
    }

    #[test]
    fn ios_read_only_world_stage_retains_native_chunks_without_dirtying_saves() {
        let root =
            std::env::temp_dir().join(format!("horizon-ios-world-stage-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let file = write_stone_region(&root);
        let bytes = fs::read(&file).unwrap();
        let (registry, biomes) = synthetic_native_tables(&root);
        let center = read_existing_saved_column(&root, [8, -5], &registry, &biomes)
            .unwrap()
            .unwrap();
        let region = load_saved_initial_region(&root, &center, &registry, &biomes).unwrap();
        let stage = ReadOnlyWorldStage::from_region(
            &region,
            12345,
            horizon_world::registry::BlockStateId(0),
        )
        .unwrap();
        assert_eq!(stage.saved_columns(), 1);
        assert_eq!(stage.world.chunks.len(), 1);
        assert_eq!(stage.world.block(128, 71, -65).0, 1);
        assert_eq!(stage.world.block(144, 71, -65).0, 0);
        assert_eq!(stage.world.shape.column().min_y(), -64);
        assert!(stage.world.modified.is_empty());
        assert!(stage.world.dirty_sections.is_empty());
        assert!(stage.world.loaded_chunks.is_empty());
        fn assert_send<T: Send>() {}
        assert_send::<ReadOnlyWorldStage>();
        assert_eq!(fs::read(file).unwrap(), bytes);
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
    fn ios_preview_camera_rejects_invalid_rotation_and_pitch() {
        assert_eq!(safe_preview_rotation([f32::NAN, f32::INFINITY]), [0.0, 0.0]);
        assert_eq!(safe_preview_rotation([180.0, 180.0]), [180.0, 90.0]);
        assert_eq!(safe_preview_rotation([-45.0, -130.0]), [-45.0, -90.0]);
    }

    #[test]
    fn ios_terrain_frame_uses_saved_camera_and_is_not_a_title_frame() {
        let hud = crate::hud::HudBatch::new([720, 480]);
        let frame = terrain_preview_frame(&hud, [128.5, 72.0, -64.25], [45.0, -15.0]);
        assert!(!frame.title);
        assert!(frame.skybox);
        assert_eq!(frame.camera, glam::DVec3::new(128.5, 73.62, -64.25));
        assert_eq!(frame.eye, frame.camera);
        assert_eq!(frame.eye_rotation, [45.0, -15.0]);
        assert_eq!(frame.yaw, 45.0);
        assert_eq!(frame.pitch, -15.0);
        assert!(frame.distance >= 64.0);
    }

    #[test]
    fn ios_cpu_terrain_summary_counts_only_emitted_quads() {
        use crate::mesher::{SectionMesh, TerrainVertex};
        let mut one = SectionMesh::default();
        let vertex = TerrainVertex::new([0.0, 0.0, 0.0], [0.0, 0.0], 0, 0, 0);
        one.solid.extend([vertex; 8]);
        one.translucent.extend([vertex; 4]);
        let mut neighbour = SectionMesh::default();
        neighbour.solid.extend([vertex; 4]);
        let meshes = [
            ((8, 8, -5), one),
            ((9, 8, -5), neighbour),
            ((9, 9, -5), SectionMesh::default()),
        ];
        let summary = terrain_mesh_summary(&meshes);
        assert_eq!(meshes[0].0, (8, 8, -5));
        assert_eq!(meshes[1].0, (9, 8, -5));
        assert_eq!(summary.sections, 2);
        assert_eq!(summary.solid_quads, 3);
        assert_eq!(summary.translucent_quads, 1);
        assert_eq!(terrain_mesh_summary(&[]).sections, 0);
        // Rendering resources and CPU meshes move to and from the iOS worker.
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<TerrainMeshingResources>();
        fn assert_send<T: Send>() {}
        assert_send::<PreparedTerrain>();
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
