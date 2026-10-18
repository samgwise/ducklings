//! Level construction: generate a braided maze, materialise it as a tilemap
//! (cells as floor, passages carved through, everything else hedge), spawn
//! the duck, her waiting ducklings and the exit mat — plus the decorative
//! title lettering map shown while the difficulty is chosen. The tile atlas
//! is procedural so the prototype needs no art assets.

use std::collections::HashSet;

use bevy::asset::RenderAssetUsages;
use bevy::image::Image;
use bevy::math::primitives::Circle;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_ecs_tilemap::prelude::*;

use maze::{Direction, Maze};

use crate::bridge;
use crate::duck::{BufferedMove, Duck, PathHistory, WalkPlan};
use crate::ducklings::{Clutch, Duckling, Waiting, follow_step};
use crate::exit;
use crate::fog;
use crate::glyphs::title_lettering;
use crate::level::{Difficulty, GamePhase, RunSeed, level_plan};

/// Size of a single tile in world units (pixels).
pub const TILE_SIZE: f32 = 40.0;

/// Tile size of the title lettering map — smaller than gameplay so the
/// whole banner fits the viewport.
const TITLE_TILE_SIZE: f32 = 20.0;
/// How far the lettering map sits above the screen centre, leaving the
/// lower third for the difficulty buttons.
const TITLE_MAP_LIFT: f32 = 95.0;
/// The lettering map's colours — plain solid tiles: cream corridors on a
/// dark-green hedge, no texture noise, so each character reads cleanly.
const TITLE_LETTER_COLOUR: Color = Color::srgb(0.96, 0.92, 0.84);
const TITLE_HEDGE_COLOUR: Color = Color::srgb(0.20, 0.33, 0.16);

/// Exponential smoothing rate for camera follow.
const CAMERA_SMOOTHING: f32 = 6.0;

/// Atlas columns: cream floor, hedge wall, pink exit mat, four blue water
/// tiles with a flow chevron pointing along the current, and two bark
/// brown fallen logs lying across a horizontal or vertical passage.
const FLOOR_ATLAS_INDEX: u32 = 0;
const WALL_ATLAS_INDEX: u32 = 1;
const EXIT_ATLAS_INDEX: u32 = 2;
const WATER_EAST_ATLAS_INDEX: u32 = 3;
const WATER_WEST_ATLAS_INDEX: u32 = 4;
const WATER_NORTH_ATLAS_INDEX: u32 = 5;
const WATER_SOUTH_ATLAS_INDEX: u32 = 6;
const LOG_HORIZONTAL_ATLAS_INDEX: u32 = 7;
const LOG_VERTICAL_ATLAS_INDEX: u32 = 8;
const DEEP_ATLAS_INDEX: u32 = 9;
const SPOT_ATLAS_INDEX: u32 = 10;
const ATLAS_TILES: u32 = 11;
/// Atlas columns of the title map's plain atlas: letter, hedge.
const LETTER_ATLAS_INDEX: u32 = 0;
const TITLE_HEDGE_ATLAS_INDEX: u32 = 1;

/// The duck's colour — white, with an orange beak child mesh.
const DUCK_COLOUR: Color = Color::srgb(0.99, 0.98, 0.96);
/// The beak's colour.
const BEAK_COLOUR: Color = Color::srgb(0.94, 0.59, 0.24);
/// A duckling's colour — warm yellow.
const DUCKLING_COLOUR: Color = Color::srgb(0.97, 0.79, 0.34);
/// Translucent dark for the drop shadows lifting birds off the floor.
const SHADOW_COLOUR: Color = Color::srgba(0.15, 0.12, 0.08, 0.28);

/// Marks everything that belongs to the current level and despawns with it.
#[derive(Component)]
pub struct LevelEntity;

/// Marks the following camera.
#[derive(Component)]
pub struct MainCamera;

/// The current maze and its points of interest.
#[derive(Resource)]
pub struct MazeRes {
    pub maze: Maze,
    pub start: (u32, u32),
    pub exit: (u32, u32),
}

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Clutch>()
            .add_plugins(TilemapPlugin)
            .add_systems(Startup, (setup, build_title_pond))
            .add_systems(OnExit(GamePhase::Title), build_first_pond)
            .add_systems(Update, camera_follow.run_if(in_state(GamePhase::Playing)));
    }
}

/// Startup: just the camera — the first pond waits for the title screen.
fn setup(mut commands: Commands) {
    commands.spawn((MainCamera, Camera2d, Transform::from_xyz(0.0, 0.0, 1000.0)));
}

