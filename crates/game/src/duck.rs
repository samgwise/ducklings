//! The mother duck: buffered input from keys, D-pad and clicks/taps,
//! one-tile hops with a smooth lerp, and the path history her ducklings
//! follow.

use std::collections::VecDeque;

use bevy::input::gamepad::{Gamepad, GamepadButton};
use bevy::prelude::*;

use maze::{Direction, Maze};

use crate::audio::SfxEvent;
use crate::level::GamePhase;
use crate::world::{MainCamera, MazeRes, TILE_SIZE, cell_world_pos};

/// Duration of one hop, in seconds.
pub const HOP_SECS: f32 = 0.15;

/// Path history entries kept per level — long enough for any conga line.
pub const HISTORY_CAP: usize = 512;

/// The mother duck.
#[derive(Component)]
pub struct Duck {
    /// Current maze cell.
    pub cell: (u32, u32),
    /// Facing yaw in radians — the beak swings around as she hops.
    pub facing: f32,
}

/// Mid-hop animation between two world points; removed on landing.
#[derive(Component)]
pub struct Hop {
    pub from: Vec2,
    pub to: Vec2,
    /// 0–1 through the hop.
    pub progress: f32,
    /// The cell committed when the hop lands.
    to_cell: (u32, u32),
}

/// The duck's completed cells, oldest first — the trail ducklings follow.
#[derive(Component, Default)]
pub struct PathHistory(pub Vec<(u32, u32)>);

/// A queued move, consumed when the current hop ends so quick turns chain.
#[derive(Component, Default)]
pub struct BufferedMove(pub Option<Direction>);

/// Pending hop directions from a click/tap, consumed one per hop, with the
/// clicked cell the walk is aiming at — the out-of-range cue reads it when
/// the corridor ends short.
#[derive(Component, Default)]
pub struct WalkPlan {
    pub queue: VecDeque<Direction>,
    pub target: Option<(u32, u32)>,
}

/// An out-of-range head shake in progress.
#[derive(Component, Default)]
pub struct HeadShake {
    progress: f32,
}

/// How long the out-of-range head shake lasts, in seconds.
const SHAKE_SECS: f32 = 0.36;
/// Peak yaw swing of the shake, in radians.
const SHAKE_AMPLITUDE: f32 = 0.6;
/// How many full beak-wags fit in the shake.
const SHAKE_WAGS: f32 = 3.0;

pub struct DuckPlugin;

impl Plugin for DuckPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (pointer_walk, buffer_input, hop, head_shake)
                .chain()
                .run_if(in_state(GamePhase::Playing)),
        );
    }
}

/// Capture the latest pressed direction from the keyboard or any connected
/// pad's D-pad. Overwriting a queued move with a newer one feels right:
/// intent, not a queue.
fn buffer_input(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut buffered: Single<&mut BufferedMove>,
) {
    if let Some(dir) = keyboard_direction(&keys).or_else(|| pad_direction(&pads)) {
        buffered.0 = Some(dir);
    }
}

/// The keyboard direction pressed this frame, if any.
fn keyboard_direction(keys: &ButtonInput<KeyCode>) -> Option<Direction> {
    if keys.just_pressed(KeyCode::KeyW) || keys.just_pressed(KeyCode::ArrowUp) {
        Some(Direction::North)
    } else if keys.just_pressed(KeyCode::KeyS) || keys.just_pressed(KeyCode::ArrowDown) {
        Some(Direction::South)
    } else if keys.just_pressed(KeyCode::KeyD) || keys.just_pressed(KeyCode::ArrowRight) {
        Some(Direction::East)
    } else if keys.just_pressed(KeyCode::KeyA) || keys.just_pressed(KeyCode::ArrowLeft) {
        Some(Direction::West)
    } else {
        None
    }
}

/// The D-pad direction just pressed on any connected pad, if any. Steam
/// Deck and Xbox pads both map their D-pads to these standard buttons.
fn pad_direction(pads: &Query<&Gamepad>) -> Option<Direction> {
    const D_PAD: [(GamepadButton, Direction); 4] = [
        (GamepadButton::DPadUp, Direction::North),
        (GamepadButton::DPadDown, Direction::South),
        (GamepadButton::DPadRight, Direction::East),
        (GamepadButton::DPadLeft, Direction::West),
    ];
    pads.iter().find_map(|pad| {
        D_PAD
            .iter()
            .find(|&(button, _)| pad.just_pressed(*button))
            .map(|&(_, dir)| dir)
    })
}

