//! Level construction: generate a braided maze, materialise it as a tilemap
//! (cells as floor, passages carved through, everything else hedge), spawn
//! the duck, her waiting ducklings and the exit mat. The tile atlas is
//! procedural so the prototype needs no art assets.

use bevy::asset::RenderAssetUsages;
use bevy::image::Image;
use bevy::math::primitives::Circle;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_ecs_tilemap::prelude::*;

use maze::{Direction, Maze};

use crate::duck::{BufferedMove, Duck, PathHistory};
use crate::ducklings::{Clutch, Duckling, Waiting, follow_step};
use crate::level::{LevelIndex, RunSeed, level_plan};

/// Size of a single tile in world units (pixels).
pub const TILE_SIZE: f32 = 40.0;

/// Exponential smoothing rate for camera follow.
const CAMERA_SMOOTHING: f32 = 6.0;

/// Atlas columns: cream floor, hedge wall, pink exit mat.
const FLOOR_ATLAS_INDEX: u32 = 0;
const WALL_ATLAS_INDEX: u32 = 1;
const EXIT_ATLAS_INDEX: u32 = 2;
const ATLAS_TILES: u32 = 3;

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
        app.add_plugins(TilemapPlugin)
            .add_systems(Startup, setup)
            .add_systems(Update, camera_follow);
    }
}

/// Startup: spawn the camera and build level 0.
fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    level: Res<LevelIndex>,
    seed: Res<RunSeed>,
) {
    commands.spawn((MainCamera, Camera2d, Transform::from_xyz(0.0, 0.0, 1000.0)));
    let plan = level_plan(level.0);
    build_level(
        &mut commands,
        &mut images,
        &mut meshes,
        &mut materials,
        &plan,
        seed.0.wrapping_add(level.0 as u64),
    );
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
    let maze = Maze::generate(plan.width, plan.height, seed, plan.braid);
    let start = (0u32, 0u32);
    let cells = (plan.width, plan.height);
    // The exit is the farthest cell from the start.
    let distances = maze.distances_from(start);
    let exit = farthest_cell(&distances, cells);
    // Ducklings wait at dead ends, farthest first, padded with the next
    // most distant cells when braiding leaves too few dead ends.
    let spots = duckling_spots(&maze, start, exit, plan.ducklings);

    commands.insert_resource(MazeRes {
        maze: maze.clone(),
        start,
        exit,
    });
    commands.insert_resource(Clutch {
        collected: 0,
        total: spots.len(),
    });

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
/// row-major order).
fn farthest_cell(distances: &[Option<u32>], cells: (u32, u32)) -> (u32, u32) {
    let mut best = (0u32, 0u32);
    let mut best_steps = 0u32;
    for (i, steps) in distances.iter().enumerate() {
        let Some(steps) = steps else { continue };
        let cell = (i as u32 % cells.0, i as u32 / cells.0);
        if *steps > best_steps {
            best = cell;
            best_steps = *steps;
        }
    }
    best
}

/// `count` duckling spots: dead ends first (farthest from the start), then
/// the next most distant ordinary cells. Never the start or the exit.
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
        .filter(|&(_, cell)| cell != start && cell != exit)
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
            if (tx / 2, ty / 2) == exit {
                EXIT_ATLAS_INDEX
            } else {
                FLOOR_ATLAS_INDEX
            }
        }
        // Between cell (tx/2 - 1, ty/2) and (tx/2, ty/2): floor iff the
        // west cell's east passage is carved.
        (0, 1) if tx >= 1 && tx + 1 < tw => {
            let west = ((tx - 1) / 2, ty / 2);
            if maze.is_open(west.0 as i32, west.1 as i32, Direction::East) {
                FLOOR_ATLAS_INDEX
            } else {
                WALL_ATLAS_INDEX
            }
        }
        // Between cell (tx/2, ty/2 - 1) and (tx/2, ty/2): floor iff the
        // south cell's north passage is carved.
        (1, 0) if ty >= 1 && ty + 1 < th => {
            let south = (tx / 2, (ty - 1) / 2);
            if maze.is_open(south.0 as i32, south.1 as i32, Direction::North) {
                FLOOR_ATLAS_INDEX
            } else {
                WALL_ATLAS_INDEX
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

/// RGBA of one atlas pixel: cream floor tiles with a subtle border, hedge
/// green walls with a lit top edge and speckles, and a pink exit mat.
fn atlas_pixel(column: u32, px: u32, py: u32, tile_px: u32) -> [u8; 4] {
    const FLOOR: [u8; 4] = [246, 234, 215, 255];
    const FLOOR_EDGE: [u8; 4] = [233, 217, 193, 255];
    const HEDGE: [u8; 4] = [106, 153, 78, 255];
    const HEDGE_LIT: [u8; 4] = [127, 175, 95, 255];
    const HEDGE_DARK: [u8; 4] = [84, 124, 60, 255];
    const MAT: [u8; 4] = [238, 141, 176, 255];
    const MAT_EDGE: [u8; 4] = [250, 205, 222, 255];
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
    fn duckling_spots_prefer_distant_dead_ends() {
        // A braided maze keeps few dead ends; spots must never be the start
        // or exit, and never exceed the count.
        for seed in 0..10u64 {
            let maze = Maze::generate(7, 5, seed, 0.2);
            let distances = maze.distances_from((0, 0));
            let exit = farthest_cell(&distances, (7, 5));
            let spots = duckling_spots(&maze, (0, 0), exit, 3);
            assert!(spots.len() <= 3);
            assert!(spots.iter().all(|&cell| cell != (0, 0) && cell != exit));
            // Spots are unique.
            let mut sorted = spots.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(sorted.len(), spots.len());
        }
    }
}