/// Leaving the title: sweep the lettering map away and build level 0 from
/// the chosen difficulty.
fn build_first_pond(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    difficulty: Res<Difficulty>,
    seed: Res<RunSeed>,
    existing: Query<Entity, With<LevelEntity>>,
) {
    for entity in existing.iter() {
        commands.entity(entity).despawn();
    }
    let plan = level_plan(0, *difficulty);
    build_level(
        &mut commands,
        &mut images,
        &mut meshes,
        &mut materials,
        &plan,
        seed.0,
    );
}

/// Spawn the decorative "Ooh Ducklings!" lettering map — the title
/// screen's backdrop. Open corridors spell the words; the rest is hedge.
fn build_title_pond(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let ((cols, rows), ink) = title_lettering();
    let ink: HashSet<(u32, u32)> = ink.into_iter().collect();
    let atlas: Handle<Image> = images.add(build_title_atlas(TITLE_TILE_SIZE));
    let tilemap_entity = commands.spawn_empty().id();
    let mut storage = TileStorage::empty(TilemapSize { x: cols, y: rows });
    for ty in 0..rows {
        for tx in 0..cols {
            // The crate renders tile rows bottom-up; the font is indexed
            // top-down — flip the row so the words read upright.
            let kind = if ink.contains(&(tx, rows - 1 - ty)) {
                LETTER_ATLAS_INDEX
            } else {
                TITLE_HEDGE_ATLAS_INDEX
            };
            let pos = TilePos { x: tx, y: ty };
            let entity = commands
                .spawn((
                    LevelEntity,
                    TileBundle {
                        position: pos,
                        texture_index: TileTextureIndex(kind),
                        tilemap_id: TilemapId(tilemap_entity),
                        ..Default::default()
                    },
                ))
                .id();
            storage.set(&pos, entity);
        }
    }
    let tile_size = TilemapTileSize {
        x: TITLE_TILE_SIZE,
        y: TITLE_TILE_SIZE,
    };
    commands.entity(tilemap_entity).insert((
        LevelEntity,
        TilemapBundle {
            grid_size: tile_size.into(),
            size: TilemapSize { x: cols, y: rows },
            storage,
            texture: TilemapTexture::Single(atlas),
            tile_size,
            anchor: TilemapAnchor::Center,
            transform: Transform::from_xyz(0.0, TITLE_MAP_LIFT, 0.0),
            ..Default::default()
        },
    ));
}

/// Smoothly follow the duck.
fn camera_follow(
    time: Res<Time>,
    duck: Single<&Transform, With<Duck>>,
    mut camera: Single<&mut Transform, (With<MainCamera>, Without<Duck>)>,
) {
    let next = follow_step(
        camera.translation.xy(),
        duck.translation.xy(),
        time.delta_secs(),
        CAMERA_SMOOTHING,
    );
    camera.translation = next.extend(camera.translation.z);
}