/// Drive hops: advance the lerp, land (committing the cell and trail), then
/// start the next hop from the buffer — all in the frame the last one ends.
#[allow(clippy::type_complexity)]
fn hop(
    time: Res<Time>,
    maze_res: Res<MazeRes>,
    mut sfx: MessageWriter<SfxEvent>,
    mut commands: Commands,
    mover: Single<
        (
            Entity,
            &mut Duck,
            &mut Transform,
            &mut BufferedMove,
            Option<&mut Hop>,
            &mut PathHistory,
            &mut WalkPlan,
        ),
        With<Duck>,
    >,
) {
    let (entity, mut duck, mut transform, mut buffered, hop, mut history, mut plan) =
        mover.into_inner();
    if let Some(mut hop) = hop {
        hop.progress += time.delta_secs() / HOP_SECS;
        if hop.progress < 1.0 {
            transform.translation = hop
                .from
                .lerp(hop.to, hop.progress)
                .extend(transform.translation.z);
            return;
        }
        // Land: commit the cell, record the trail, cap the history.
        transform.translation = hop.to.extend(transform.translation.z);
        duck.cell = hop.to_cell;
        history.0.push(hop.to_cell);
        if history.0.len() > HISTORY_CAP {
            history.0.remove(0);
        }
        commands.entity(entity).remove::<Hop>();
        sfx.write(SfxEvent::Hop);
        // The walk ended short of the click: refuse in place.
        if plan.queue.is_empty()
            && let Some(target) = plan.target.take()
            && duck.cell != target
        {
            sfx.write(SfxEvent::Bump);
            commands.entity(entity).insert(HeadShake::default());
        }
    }
    // A manual press cancels any queued walk: latest intent wins.
    let next = buffered.0.take().inspect(|_| {
        plan.queue.clear();
        plan.target = None;
    });
    let Some(dir) = next.or_else(|| plan.queue.pop_front()) else {
        return;
    };
    let (x, y) = duck.cell;
    if !maze_res.maze.is_open(x as i32, y as i32, dir) {
        // A bump clears the buffer so a held key never machine-guns thuds.
        sfx.write(SfxEvent::Bump);
        return;
    }
    let (dx, dy) = dir.delta();
    let to_cell = ((x as i32 + dx) as u32, (y as i32 + dy) as u32);
    let cells = (maze_res.maze.width(), maze_res.maze.height());
    duck.facing = facing_angle(dir);
    transform.rotation = Quat::from_rotation_z(duck.facing);
    commands.entity(entity).insert(Hop {
        from: transform.translation.xy(),
        to: cell_world_pos(cells, to_cell, TILE_SIZE),
        progress: 0.0,
        to_cell,
    });
}

/// World-space yaw a hop in `dir` points the duck (east is zero).
fn facing_angle(dir: Direction) -> f32 {
    match dir {
        Direction::North => core::f32::consts::FRAC_PI_2,
        Direction::East => 0.0,
        Direction::South => -core::f32::consts::FRAC_PI_2,
        Direction::West => core::f32::consts::PI,
    }
}

/// Click or tap: queue hops along the straight corridor toward the point —
/// as far as the click along one axis, never around corners.
#[allow(clippy::too_many_arguments)]
fn pointer_walk(
    clicks: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    mut sfx: MessageWriter<SfxEvent>,
    mut commands: Commands,
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    maze_res: Res<MazeRes>,
    target: Single<(Entity, &Duck, &mut WalkPlan)>,
) {
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
    let (duck_entity, duck, mut plan) = target.into_inner();
    let steps = walk_plan(&maze_res.maze, duck.cell, cell);
    if steps.is_empty() && cell != duck.cell {
        // Straight into a hedge: refuse on the spot.
        sfx.write(SfxEvent::Bump);
        commands.entity(duck_entity).insert(HeadShake::default());
        plan.queue.clear();
        plan.target = None;
    } else {
        plan.queue = steps.into();
        plan.target = Some(cell);
    }
}

/// The out-of-range head shake: the beak wags left and right, decaying,
/// while the duck otherwise stands still.
fn head_shake(
    time: Res<Time>,
    mut commands: Commands,
    shaker: Single<(Entity, &mut Duck, &mut Transform, &mut HeadShake)>,
) {
    let (entity, duck, mut transform, mut shake) = shaker.into_inner();
    shake.progress += time.delta_secs() / SHAKE_SECS;
    if shake.progress >= 1.0 {
        commands.entity(entity).remove::<HeadShake>();
        transform.rotation = Quat::from_rotation_z(duck.facing);
        return;
    }
    let decay = 1.0 - shake.progress;
    let offset =
        (shake.progress * SHAKE_WAGS * core::f32::consts::TAU).sin() * SHAKE_AMPLITUDE * decay;
    transform.rotation = Quat::from_rotation_z(duck.facing + offset);
}

