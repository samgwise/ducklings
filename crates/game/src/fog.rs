//! Fog of war: on the two harder difficulties the maze sleeps in the dark.
//! A second tilemap veils every tile mum's sight has not reached; cells she
//! has seen stay dimly remembered, cells she sees now render clear. A pure
//! overlay — no generation or movement code changes, so the solvability
//! gates keep holding.

use std::collections::{HashSet, VecDeque};

use bevy::asset::RenderAssetUsages;
use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_ecs_tilemap::prelude::*;

use maze::{Direction, Maze};

use crate::duck::Duck;
use crate::level::GamePhase;
use crate::world::{LevelEntity, MazeRes, TILE_SIZE};

/// How many corridor steps mum's sight reaches from her cell.
pub const FOG_REVEAL_RADIUS: u32 = 4;

/// Alpha of the translucent veil over remembered (explored but unlit) tiles.
const FOG_REMEMBERED_ALPHA: u8 = 150;

/// The fog's colour — a very dark green-black night.
const FOG_COLOUR: [u8; 3] = [14, 20, 10];

/// Height of the fog layer above the gameplay tilemap — above the birds
/// too (mum swims at z 1.0), so unseen ducklings stay hidden until found.
const FOG_LAYER_Z: f32 = 5.0;

/// Atlas columns: opaque dark for unexplored, a translucent veil for
/// remembered, fully clear for lit.
const FOG_UNSEEN_ATLAS_INDEX: u32 = 0;
const FOG_REMEMBERED_ATLAS_INDEX: u32 = 1;
const FOG_CLEAR_ATLAS_INDEX: u32 = 2;
const FOG_ATLAS_TILES: u32 = 3;

/// How much fog covers a tile, ordered darkest to clearest so states
/// compare — a passage is as clear as its clearest endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Fog {
    Unseen,
    Remembered,
    Clear,
}

impl Fog {
    /// The fog atlas column that renders this state.
    fn atlas_index(self) -> u32 {
        match self {
            Fog::Unseen => FOG_UNSEEN_ATLAS_INDEX,
            Fog::Remembered => FOG_REMEMBERED_ATLAS_INDEX,
            Fog::Clear => FOG_CLEAR_ATLAS_INDEX,
        }
    }
}

/// The level's fog: whether this run plays with it, and every cell sight
/// has ever touched. Rebuilt with each level.
#[derive(Resource, Debug, Default)]
pub struct FogState {
    pub enabled: bool,
    pub seen: HashSet<(u32, u32)>,
}

/// Marks the fog layer's tilemap entity.
#[derive(Component)]
struct FogLayer;

pub struct FogPlugin;

impl Plugin for FogPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FogState>()
            .add_systems(Update, update_fog.run_if(in_state(GamePhase::Playing)));
    }
}

/// The cells mum's sight reaches from `from`: a flood of at most `radius`
/// steps through open passages. Hedges block the flood, so sight follows
/// corridors — it never bleeds through a wall to a geographically close
/// but corridor-far cell.
pub fn reveal(maze: &Maze, from: (u32, u32), radius: u32) -> HashSet<(u32, u32)> {
    let mut lit = HashSet::new();
    let mut frontier = VecDeque::new();
    lit.insert(from);
    frontier.push_back((from, 0u32));
    while let Some(((x, y), steps)) = frontier.pop_front() {
        if steps == radius {
            continue;
        }
        for dir in Direction::ALL {
            if !maze.is_open(x as i32, y as i32, dir) {
                continue;
            }
            let (dx, dy) = dir.delta();
            let next = ((x as i32 + dx) as u32, (y as i32 + dy) as u32);
            if lit.insert(next) {
                frontier.push_back((next, steps + 1));
            }
        }
    }
    lit
}