/// Build a level: maze, tilemap, duck, ducklings, exit. Replaces the maze
/// and clutch resources; every spawned entity carries `LevelEntity`.
pub(crate) fn build_level(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    plan: &crate::level::LevelPlan,
    seed: u64,
) {
    let maze = if plan.bridge {
        Maze::with_channel(plan.width, plan.height, seed, plan.braid, plan.islands)
    } else {
        Maze::with_obstacles(
            plan.width,
            plan.height,
            seed,
            plan.braid,
            plan.streams,
            plan.logs,
        )
    };
    let start = (0u32, 0u32);
    let cells = (plan.width, plan.height);
    // The exit is the farthest cell from the start — on bridge levels, the
    // farthest once every gap is planked, so it lands on the far bank.
    let exit = if plan.bridge {
        bridge::far_exit(&maze, start)
    } else {
        let distances = maze.distances_from(start);
        farthest_cell(&maze, &distances, cells)
    };
    // Ducklings wait at dead ends, farthest first — on bridge levels the
    // chain islands hold them first, since bridging to them is the
    // level's point — padded with the next most distant cells when
    // braiding leaves too few dead ends.
    let mut spots = Vec::new();
    if plan.bridge {
        for island in maze.islands() {
            if maze.gap_sides(island.0, island.1) == 2 {
                spots.push(island);
            }
        }
        spots.truncate(plan.ducklings);
    }
    let bank_spots = duckling_spots(&maze, start, exit, plan.ducklings - spots.len());
    spots.extend(bank_spots);
    spots.truncate(plan.ducklings);

    commands.insert_resource(MazeRes {
        maze: maze.clone(),
        start,
        exit,
    });
    commands.insert_resource(Clutch {
        collected: 0,
        total: spots.len(),
    });
    // Bridge levels carry the plank stock — two planks per chain island —
    // and a fresh planked set.
    let chain_gaps = if plan.bridge { 2 * plan.islands } else { 0 };
    bridge::build_bridge(commands, meshes, materials, chain_gaps);

    // The atlas is procedural so the prototype needs no art assets.
    let atlas: Handle<Image> = images.add(build_atlas(TILE_SIZE));
    let tile_w = plan.width * 2 + 1;
    let tile_h = plan.height * 2 + 1;
    let map_tiles = (tile_w, tile_h);
    let tilemap_entity = commands.spawn_empty().id();
    let mut storage = TileStorage::empty(TilemapSize {
        x: tile_w,
        y: tile_h,
    });
    for ty in 0..tile_h {
        for tx in 0..tile_w {
            let kind = tile_kind(&maze, exit, (tx, ty), map_tiles);
            let pos = TilePos { x: tx, y: ty };
            let entity = commands
                .spawn((
                    LevelEntity,
                    TileBundle {
                        position: pos,
                        texture_index: TileTextureIndex(kind),
                        tilemap_id: TilemapId(tilemap_entity),
                        ..Default::default()
                    },
                ))
                .id();
            storage.set(&pos, entity);
        }
    }
    let tile_size = TilemapTileSize {
        x: TILE_SIZE,
        y: TILE_SIZE,
    };
    commands.entity(tilemap_entity).insert((
        LevelEntity,
        TilemapBundle {
            grid_size: tile_size.into(),
            size: TilemapSize {
                x: tile_w,
                y: tile_h,
            },
            storage,
            texture: TilemapTexture::Single(atlas),
            tile_size,
            anchor: TilemapAnchor::Center,
            ..Default::default()
        },
    ));

    // Fog of war, on the difficulties that play with it: a second tilemap
    // over the level that hides the maze mum has yet to explore.
    fog::build_fog(commands, &mut *images, (tile_w, tile_h), plan.fog);

    // Exit cues: the edge arrow and the mat's pulsing outline.
    exit::build_exit_cues(commands, meshes, materials, exit, cells);

    // One shared drop-shadow mesh and material. Centred under each bird —
    // not offset — so the duck's facing rotation never swings it around,
    // and larger than every body (mum 0.38, ducklings 0.7 × this) so a rim
    // of shade peeks out and lifts the birds off the floor.
    let shadow_mesh = meshes.add(Mesh::from(Circle::new(TILE_SIZE * 0.46)));
    let shadow_material = materials.add(ColorMaterial::from_color(SHADOW_COLOUR));

    // Mum: a white circle with an orange beak, facing east.
    let duck_pos = cell_world_pos(cells, start, TILE_SIZE);
    commands
        .spawn((
            LevelEntity,
            Duck {
                cell: start,
                facing: 0.0,
            },
            PathHistory(vec![start]),
            BufferedMove::default(),
            WalkPlan::default(),
            Mesh2d(meshes.add(Mesh::from(Circle::new(TILE_SIZE * 0.38)))),
            MeshMaterial2d(materials.add(ColorMaterial::from_color(DUCK_COLOUR))),
            Transform::from_xyz(duck_pos.x, duck_pos.y, 1.0),
        ))
        .with_children(|duck| {
            duck.spawn((
                Mesh2d(shadow_mesh.clone()),
                MeshMaterial2d(shadow_material.clone()),
                Transform::from_xyz(0.0, 0.0, -0.1),
            ));
            duck.spawn((
                Mesh2d(meshes.add(Mesh::from(Circle::new(TILE_SIZE * 0.11)))),
                MeshMaterial2d(materials.add(ColorMaterial::from_color(BEAK_COLOUR))),
                Transform::from_xyz(TILE_SIZE * 0.36, 0.0, 0.0),
            ));
        });

    // The waiting clutch.
    for &cell in &spots {
        let pos = cell_world_pos(cells, cell, TILE_SIZE);
        commands
            .spawn((
                LevelEntity,
                Duckling { cell, index: 0 },
                Waiting,
                Mesh2d(meshes.add(Mesh::from(Circle::new(TILE_SIZE * 0.26)))),
                MeshMaterial2d(materials.add(ColorMaterial::from_color(DUCKLING_COLOUR))),
                Transform::from_xyz(pos.x, pos.y, 0.9),
            ))
            .with_children(|duckling| {
                duckling.spawn((
                    Mesh2d(shadow_mesh.clone()),
                    MeshMaterial2d(shadow_material.clone()),
                    Transform::from_xyz(0.0, 0.0, -0.1).with_scale(Vec3::splat(0.7)),
                ));
            });
    }
}