/// The maze cell under a world point — hedges snap to their nearest cell,
/// and clicks beyond the maze clamp inside it.
fn world_to_cell(cells: (u32, u32), world: Vec2) -> (u32, u32) {
    // The tile grid is (2w+1) wide, so tile index from world is
    // `x / TILE + w`; cell (x, y) occupies tile (2x + 1, 2y + 1).
    let tile_x = (world.x / TILE_SIZE + cells.0 as f32).round() as i32;
    let tile_y = (world.y / TILE_SIZE + cells.1 as f32).round() as i32;
    (
        ((tile_x - 1).div_euclid(2)).clamp(0, cells.0 as i32 - 1) as u32,
        ((tile_y - 1).div_euclid(2)).clamp(0, cells.1 as i32 - 1) as u32,
    )
}

/// The straight-corridor hops from `from` toward `target`: the dominant
/// axis only, as far as the target along that axis, truncated at the first
/// hedge — a click never navigates the duck around corners.
fn walk_plan(maze: &Maze, from: (u32, u32), target: (u32, u32)) -> Vec<Direction> {
    let dx = target.0 as i32 - from.0 as i32;
    let dy = target.1 as i32 - from.1 as i32;
    let (dir, steps) = if dx.abs() >= dy.abs() {
        (
            if dx >= 0 {
                Direction::East
            } else {
                Direction::West
            },
            dx.abs(),
        )
    } else {
        (
            if dy >= 0 {
                Direction::North
            } else {
                Direction::South
            },
            dy.abs(),
        )
    };
    let mut plan = Vec::new();
    let (mut x, mut y) = (from.0 as i32, from.1 as i32);
    for _ in 0..steps {
        if !maze.is_open(x, y, dir) {
            break;
        }
        let (dx, dy) = dir.delta();
        x += dx;
        y += dy;
        plan.push(dir);
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The open run from (2, 2) along `dir`, counted from the maze itself.
    fn open_run(maze: &Maze, dir: Direction) -> i32 {
        let (dx, dy) = dir.delta();
        let (mut x, mut y) = (2i32, 2i32);
        let mut run = 0;
        while maze.is_open(x, y, dir) {
            x += dx;
            y += dy;
            run += 1;
        }
        run
    }

    #[test]
    fn walk_plans_follow_the_open_corridor_and_stop() {
        // A click past the hedge plans exactly the open run — never more.
        let maze = Maze::generate(7, 7, 7, 0.0);
        for dir in Direction::ALL {
            let (dx, dy) = dir.delta();
            let run = open_run(&maze, dir);
            let target = ((2 + dx * 5).max(0) as u32, (2 + dy * 5).max(0) as u32);
            let plan = walk_plan(&maze, (2, 2), target);
            assert_eq!(plan.len(), run as usize, "run {dir:?}");
            assert!(plan.iter().all(|&d| d == dir), "run {dir:?}");
        }
    }

    #[test]
    fn walk_plans_take_the_dominant_axis_only() {
        let maze = Maze::generate(9, 5, 3, 0.0);
        // A diagonal click never turns a corner: one axis only is walked.
        let plan = walk_plan(&maze, (4, 2), (6, 3));
        assert!(plan.iter().all(|&d| d == Direction::East));
        let plan = walk_plan(&maze, (4, 2), (5, 5));
        assert!(plan.iter().all(|&d| d == Direction::North));
    }

    #[test]
    fn clicking_the_duck_plans_nothing() {
        let maze = Maze::generate(5, 5, 1, 0.0);
        assert!(walk_plan(&maze, (2, 2), (2, 2)).is_empty());
    }

    #[test]
    fn world_to_cell_inverts_cell_world_pos() {
        let cells = (4u32, 3u32);
        for y in 0..cells.1 {
            for x in 0..cells.0 {
                let world = cell_world_pos(cells, (x, y), TILE_SIZE);
                assert_eq!(world_to_cell(cells, world), (x, y));
            }
        }
        // Clicks far outside the maze clamp inside it.
        assert_eq!(world_to_cell(cells, Vec2::new(-1000.0, -1000.0)), (0, 0));
        assert_eq!(world_to_cell(cells, Vec2::new(1000.0, 1000.0)), (3, 2));
    }
}