/// Spawn the fog layer for a level: one dark tile per position over the
/// maze, plus the level's [`FogState`]. Difficulties without fog get the
/// state (disabled) and no layer at all.
pub(crate) fn build_fog(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    map_tiles: (u32, u32),
    enabled: bool,
) {
    commands.insert_resource(FogState {
        enabled,
        seen: HashSet::new(),
    });
    if !enabled {
        return;
    }
    let atlas: Handle<Image> = images.add(build_fog_atlas(TILE_SIZE));
    let tilemap_entity = commands.spawn_empty().id();
    let mut storage = TileStorage::empty(TilemapSize {
        x: map_tiles.0,
        y: map_tiles.1,
    });
    for ty in 0..map_tiles.1 {
        for tx in 0..map_tiles.0 {
            let pos = TilePos { x: tx, y: ty };
            let entity = commands
                .spawn((
                    LevelEntity,
                    TileBundle {
                        position: pos,
                        texture_index: TileTextureIndex(FOG_UNSEEN_ATLAS_INDEX),
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
        FogLayer,
        TilemapBundle {
            grid_size: tile_size.into(),
            size: TilemapSize {
                x: map_tiles.0,
                y: map_tiles.1,
            },
            storage,
            texture: TilemapTexture::Single(atlas),
            tile_size,
            anchor: TilemapAnchor::Center,
            transform: Transform::from_xyz(0.0, 0.0, FOG_LAYER_Z),
            ..Default::default()
        },
    ));
}

/// Mum moved: flood her sight through the passages, fold the newly lit
/// cells into the memory, and re-tint every fog tile to the freshest
/// state. Levels without fog never leave the early returns.
#[allow(clippy::type_complexity)]
fn update_fog(
    duck: Option<Single<&Duck, Changed<Duck>>>,
    maze_res: Res<MazeRes>,
    mut fog: ResMut<FogState>,
    layer: Option<Single<&TileStorage, With<FogLayer>>>,
    mut tiles: Query<&mut TileTextureIndex>,
) {
    let Some(duck) = duck else {
        return;
    };
    let Some(storage) = layer else {
        return;
    };
    if !fog.enabled {
        return;
    }
    // The flood itself is what is lit right now; everything ever lit —
    // the flood included — stays remembered.
    let lit = reveal(&maze_res.maze, duck.cell, FOG_REVEAL_RADIUS);
    fog.seen.extend(lit.iter().copied());
    let cells = (maze_res.maze.width(), maze_res.maze.height());
    for ty in 0..cells.1 * 2 + 1 {
        for tx in 0..cells.0 * 2 + 1 {
            let Some(entity) = storage.get(&TilePos { x: tx, y: ty }) else {
                continue;
            };
            let state = tile_fog((tx, ty), cells, &lit, &fog.seen);
            if let Ok(mut index) = tiles.get_mut(entity) {
                index.0 = state.atlas_index();
            }
        }
    }
}

/// The fog over one tile of the doubled grid: the clearest state among the
/// cells it shadows — its own cell when it is a cell tile, the two
/// endpoints when it is a passage, the four corners when it is a hedge
/// join. Hedges light with their first lit neighbour, so a dark maze keeps
/// one clean silhouette.
fn tile_fog(
    tile: (u32, u32),
    cells: (u32, u32),
    lit: &HashSet<(u32, u32)>,
    seen: &HashSet<(u32, u32)>,
) -> Fog {
    let mut best = Fog::Unseen;
    for cx in shadowing_cells(tile.0, cells.0) {
        for cy in shadowing_cells(tile.1, cells.1) {
            let cell = (cx, cy);
            best = best.max(if lit.contains(&cell) {
                Fog::Clear
            } else if seen.contains(&cell) {
                Fog::Remembered
            } else {
                Fog::Unseen
            });
        }
    }
    best
}

/// The cell coordinates a tile coordinate shadows: itself when it is a
/// cell (odd) tile, the pair either side when it is a passage or hedge
/// (even) tile. Out-of-bounds halves are dropped.
fn shadowing_cells(t: u32, count: u32) -> Vec<u32> {
    if t % 2 == 1 {
        return vec![t / 2];
    }
    [(t as i32 - 1) / 2, (t as i32 + 1) / 2]
        .into_iter()
        .filter(|&c| (0..count as i32).contains(&c))
        .map(|c| c as u32)
        .collect()
}

/// The fog atlas: one column per state — opaque dark for unexplored, a
/// translucent veil for remembered, fully clear for lit.
fn build_fog_atlas(tile_size: f32) -> Image {
    let alphas = [255u8, FOG_REMEMBERED_ALPHA, 0];
    let tile_px = tile_size as u32;
    let mut data = Vec::with_capacity((tile_px * FOG_ATLAS_TILES * tile_px * 4) as usize);
    for _ in 0..tile_px {
        for x in 0..tile_px * FOG_ATLAS_TILES {
            let alpha = alphas[(x / tile_px) as usize];
            data.extend_from_slice(&[FOG_COLOUR[0], FOG_COLOUR[1], FOG_COLOUR[2], alpha]);
        }
    }
    Image::new(
        Extent3d {
            width: tile_px * FOG_ATLAS_TILES,
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

    /// A small helper: the set of cells from a slice.
    fn set(cells: &[(u32, u32)]) -> HashSet<(u32, u32)> {
        cells.iter().copied().collect()
    }

    #[test]
    fn sight_floods_corridors_and_stops_at_hedges() {
        // In a perfect maze, sight must follow corridors: every lit cell is
        // within the radius's walk, and a cell that is geographically close
        // but corridor-far stays dark.
        let mut found = false;
        for seed in 0..20u64 {
            let maze = Maze::generate(7, 5, seed, 0.0);
            let lit = reveal(&maze, (0, 0), 3);
            assert!(lit.contains(&(0, 0)));
            let distances = maze.distances_from((0, 0));
            for cell in &lit {
                let steps = distances[(cell.1 * 7 + cell.0) as usize]
                    .expect("lit cells are reachable");
                assert!(steps <= 3, "cell {cell:?} lit from {steps} steps away");
            }
            // Find a walled-off neighbour: manhattan-close, corridor-far.
            let hidden = distances.iter().enumerate().any(|(i, steps)| {
                let cell = (i as u32 % 7, i as u32 / 7);
                let manhattan = cell.0.abs_diff(0) + cell.1.abs_diff(0);
                manhattan <= 2 && steps.map_or(false, |s| s > 3) && !lit.contains(&cell)
            });
            if hidden {
                found = true;
                break;
            }
        }
        assert!(found, "some seed walls a nearby cell out of sight");
    }

    #[test]
    fn a_passage_lights_when_either_endpoint_does() {
        let cells = (3u32, 3u32);
        // Tile (2,1) sits between cells (0,0) and (1,0).
        let tile = (2u32, 1u32);
        assert_eq!(
            tile_fog(tile, cells, &set(&[(0, 0)]), &HashSet::new()),
            Fog::Clear
        );
        assert_eq!(
            tile_fog(tile, cells, &set(&[(1, 0)]), &HashSet::new()),
            Fog::Clear
        );
        assert_eq!(
            tile_fog(tile, cells, &HashSet::new(), &set(&[(0, 0)])),
            Fog::Remembered
        );
        assert_eq!(
            tile_fog(tile, cells, &HashSet::new(), &HashSet::new()),
            Fog::Unseen
        );
    }

    #[test]
    fn hedge_corners_light_with_any_neighbour() {
        // Tile (2,2) is a hedge join shadowed by the four cells around it —
        // any one of them lit or seen lifts the corner out of the dark.
        let cells = (3u32, 3u32);
        assert_eq!(
            tile_fog((2, 2), cells, &HashSet::new(), &set(&[(1, 1)])),
            Fog::Remembered
        );
        assert_eq!(
            tile_fog((2, 2), cells, &set(&[(0, 1)]), &HashSet::new()),
            Fog::Clear
        );
        assert_eq!(
            tile_fog((2, 2), cells, &HashSet::new(), &HashSet::new()),
            Fog::Unseen
        );
    }

    #[test]
    fn the_fog_atlas_veils_by_alpha() {
        // Unexplored tiles are opaque, remembered ones a translucent veil,
        // lit ones fully clear.
        let tile_px = 8u32;
        let image = build_fog_atlas(tile_px as f32);
        let data = image
            .data
            .as_deref()
            .expect("the atlas keeps its data on the main world");
        let py = tile_px / 2;
        for (column, expected_alpha) in [(0u32, 255u8), (1, FOG_REMEMBERED_ALPHA), (2, 0)] {
            let x = column * tile_px + tile_px / 2;
            let i = ((py * FOG_ATLAS_TILES * tile_px + x) as usize) * 4;
            assert_eq!(data[i], FOG_COLOUR[0], "column {column} keeps the fog colour");
            assert_eq!(data[i + 3], expected_alpha, "column {column} alpha");
        }
    }
}