/// The reachable cell with the largest BFS distance (ties keep the first in
/// row-major order). Stream cells are skipped — the exit must be dry, or
/// the duck would ride straight past it.
fn farthest_cell(maze: &Maze, distances: &[Option<u32>], cells: (u32, u32)) -> (u32, u32) {
    let mut best = (0u32, 0u32);
    let mut best_steps = 0u32;
    for (i, steps) in distances.iter().enumerate() {
        let Some(steps) = steps else { continue };
        let cell = (i as u32 % cells.0, i as u32 / cells.0);
        if maze.flow_at(cell.0, cell.1).is_some() {
            continue;
        }
        if *steps > best_steps {
            best = cell;
            best_steps = *steps;
        }
    }
    best
}

/// `count` duckling spots: dead ends first (farthest from the start), then
/// the next most distant ordinary cells. Never the start, the exit, or the
/// water — a duckling on a stream is swept off before it can be found.
fn duckling_spots(
    maze: &Maze,
    start: (u32, u32),
    exit: (u32, u32),
    count: usize,
) -> Vec<(u32, u32)> {
    let distances = maze.distances_from(start);
    let ends = maze.dead_ends();
    let mut ordered: Vec<(u32, (u32, u32))> = distances
        .iter()
        .enumerate()
        .filter_map(|(i, distance)| {
            distance.map(|steps| (steps, (i as u32 % maze.width(), i as u32 / maze.width())))
        })
        .filter(|&(_, cell)| {
            cell != start && cell != exit && maze.flow_at(cell.0, cell.1).is_none()
        })
        .collect();
    // Farthest first; ties fall back to row-major order for determinism.
    ordered.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut spots: Vec<_> = ordered
        .iter()
        .map(|&(_, cell)| cell)
        .filter(|cell| ends.contains(cell))
        .collect();
    spots.extend(
        ordered
            .iter()
            .map(|&(_, cell)| cell)
            .filter(|cell| !ends.contains(cell)),
    );
    spots.truncate(count);
    spots
}

/// Atlas column for a tile of the doubled grid: cells are odd/odd tiles,
/// even tiles between two cells carve the passage (or stay hedge), the
/// border and everything else is hedge. The exit cell gets the mat.
fn tile_kind(maze: &Maze, exit: (u32, u32), tile: (u32, u32), size: (u32, u32)) -> u32 {
    let (tx, ty) = tile;
    let (tw, th) = size;
    match (tx % 2, ty % 2) {
        (1, 1) => {
            let cell = (tx / 2, ty / 2);
            if cell == exit {
                EXIT_ATLAS_INDEX
            } else if maze.is_deep(cell.0, cell.1) {
                DEEP_ATLAS_INDEX
            } else if let Some(flow) = maze.flow_at(cell.0, cell.1) {
                water_atlas_index(flow)
            } else if maze.is_spot(cell.0, cell.1) {
                SPOT_ATLAS_INDEX
            } else {
                FLOOR_ATLAS_INDEX
            }
        }
        // Between cell (tx/2 - 1, ty/2) and (tx/2, ty/2): floor iff the
        // west cell's east passage is carved — water, chevron along the
        // flow, when that passage carries the current, or a fallen log
        // lying across it. The current crosses either way: the west cell
        // flowing east or the east cell flowing west — checking one end
        // only used to split westbound runs into alternating rapids.
        (0, 1) if tx >= 1 && tx + 1 < tw => {
            let west = ((tx - 1) / 2, ty / 2);
            let east = (west.0 + 1, west.1);
            if maze.is_deep(west.0, west.1) || maze.is_deep(east.0, east.1) {
                DEEP_ATLAS_INDEX
            } else if maze.is_logged(west.0 as i32, west.1 as i32, Direction::East) {
                LOG_HORIZONTAL_ATLAS_INDEX
            } else {
                let crossing = match (
                    maze.flow_at(west.0, west.1),
                    maze.flow_at(east.0, east.1),
                ) {
                    (Some(Direction::East), _) => Some(Direction::East),
                    (_, Some(Direction::West)) => Some(Direction::West),
                    _ => None,
                };
                match crossing {
                    Some(flow) => water_atlas_index(flow),
                    _ if maze.is_open(west.0 as i32, west.1 as i32, Direction::East) => {
                        FLOOR_ATLAS_INDEX
                    }
                    _ => WALL_ATLAS_INDEX,
                }
            }
        }
        // Between cell (tx/2, ty/2 - 1) and (tx/2, ty/2): floor iff the
        // south cell's north passage is carved — water along the flow
        // (southbound runs included), or a log across it.
        (1, 0) if ty >= 1 && ty + 1 < th => {
            let south = (tx / 2, (ty - 1) / 2);
            let north = (south.0, south.1 + 1);
            if maze.is_deep(south.0, south.1) || maze.is_deep(north.0, north.1) {
                DEEP_ATLAS_INDEX
            } else if maze.is_logged(south.0 as i32, south.1 as i32, Direction::North) {
                LOG_VERTICAL_ATLAS_INDEX
            } else {
                let crossing = match (
                    maze.flow_at(south.0, south.1),
                    maze.flow_at(north.0, north.1),
                ) {
                    (Some(Direction::North), _) => Some(Direction::North),
                    (_, Some(Direction::South)) => Some(Direction::South),
                    _ => None,
                };
                match crossing {
                    Some(flow) => water_atlas_index(flow),
                    _ if maze.is_open(south.0 as i32, south.1 as i32, Direction::North) => {
                        FLOOR_ATLAS_INDEX
                    }
                    _ => WALL_ATLAS_INDEX,
                }
            }
        }
        _ => WALL_ATLAS_INDEX,
    }
}

