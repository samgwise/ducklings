//! Bridges: on bridge levels a channel of still deep water splits the
//! maze, and the player lays a limited stock of planks between facing
//! spots — click the water gap to lay, click a laid plank to pick it back
//! up — until the banks connect. With the whole clutch gathered and a
//! chain reaching the exit's side, the family waddles across.

use std::collections::{HashMap, HashSet, VecDeque};

use bevy::math::primitives::Rectangle;
use bevy::prelude::*;

use maze::{Direction, Maze};

use crate::duck::{Duck, WalkPlan, world_to_cell};
use crate::ducklings::Clutch;
use crate::level::GamePhase;
use crate::world::{LevelEntity, MainCamera, MazeRes, TILE_SIZE, cell_world_pos};

/// The wood a plank wears.
const PLANK_COLOUR: Color = Color::srgb(0.59, 0.43, 0.26);

/// The plank stock: how many the level grants and how many remain to lay.
/// Rebuilt with each level; a `total` of zero marks a non-bridge level.
#[derive(Resource, Debug, Default)]
pub struct PlankStock {
    pub left: u32,
    pub total: u32,
    /// Whether the waddle-across has started this level.
    pub(crate) waddled: bool,
}

/// The deep cells currently carrying a plank.
#[derive(Resource, Debug, Default)]
pub struct Planked(pub HashSet<(u32, u32)>);

/// The shared plank mesh and material.
#[derive(Resource)]
pub(crate) struct PlankMesh {
    mesh: Handle<Mesh>,
    material: Handle<ColorMaterial>,
}

/// A plank laid across the gap at this cell.
#[derive(Component)]
struct PlankTile((u32, u32));

pub struct BridgePlugin;

impl Plugin for BridgePlugin {
    fn build(&self, app: &mut App) {
        // The HUD reads the stock from startup, long before the first
        // level inserts its own — start everyone at zero.
        app.init_resource::<PlankStock>()
            .init_resource::<Planked>()
            .add_systems(
                Update,
                (toggle_plank, waddle_across).run_if(in_state(GamePhase::Playing)),
            );
    }
}

/// Prepare a level: the plank stock, a fresh planked set and — on bridge
/// levels — the shared plank mesh and material.
pub(crate) fn build_bridge(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    chain_gaps: u32,
) {
    commands.insert_resource(if chain_gaps > 0 {
        PlankStock {
            left: chain_gaps,
            total: chain_gaps,
            waddled: false,
        }
    } else {
        PlankStock::default()
    });
    commands.insert_resource(Planked::default());
    if chain_gaps > 0 {
        commands.insert_resource(PlankMesh {
            mesh: meshes.add(Mesh::from(Rectangle::new(
                TILE_SIZE * 0.45,
                TILE_SIZE * 0.45,
            ))),
            material: materials.add(ColorMaterial::from_color(PLANK_COLOUR)),
        });
    } else {
        commands.remove_resource::<PlankMesh>();
    }
}

/// Lay a plank across the gap at `cell`; the caller has checked the stock.
pub(crate) fn lay(
    commands: &mut Commands,
    planked: &mut Planked,
    stock: &mut PlankStock,
    meshes: &PlankMesh,
    map_cells: (u32, u32),
    cell: (u32, u32),
) {
    stock.left -= 1;
    planked.0.insert(cell);
    let at = cell_world_pos(map_cells, cell, TILE_SIZE);
    commands.spawn((
        LevelEntity,
        PlankTile(cell),
        Mesh2d(meshes.mesh.clone()),
        MeshMaterial2d(meshes.material.clone()),
        Transform::from_xyz(at.x, at.y, 0.4),
    ));
}

/// Click or tap a gap: lay a plank (if the stock allows) or pick a laid
/// one back up.
#[allow(clippy::too_many_arguments)]
fn toggle_plank(
    mut commands: Commands,
    clicks: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    maze_res: Res<MazeRes>,
    mut stock: ResMut<PlankStock>,
    mut planked: ResMut<Planked>,
    meshes: Option<Res<PlankMesh>>,
    laid: Query<(Entity, &PlankTile)>,
) {
    let Some(meshes) = meshes else {
        return;
    };
    if stock.total == 0 {
        return;
    }
    let point = if clicks.just_pressed(MouseButton::Left) {
        window.cursor_position()
    } else {
        touches
            .iter_just_pressed()
            .next()
            .map(|touch| touch.position())
    };
    let Some(point) = point else {
        return;
    };
    let Ok(world) = camera.0.viewport_to_world_2d(camera.1, point) else {
        return;
    };
    let cells = (maze_res.maze.width(), maze_res.maze.height());
    let cell = world_to_cell(cells, world);
    if !maze_res.maze.is_gap(cell.0, cell.1) {
        return;
    }
    if let Some((entity, _)) = laid.iter().find(|(_, plank)| plank.0 == cell) {
        // Pick the plank back up: the stock is never lost.
        commands.entity(entity).despawn();
        planked.0.remove(&cell);
        stock.left += 1;
    } else if stock.left > 0 {
        lay(&mut commands, &mut planked, &mut stock, &meshes, cells, cell);
    }
}