/// World-space centre of a tile for an anchor-centred map whose tilemap
/// entity sits at the origin.
pub fn tile_world_pos(map_size: (u32, u32), tile: (u32, u32), tile_size: f32) -> Vec2 {
    Vec2::new(
        (tile.0 as f32 - map_size.0 as f32 / 2.0 + 0.5) * tile_size,
        (tile.1 as f32 - map_size.1 as f32 / 2.0 + 0.5) * tile_size,
    )
}

/// World-space centre of a maze cell. Cell (x, y) renders at tile
/// (2x + 1, 2y + 1) of the (2w+1) x (2h+1) tile grid — hedges fill the rest.
pub fn cell_world_pos(map_cells: (u32, u32), cell: (u32, u32), tile_size: f32) -> Vec2 {
    tile_world_pos(
        (map_cells.0 * 2 + 1, map_cells.1 * 2 + 1),
        (cell.0 * 2 + 1, cell.1 * 2 + 1),
        tile_size,
    )
}

/// Atlas column for a water tile flowing in `direction`.
fn water_atlas_index(direction: Direction) -> u32 {
    match direction {
        Direction::East => WATER_EAST_ATLAS_INDEX,
        Direction::West => WATER_WEST_ATLAS_INDEX,
        Direction::North => WATER_NORTH_ATLAS_INDEX,
        Direction::South => WATER_SOUTH_ATLAS_INDEX,
    }
}

/// Whether an atlas pixel sits on the water tile's flow chevron. The
/// canonical chevron points east; the other directions rotate the pixel.
fn water_chevron(direction: Direction, px: u32, py: u32, tile_px: u32) -> bool {
    let s = tile_px as i32;
    let (mut px, mut py) = (px as i32, py as i32);
    match direction {
        Direction::East => {}
        Direction::West => {
            // 180°: the apex swings to the left.
            px = s - 1 - px;
            py = s - 1 - py;
        }
        Direction::North => {
            // The apex swings to the top.
            let swap = px;
            px = s - 1 - py;
            py = swap;
        }
        Direction::South => {
            // The apex swings to the bottom.
            let swap = px;
            px = py;
            py = s - 1 - swap;
        }
    }
    let c = s / 2;
    let apex = s - 4;
    let arm = s / 4;
    // Two diagonal strokes running from the left of centre and meeting
    // at the apex — clipped there, so the shape reads as a chevron and
    // not a crossing.
    let upper = ((px - py) - (apex - c)).abs() <= 1;
    let lower = ((px + py) - (apex + c)).abs() <= 1;
    px >= c - arm && px <= apex && (upper || lower)
}

/// RGBA of a fallen-log pixel: bark brown with a lit long edge, a dark
/// opposite edge, end caps, and the occasional grain streak. A horizontal
/// log lies along the passage (grain runs east–west); a vertical one is
/// the same log stood the other way.
fn log_pixel(horizontal: bool, px: u32, py: u32, tile_px: u32) -> [u8; 4] {
    const BARK: [u8; 4] = [124, 92, 60, 255];
    const BARK_LIT: [u8; 4] = [156, 122, 82, 255];
    const BARK_DARK: [u8; 4] = [94, 68, 44, 255];
    // Along and across the log, so one body of code draws both layouts.
    let (along, across) = if horizontal { (px, py) } else { (py, px) };
    if across < 2 {
        BARK_LIT
    } else if across >= tile_px - 2 || along < 3 || along >= tile_px - 3 {
        BARK_DARK
    } else if (along * 5 + across * 11).is_multiple_of(13) {
        BARK_LIT
    } else {
        BARK
    }
}

/// The title map's atlas: two plain solid tiles (letter, hedge) — the
/// clean single expression of each character that the textured gameplay
/// atlas would blur.
fn build_title_atlas(tile_size: f32) -> Image {
    let tile_px = tile_size as u32;
    let columns = [TITLE_LETTER_COLOUR, TITLE_HEDGE_COLOUR];
    let mut data = Vec::with_capacity((tile_px * 2 * tile_px * 4) as usize);
    for _ in 0..tile_px {
        for x in 0..tile_px * 2 {
            let colour = columns[(x / tile_px) as usize].to_srgba();
            data.extend_from_slice(&[
                (colour.red * 255.0) as u8,
                (colour.green * 255.0) as u8,
                (colour.blue * 255.0) as u8,
                255,
            ]);
        }
    }
    Image::new(
        Extent3d {
            width: tile_px * 2,
            height: tile_px,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
}

/// RGBA of one atlas pixel: cream floor tiles with a subtle border, hedge
/// green walls with a lit top edge and speckles, and a pink exit mat.
fn atlas_pixel(column: u32, px: u32, py: u32, tile_px: u32) -> [u8; 4] {
    const FLOOR: [u8; 4] = [246, 234, 215, 255];
    const FLOOR_EDGE: [u8; 4] = [233, 217, 193, 255];
    const HEDGE: [u8; 4] = [106, 153, 78, 255];
    const HEDGE_LIT: [u8; 4] = [127, 175, 95, 255];
    const HEDGE_DARK: [u8; 4] = [84, 124, 60, 255];
    const WATER: [u8; 4] = [127, 178, 190, 255];
    const WATER_CREST: [u8; 4] = [214, 236, 242, 255];
    const MAT: [u8; 4] = [238, 141, 176, 255];
    const MAT_EDGE: [u8; 4] = [250, 205, 222, 255];
    const DEEP: [u8; 4] = [74, 110, 124, 255];
    const DEEP_EDGE: [u8; 4] = [64, 98, 112, 255];
    const PEG: [u8; 4] = [140, 105, 70, 255];
    let border = px == 0 || py == 0 || px == tile_px - 1 || py == tile_px - 1;
    match column {
        FLOOR_ATLAS_INDEX => {
            if border {
                FLOOR_EDGE
            } else {
                FLOOR
            }
        }
        WALL_ATLAS_INDEX => {
            if py < 2 {
                HEDGE_LIT
            } else if py >= tile_px - 2 || (px * 7 + py * 13).is_multiple_of(11) {
                HEDGE_DARK
            } else {
                HEDGE
            }
        }
        WATER_EAST_ATLAS_INDEX..=WATER_SOUTH_ATLAS_INDEX => {
            // A cool pool with a light chevron pointing downstream.
            let direction = match column {
                WATER_EAST_ATLAS_INDEX => Direction::East,
                WATER_WEST_ATLAS_INDEX => Direction::West,
                WATER_NORTH_ATLAS_INDEX => Direction::North,
                _ => Direction::South,
            };
            if water_chevron(direction, px, py, tile_px) {
                WATER_CREST
            } else {
                WATER
            }
        }
        LOG_HORIZONTAL_ATLAS_INDEX => log_pixel(true, px, py, tile_px),
        LOG_VERTICAL_ATLAS_INDEX => log_pixel(false, px, py, tile_px),
        DEEP_ATLAS_INDEX => {
            // Still, darker water: no current, no chevron.
            if border {
                DEEP_EDGE
            } else {
                DEEP
            }
        }
        SPOT_ATLAS_INDEX => {
            // The floor with a peg: a plank spot.
            if (px as i32 - tile_px as i32 / 2).abs() <= 2
                && (py as i32 - tile_px as i32 / 2).abs() <= 2
            {
                PEG
            } else if border {
                FLOOR_EDGE
            } else {
                FLOOR
            }
        }
        _ => {
            if border {
                MAT_EDGE
            } else {
                MAT
            }
        }
    }
}

/// Build the atlas image: one column per tile variant. Image data is
/// row-major across the FULL atlas width — row 0 of every column first,
/// then the next row — so the loops must walk rows, then x.
fn build_atlas(tile_size: f32) -> Image {
    let tile_px = tile_size as u32;
    let mut data = Vec::with_capacity((tile_px * ATLAS_TILES * tile_px * 4) as usize);
    for py in 0..tile_px {
        for x in 0..tile_px * ATLAS_TILES {
            let column = x / tile_px;
            let px = x % tile_px;
            data.extend_from_slice(&atlas_pixel(column, px, py, tile_px));
        }
    }
    Image::new(
        Extent3d {
            width: tile_px * ATLAS_TILES,
            height: tile_px,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_world_pos_centres_the_grid_on_the_origin() {
        // 2x2 cells of 10px tiles: cell (0,0) sits at tile (1,1) of the 5x5
        // tile grid — one tile left of and below the centre — and (1,1)
        // mirrors it.
        assert_eq!(
            cell_world_pos((2, 2), (0, 0), 10.0),
            Vec2::new(-10.0, -10.0)
        );
        assert_eq!(cell_world_pos((2, 2), (1, 1), 10.0), Vec2::new(10.0, 10.0));
    }

    #[test]
    fn tile_kind_carves_only_open_passages() {
        let maze = Maze::generate(3, 3, 42, 0.0);
        let exit = (2, 2);
        let size = (7, 7);
        // Every cell is floor, except the exit's mat.
        for y in 0..3u32 {
            for x in 0..3u32 {
                let kind = tile_kind(&maze, exit, (2 * x + 1, 2 * y + 1), size);
                let expected = if (x, y) == exit {
                    EXIT_ATLAS_INDEX
                } else {
                    FLOOR_ATLAS_INDEX
                };
                assert_eq!(kind, expected, "cell {x},{y}");
            }
        }
        // An open east passage carves the even tile between the cells.
        let carved: Vec<_> = (0..3u32)
            .flat_map(|y| (0..3u32).map(move |x| (x, y)))
            .filter(|&(x, y)| x + 1 < 3 && maze.is_open(x as i32, y as i32, Direction::East))
            .collect();
        assert!(!carved.is_empty(), "the maze has eastward passages");
        for (x, y) in carved {
            assert_eq!(
                tile_kind(&maze, exit, (2 * x + 2, 2 * y + 1), size),
                FLOOR_ATLAS_INDEX
            );
        }
        // The border stays hedge.
        assert_eq!(tile_kind(&maze, exit, (0, 0), size), WALL_ATLAS_INDEX);
        // A closed north passage stays hedge.
        let closed = (0..3u32)
            .flat_map(|y| (0..3u32).map(move |x| (x, y)))
            .find(|&(x, y)| y + 1 < 3 && !maze.is_open(x as i32, y as i32, Direction::North));
        assert!(closed.is_some(), "the maze has closed north passages");
        if let Some((x, y)) = closed {
            assert_eq!(
                tile_kind(&maze, exit, (2 * x + 1, 2 * y + 2), size),
                WALL_ATLAS_INDEX
            );
        }
    }

    #[test]
    fn atlas_data_is_row_major() {
        // Regression: the atlas must be written row-major across the full
        // atlas width (see the layout comment on build_atlas).
        let tile_px = 8u32;
        let image = build_atlas(tile_px as f32);
        let data = image
            .data
            .as_deref()
            .expect("the atlas keeps its data on the main world");
        let width = (tile_px * ATLAS_TILES) as usize;
        for py in [0u32, 3, 7] {
            for x in 0..width as u32 {
                let expected = atlas_pixel(x / tile_px, x % tile_px, py, tile_px);
                let i = ((py as usize) * width + x as usize) * 4;
                assert_eq!(&data[i..i + 4], &expected[..]);
            }
        }
    }

    #[test]
    fn tile_kind_paints_logged_passages() {
        // Find a generated maze's logged passages — one of each orientation
        // across a few seeds — and check the tile between the cells gets
        // the log, not the floor.
        let mut saw_horizontal = false;
        let mut saw_vertical = false;
        for seed in 0..20u64 {
            let maze = Maze::with_obstacles(13, 11, seed, 0.2, 2, 3);
            let exit = (0, 0);
            let size = (27, 23);
            for y in 0..11u32 {
                for x in 0..13u32 {
                    if maze.is_logged(x as i32, y as i32, Direction::East) && x + 1 < 13 {
                        saw_horizontal = true;
                        assert_eq!(
                            tile_kind(&maze, exit, (2 * x + 2, 2 * y + 1), size),
                            LOG_HORIZONTAL_ATLAS_INDEX,
                            "east passage at {x},{y}"
                        );
                    }
                    if maze.is_logged(x as i32, y as i32, Direction::North) && y + 1 < 11 {
                        saw_vertical = true;
                        assert_eq!(
                            tile_kind(&maze, exit, (2 * x + 1, 2 * y + 2), size),
                            LOG_VERTICAL_ATLAS_INDEX,
                            "north passage at {x},{y}"
                        );
                    }
                }
            }
        }
        assert!(saw_horizontal, "some seed laid a horizontal log");
        assert!(saw_vertical, "some seed laid a vertical log");
    }

    #[test]
    fn streams_render_as_continuous_water() {
        // Every passage a current crosses renders as water pointing along
        // the flow — whichever way the stream runs — so a run never breaks
        // into alternating rapids and dry floor.
        let (exit, size) = ((0u32, 0u32), (27u32, 23u32));
        let mut saw_horizontal = false;
        let mut saw_vertical = false;
        for seed in 0..10u64 {
            let maze = Maze::with_streams(13, 11, seed, 0.2, 4);
            for y in 0..11u32 {
                for x in 0..13u32 {
                    let Some(flow) = maze.flow_at(x, y) else {
                        continue;
                    };
                    // Generation keeps the let-off in bounds, so the tile
                    // along the flow is always a passage tile the guard
                    // arms accept.
                    let tile = match flow {
                        Direction::East => (2 * x + 2, 2 * y + 1),
                        Direction::West => (2 * x, 2 * y + 1),
                        Direction::North => (2 * x + 1, 2 * y + 2),
                        Direction::South => (2 * x + 1, 2 * y),
                    };
                    saw_horizontal |= matches!(flow, Direction::East | Direction::West);
                    saw_vertical |= matches!(flow, Direction::North | Direction::South);
                    assert_eq!(
                        tile_kind(&maze, exit, tile, size),
                        water_atlas_index(flow),
                        "the {flow:?} passage out of {x},{y}"
                    );
                }
            }
        }
        assert!(saw_horizontal, "some seed ran a stream east or west");
        assert!(saw_vertical, "some seed ran a stream north or south");
    }

    #[test]
    fn water_chevrons_point_their_direction() {
        // The apex — the chevron's extreme pixels along the pointing axis —
        // must hug the tile's axis centre. With the vertical rotations
        // swapped, the north and south arrows pointed backwards.
        const TILE_PX: u32 = 40; // the gameplay atlas' tile size
        let centre = (TILE_PX / 2) as i32;
        let chevron = |direction: Direction| -> Vec<(i32, i32)> {
            let mut points = Vec::new();
            for py in 0..TILE_PX {
                for px in 0..TILE_PX {
                    if water_chevron(direction, px, py, TILE_PX) {
                        points.push((px as i32, py as i32));
                    }
                }
            }
            assert!(!points.is_empty(), "{direction:?} draws a chevron");
            points
        };
        // For each direction: key picks the pointing axis (negated where
        // the apex is the minimum), cross picks the off-axis coordinate —
        // and the apex pixels must hug the tile's axis centre.
        let assert_apex_centred = |
            points: Vec<(i32, i32)>,
            key: &dyn Fn(&(i32, i32)) -> i32,
            cross: &dyn Fn(&(i32, i32)) -> i32,
        | {
            let apex = points.iter().map(|p| key(p)).max().unwrap();
            let off_axis: Vec<i32> = points
                .iter()
                .filter(|p| key(p) == apex)
                .map(|p| cross(p))
                .collect();
            assert!(
                off_axis.iter().all(|&v| (v - centre).abs() <= 2),
                "the apex hugs the axis centre, got {off_axis:?}"
            );
        };
        assert_apex_centred(chevron(Direction::East), &|p| p.0, &|p| p.1);
        assert_apex_centred(chevron(Direction::West), &|p| -p.0, &|p| p.1);
        assert_apex_centred(chevron(Direction::North), &|p| -p.1, &|p| p.0);
        assert_apex_centred(chevron(Direction::South), &|p| p.1, &|p| p.0);
    }

    #[test]
    fn duckling_spots_prefer_distant_dead_ends() {
        // A braided maze keeps few dead ends; spots must never be the start
        // or exit, and never exceed the count.
        for seed in 0..10u64 {
            let maze = Maze::with_streams(7, 5, seed, 0.2, 1);
            let distances = maze.distances_from((0, 0));
            let exit = farthest_cell(&maze, &distances, (7, 5));
            let spots = duckling_spots(&maze, (0, 0), exit, 3);
            assert!(spots.len() <= 3);
            assert!(spots.iter().all(|&cell| cell != (0, 0) && cell != exit));
            // Spots are dry — a duckling on the current is swept off
            // before it can be found.
            assert!(
                spots.iter().all(|&cell| maze.flow_at(cell.0, cell.1).is_none()),
                "every spot is dry"
            );
            // Spots are unique.
            let mut sorted = spots.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(sorted.len(), spots.len());
        }
    }
}