/// Once the whole clutch is aboard and a plank chain joins her bank to
/// the exit's, mum leads the parade across.
fn waddle_across(
    duck: Single<(&Duck, &mut WalkPlan)>,
    maze_res: Res<MazeRes>,
    planked: Res<Planked>,
    clutch: Res<Clutch>,
    mut stock: ResMut<PlankStock>,
) {
    // Bridge levels only: on the others the exit is a walk away, and
    // the walk is the player's.
    if stock.total == 0 || stock.waddled || clutch.collected != clutch.total {
        return;
    }
    let (duck, mut plan) = duck.into_inner();
    let Some(path) = waddle_path(&maze_res.maze, &planked.0, duck.cell, maze_res.exit) else {
        return;
    };
    stock.waddled = true;
    plan.queue = path.into();
}

/// The hop directions from `from` to `target` over dry cells and planked
/// gaps, if they connect.
fn waddle_path(
    maze: &Maze,
    planked: &HashSet<(u32, u32)>,
    from: (u32, u32),
    target: (u32, u32),
) -> Option<Vec<Direction>> {
    let walkable = |cell: (u32, u32)| !maze.is_deep(cell.0, cell.1) || planked.contains(&cell);
    if !walkable(from) || !walkable(target) {
        return None;
    }
    let mut parents: HashMap<(u32, u32), ((u32, u32), Direction)> = HashMap::new();
    let mut visited = HashSet::new();
    visited.insert(from);
    let mut queue = VecDeque::new();
    queue.push_back(from);
    while let Some(cell) = queue.pop_front() {
        if cell == target {
            break;
        }
        for dir in Direction::ALL {
            if !maze.is_open(cell.0 as i32, cell.1 as i32, dir) {
                continue;
            }
            let (dx, dy) = dir.delta();
            let next = ((cell.0 as i32 + dx) as u32, (cell.1 as i32 + dy) as u32);
            if walkable(next) && visited.insert(next) {
                parents.insert(next, (cell, dir));
                queue.push_back(next);
            }
        }
    }
    if !visited.contains(&target) {
        return None;
    }
    let mut path = Vec::new();
    let mut cur = target;
    while cur != from {
        let &(prev, dir) = parents.get(&cur).expect("visited cells know their parent");
        path.push(dir);
        cur = prev;
    }
    path.reverse();
    Some(path)
}

/// The exit for a bridge level: the farthest dry cell from the start once
/// every gap is planked — it lands on the far bank.
pub(crate) fn far_exit(maze: &Maze, start: (u32, u32)) -> (u32, u32) {
    let walkable = |cell: (u32, u32)| !maze.is_deep(cell.0, cell.1) || maze.is_gap(cell.0, cell.1);
    let mut distances: HashMap<(u32, u32), u32> = HashMap::new();
    distances.insert(start, 0);
    let mut queue = VecDeque::new();
    queue.push_back(start);
    let mut farthest = start;
    let mut far = 0;
    while let Some(cell) = queue.pop_front() {
        let steps = distances[&cell];
        if !maze.is_deep(cell.0, cell.1) && steps > far {
            far = steps;
            farthest = cell;
        }
        for dir in Direction::ALL {
            if !maze.is_open(cell.0 as i32, cell.1 as i32, dir) {
                continue;
            }
            let (dx, dy) = dir.delta();
            let next = ((cell.0 as i32 + dx) as u32, (cell.1 as i32 + dy) as u32);
            if walkable(next) && !distances.contains_key(&next) {
                distances.insert(next, steps + 1);
                queue.push_back(next);
            }
        }
    }
    farthest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_gaps_between_facing_spots_answer_taps() {
        // Every gap is deep with spots facing across it; the channel also
        // carries deep cells that are not gaps.
        for seed in 0..10u64 {
            let maze = Maze::with_channel(9, 7, seed, 0.3, 1);
            let mut gaps = 0;
            let mut deep = 0;
            for y in 0..maze.height() {
                for x in 0..maze.width() {
                    if maze.is_deep(x, y) {
                        deep += 1;
                    }
                    if maze.is_gap(x, y) {
                        gaps += 1;
                        assert!(maze.is_deep(x, y));
                    }
                }
            }
            assert!(deep > gaps, "not every deep cell is a gap, seed {seed}");
            assert!(gaps >= 3, "the chain plus the decoy leave gaps, seed {seed}");
        }
    }

    #[test]
    fn the_exit_waits_on_the_far_bank() {
        // The bridge level's exit is beyond the channel: no walk reaches
        // it until the planks are down.
        for seed in 0..10u64 {
            let maze = Maze::with_channel(9, 7, seed, 0.3, 1);
            let exit = far_exit(&maze, (0, 0));
            assert!(!maze.is_deep(exit.0, exit.1));
            assert!(
                maze.distances_from((0, 0))[(exit.1 * maze.width() + exit.0) as usize].is_none(),
                "the exit lies beyond the channel"
            );
        }
    }

    #[test]
    fn the_waddle_needs_the_chain_planked() {
        for seed in 0..10u64 {
            let maze = Maze::with_channel(9, 7, seed, 0.3, 1);
            let exit = far_exit(&maze, (0, 0));
            // Nothing planked: the banks are apart…
            assert!(waddle_path(&maze, &HashSet::new(), (0, 0), exit).is_none());
            // …plank every gap and the way across opens.
            let gaps: HashSet<(u32, u32)> = (0..maze.height())
                .flat_map(|y| (0..maze.width()).map(move |x| (x, y)))
                .filter(|&(x, y)| maze.is_gap(x, y))
                .collect();
            assert!(waddle_path(&maze, &gaps, (0, 0), exit).is_some());
        }
    }
}
