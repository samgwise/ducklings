//! The mother duck: buffered input from keys, D-pad and clicks/taps,
//! one-tile hops with a smooth lerp, and the path history her ducklings
//! follow.

use std::collections::VecDeque;

use bevy::input::gamepad::{Gamepad, GamepadButton};
use bevy::prelude::*;

use maze::{Direction, Maze};

use crate::audio::SfxEvent;
use crate::bridge::{self, PlankStock, Planked};
use crate::ducklings::{Clutch, TrailStranded};
use crate::level::GamePhase;
use crate::world::{MainCamera, MazeRes, TILE_SIZE, cell_world_pos};

/// Duration of one hop, in seconds.
pub const HOP_SECS: f32 = 0.15;

/// Duration of one stream-carried hop, in seconds — the current is fast,
/// roughly double walking speed.
pub const STREAM_HOP_SECS: f32 = 0.07;

/// Duration of one flight over a fallen log, in seconds — slower and
/// weightier than a hop.
pub const FLIGHT_HOP_SECS: f32 = 0.24;

/// Peak height of a flight's arc, in world units (pixels).
pub const FLIGHT_ARC: f32 = 16.0;

/// Peak size gain mid-flight — she's off the ground.
pub const FLIGHT_SCALE: f32 = 0.3;

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
    /// How long this hop takes — stream hops are faster than walking,
    /// flights over logs slower.
    duration: f32,
    /// The cell committed when the hop lands.
    to_cell: (u32, u32),
    /// A flight over a fallen log: the hop arcs and she grows mid-air.
    flight: bool,
}

/// The duck's completed cells, oldest first — the trail ducklings follow.
#[derive(Component, Default)]
pub struct PathHistory(pub Vec<(u32, u32)>);

/// A queued move, consumed when the current hop ends so quick turns chain.
#[derive(Component, Default)]
pub struct BufferedMove(pub Option<Direction>);

/// Pending hop directions from a click/tap, consumed one per hop: the walk
/// of the tapped corridor.
#[derive(Component, Default)]
pub struct WalkPlan {
    pub queue: VecDeque<Direction>,
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
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn hop(
    time: Res<Time>,
    maze_res: Res<MazeRes>,
    clutch: Res<Clutch>,
    mut sfx: MessageWriter<SfxEvent>,
    mut strand_trail: MessageWriter<TrailStranded>,
    mut planked: ResMut<Planked>,
    mut stock: ResMut<PlankStock>,
    plank_meshes: Option<Res<bridge::PlankMesh>>,
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
        hop.progress += time.delta_secs() / hop.duration;
        if hop.progress < 1.0 {
            let mut at = hop.from.lerp(hop.to, hop.progress);
            // A flight arcs over its log — she rises, grows, and settles.
            if hop.flight {
                let lift = (hop.progress * core::f32::consts::PI).sin();
                at.y += lift * FLIGHT_ARC;
                transform.scale = Vec3::splat(1.0 + lift * FLIGHT_SCALE);
            }
            transform.translation = at.extend(transform.translation.z);
            return;
        }
        // Land: commit the cell, record the trail, cap the history.
        let from_flow = maze_res.maze.flow_at(duck.cell.0, duck.cell.1).is_some();
        transform.translation = hop.to.extend(transform.translation.z);
        transform.scale = Vec3::ONE;
        duck.cell = hop.to_cell;
        history.0.push(hop.to_cell);
        if history.0.len() > HISTORY_CAP {
            history.0.remove(0);
        }
        commands.entity(entity).remove::<Hop>();
        sfx.write(SfxEvent::Hop);
        // Crossing the waterline either way gets a splash.
        let to_flow = maze_res
            .maze
            .flow_at(hop.to_cell.0, hop.to_cell.1)
            .is_some();
        if from_flow != to_flow {
            sfx.write(SfxEvent::Splash);
        }
    }
    let flow = maze_res.maze.flow_at(duck.cell.0, duck.cell.1);
    // A manual press cancels any queued walk: latest intent wins.
    let manual = buffered.0.take().inspect(|_| plan.queue.clear());
    let (dir, duration) = match (manual, flow) {
        // On dry land a press walks its corridor — through bends to the
        // next decision point, like a tap. A press toward a gap lays a
        // plank instead: the deep water cannot be walked or ridden, so
        // the press has one sensible effect. A blocked first step leaves
        // an empty plan and the hop below bumps.
        (Some(dir), None) => {
            let (gdx, gdy) = dir.delta();
            let gap = (
                (duck.cell.0 as i32 + gdx) as u32,
                (duck.cell.1 as i32 + gdy) as u32,
            );
            if maze_res.maze.is_gap(gap.0, gap.1)
                && !planked.0.contains(&gap)
                && stock.left > 0
                && let Some(meshes) = plank_meshes.as_ref()
            {
                bridge::lay(
                    &mut commands,
                    &mut planked,
                    &mut stock,
                    meshes,
                    (maze_res.maze.width(), maze_res.maze.height()),
                    gap,
                );
                return;
            }
            plan.queue = corridor_walk(&maze_res.maze, &planked, duck.cell, dir)
                .unwrap_or_default()
                .into();
            (plan.queue.pop_front().unwrap_or(dir), HOP_SECS)
        }
        (None, None) => match plan.queue.pop_front() {
            Some(dir) => (dir, HOP_SECS),
            None => return,
        },
        // Riding: only an open side branch may hop out — never along the
        // flow (the current is faster) nor back upstream (one-way water).
        (Some(dir), Some(f))
            if dir != f
                && dir != f.opposite()
                && maze_res
                    .maze
                    .is_open(duck.cell.0 as i32, duck.cell.1 as i32, dir) =>
        {
            (dir, HOP_SECS)
        }
        // Otherwise the current carries her — sweeping away any queued
        // walk whose steps have gone stale — unless the queue's own next
        // step is a valid hop out of the current: a tap across the stream
        // while riding. Touch players have no direction keys, so that tap
        // is their only hop-out at junctions (and the rest of the walk
        // keeps its bearing); a tap along the current just rides.
        (_, Some(f)) => {
            let hop_out = ride_step(
                f,
                plan.queue.front().copied(),
                |dir| {
                    let (x, y) = duck.cell;
                    maze_res.maze.is_open(x as i32, y as i32, dir)
                },
            );
            if hop_out.is_none() {
                plan.queue.clear();
            }
            match hop_out {
                Some(dir) => (dir, HOP_SECS),
                None => (f, STREAM_HOP_SECS),
            }
        }
    };
    let (x, y) = duck.cell;
    let (dx, dy) = dir.delta();
    let to_cell = ((x as i32 + dx) as u32, (y as i32 + dy) as u32);
    if !maze_res.maze.is_open(x as i32, y as i32, dir)
        || (maze_res.maze.is_deep(to_cell.0, to_cell.1) && !planked.0.contains(&to_cell))
    {
        // A bump clears the buffer so a held key never machine-guns thuds.
        sfx.write(SfxEvent::Bump);
        return;
    }
    // Mum always spreads her wings over a fallen log — a slower, arcing
    // hop with a flutter — but the clutch cannot follow: flying with
    // ducklings in tow strands the whole trail on her take-off side.
    let flight = maze_res.maze.is_logged(x as i32, y as i32, dir);
    if flight {
        sfx.write(SfxEvent::Flutter);
        if clutch.collected > 0 {
            strand_trail.write(TrailStranded);
        }
    }
    let cells = (maze_res.maze.width(), maze_res.maze.height());
    duck.facing = facing_angle(dir);
    transform.rotation = Quat::from_rotation_z(duck.facing);
    commands.entity(entity).insert(Hop {
        from: transform.translation.xy(),
        to: cell_world_pos(cells, to_cell, TILE_SIZE),
        progress: 0.0,
        duration: if flight { FLIGHT_HOP_SECS } else { duration },
        to_cell,
        flight,
    });
}

/// The hop a water-bound duck takes with no direction keys pressed: the
/// queued step if it is itself a valid hop out of the current (a tap
/// across the stream — touch players' only junction hop-out), else `None`
/// and the current carries her.
fn ride_step(
    flow: Direction,
    queued: Option<Direction>,
    side_open: impl Fn(Direction) -> bool,
) -> Option<Direction> {
    queued.filter(|&dir| dir != flow && dir != flow.opposite() && side_open(dir))
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

/// Click or tap: queue the corridor walk toward the point — the corridor
/// ending nearest the tap, through bends to the next decision point. A tap
/// that gets no closer to the point is refused on the spot.
#[allow(clippy::too_many_arguments)]
fn pointer_walk(
    clicks: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    mut sfx: MessageWriter<SfxEvent>,
    mut commands: Commands,
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    maze_res: Res<MazeRes>,
    planked: Res<Planked>,
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
    let steps = walk_plan(&maze_res.maze, &planked, duck.cell, cell);
    if steps.is_empty() && cell != duck.cell {
        // No corridor ends any closer to the tap: refuse on the spot.
        sfx.write(SfxEvent::Bump);
        commands.entity(duck_entity).insert(HeadShake::default());
        plan.queue.clear();
    } else {
        plan.queue = steps.into();
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
pub fn world_to_cell(cells: (u32, u32), world: Vec2) -> (u32, u32) {
    // The tile grid is (2w+1) wide, so tile index from world is
    // `x / TILE + w`; cell (x, y) occupies tile (2x + 1, 2y + 1).
    let tile_x = (world.x / TILE_SIZE + cells.0 as f32).round() as i32;
    let tile_y = (world.y / TILE_SIZE + cells.1 as f32).round() as i32;
    (
        ((tile_x - 1).div_euclid(2)).clamp(0, cells.0 as i32 - 1) as u32,
        ((tile_y - 1).div_euclid(2)).clamp(0, cells.1 as i32 - 1) as u32,
    )
}

/// The corridor hops from `from` toward `target`: the tap picks the
/// corridor — of the walks open from her cell that PASS closer to the
/// tapped cell than she already is, the nearest-ending one — and that walk
/// runs through bends, never stopping mid-corridor, to its first decision
/// point: a branch offering a choice of ways, or a dead end. A tap with no
/// corridor passing closer — across a hedge, say — plans nothing and is
/// refused, and queued walks never fly logs or enter the water: those
/// special moves take a deliberate ask.
fn walk_plan(
    maze: &Maze,
    planked: &Planked,
    from: (u32, u32),
    target: (u32, u32),
) -> Vec<Direction> {
    if from == target {
        // A tap on the duck herself plans nothing.
        return Vec::new();
    }
    // A special move — a log flight or a step into the current — needs a
    // deliberate ask: the tap aimed straight at its own cell confirms it,
    // and the move is that single hop.
    let dir = dominant_axis(from, target);
    let (dx, dy) = dir.delta();
    let next = ((from.0 as i32 + dx) as u32, (from.1 as i32 + dy) as u32);
    if target == next
        && (maze.is_logged(from.0 as i32, from.1 as i32, dir)
            || maze.flow_at(next.0, next.1).is_some())
    {
        return vec![dir];
    }
    let here = manhattan(from, target);
    let mut best: Option<(u32, Vec<Direction>)> = None;
    for dir in Direction::ALL {
        let Some(plan) = corridor_walk(maze, planked, from, dir) else {
            continue;
        };
        // The corridor hit box: a walk qualifies when it passes closer to
        // the tapped cell than she already is — anywhere along it, so a
        // ring stop short of the tap still counts.
        if closest_approach(from, &plan, target) >= here {
            continue;
        }
        let distance = manhattan(walk_end(from, &plan), target);
        if best
            .as_ref()
            .is_none_or(|(best_distance, _)| distance < *best_distance)
        {
            best = Some((distance, plan));
        }
    }
    best.map(|(_, plan)| plan).unwrap_or_default()
}

/// The closest a corridor walk passes to `target`, in manhattan steps.
fn closest_approach(from: (u32, u32), plan: &[Direction], target: (u32, u32)) -> u32 {
    let (mut x, mut y) = (from.0 as i32, from.1 as i32);
    let mut closest = manhattan(from, target);
    for step in plan {
        let (dx, dy) = step.delta();
        x += dx;
        y += dy;
        closest = closest.min(manhattan((x as u32, y as u32), target));
    }
    closest
}

/// The manhattan distance between two cells.
fn manhattan(a: (u32, u32), b: (u32, u32)) -> u32 {
    a.0.abs_diff(b.0) + a.1.abs_diff(b.1)
}

/// The dominant axis from `from` toward `target` — ties lean east/west.
fn dominant_axis(from: (u32, u32), target: (u32, u32)) -> Direction {
    let dx = target.0 as i32 - from.0 as i32;
    let dy = target.1 as i32 - from.1 as i32;
    if dx.abs() >= dy.abs() {
        if dx >= 0 {
            Direction::East
        } else {
            Direction::West
        }
    } else if dy >= 0 {
        Direction::North
    } else {
        Direction::South
    }
}

/// The cell a corridor walk ends on.
fn walk_end(from: (u32, u32), plan: &[Direction]) -> (u32, u32) {
    let (mut x, mut y) = (from.0 as i32, from.1 as i32);
    for step in plan {
        let (dx, dy) = step.delta();
        x += dx;
        y += dy;
    }
    (x as u32, y as u32)
}

/// The walk from `from` heading `dir`: through bends — never stopping
/// mid-corridor — until the first decision point: a branch offering a
/// choice of ways, or a dead end. A queued walk never flies a log or steps
/// into the current: those need a deliberate move, so the walk ends at the
/// log and the water's edge.
fn corridor_walk(
    maze: &Maze,
    planked: &Planked,
    from: (u32, u32),
    mut dir: Direction,
) -> Option<Vec<Direction>> {
    if !maze.is_open(from.0 as i32, from.1 as i32, dir)
        || maze.is_logged(from.0 as i32, from.1 as i32, dir)
        || maze.is_deep(from.0, from.1)
    {
        return None;
    }
    let (fx, fy) = dir.delta();
    let first = ((from.0 as i32 + fx) as u32, (from.1 as i32 + fy) as u32);
    if maze.flow_at(first.0, first.1).is_some()
        || (maze.is_deep(first.0, first.1) && !planked.0.contains(&first))
    {
        return None;
    }
    let mut plan = Vec::new();
    let mut visited = vec![from];
    let (mut x, mut y) = (from.0 as i32, from.1 as i32);
    loop {
        if maze.is_logged(x, y, dir) {
            break;
        }
        let (dx, dy) = dir.delta();
        let next = ((x + dx) as u32, (y + dy) as u32);
        if maze.flow_at(next.0, next.1).is_some()
            || (maze.is_deep(next.0, next.1) && !planked.0.contains(&next))
        {
            break;
        }
        x += dx;
        y += dy;
        if visited.contains(&(x as u32, y as u32)) {
            // A ring corridor would cycle forever: the loop's entry is the
            // decision point, and the walk stops short of it.
            break;
        }
        visited.push((x as u32, y as u32));
        plan.push(dir);
        // The ways onward from here, the way back excluded: one means the
        // corridor merely runs straight on or bends — keep going — none or
        // several means a dead end or a branch, and the player chooses
        // again.
        let onward: Vec<Direction> = Direction::ALL
            .into_iter()
            .filter(|&d| d != dir.opposite())
            .filter(|&d| maze.is_open(x, y, d))
            .collect();
        match onward.as_slice() {
            &[only] => dir = only,
            _ => break,
        }
    }
    Some(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_plans_run_to_the_next_decision_point() {
        // The tapped corridor's walk runs through bends and stops at the
        // first decision point — a branch with a choice of ways, or a dead
        // end — never mid-corridor, and never revisiting a cell.
        let maze = Maze::generate(7, 7, 7, 0.0);
        let planked = Planked::default();
        for y in 0..7u32 {
            for x in 0..7u32 {
                let from = (2u32, 2u32);
                if (x, y) == from {
                    continue;
                }
                let plan = walk_plan(&maze, &planked, from, (x, y));
                // A tap that gets no closer is refused; the rest walks.
                if plan.is_empty() {
                    continue;
                }
                // Walk the plan, checking each cell it passes.
                let (mut cx, mut cy) = from;
                let mut visited = vec![from];
                for (i, &step) in plan.iter().enumerate() {
                    let (dx, dy) = step.delta();
                    cx = (cx as i32 + dx) as u32;
                    cy = (cy as i32 + dy) as u32;
                    assert!(!visited.contains(&(cx, cy)), "the walk never revisits");
                    visited.push((cx, cy));
                    let onward: Vec<Direction> = Direction::ALL
                        .into_iter()
                        .filter(|&d| d != step.opposite())
                        .filter(|&d| maze.is_open(cx as i32, cy as i32, d))
                        .collect();
                    if i + 1 == plan.len() {
                        // The walk ends here: a branch or a dead end.
                        assert!(
                            onward.len() != 1,
                            "the walk stops at a decision point, not mid-corridor"
                        );
                    } else {
                        assert_eq!(onward.len(), 1, "the walk continues through bends");
                        assert_eq!(onward[0], plan[i + 1], "each step follows the way on");
                    }
                }
            }
        }
    }

    #[test]
    fn walk_plans_pick_the_corridor_nearest_their_tap() {
        // Of the corridors open from her cell, the walk ends nearest the
        // tapped cell — a click on a duckling across a bend follows the
        // corridor that leads to it, not whichever axis happened to lead.
        for seed in 0..10u64 {
            let maze = Maze::generate(9, 7, seed, 0.3);
            let planked = Planked::default();
            let from = ((seed % 7) as u32, ((seed / 2) % 5) as u32);
            let target = ((from.0 + 3).min(8), (from.1 + 2).min(6));
            if from == target {
                continue;
            }
            let plan = walk_plan(&maze, &planked, from, target);
            let here = manhattan(from, target);
            if plan.is_empty() {
                // Refused: no corridor passes any closer than she already is.
                for dir in Direction::ALL {
                    if let Some(alternative) = corridor_walk(&maze, &planked, from, dir) {
                        assert!(
                            closest_approach(from, &alternative, target) >= here,
                            "a refused tap has no closer-passing corridor"
                        );
                    }
                }
                continue;
            }
            // A walked tap passes closer to the tapped cell than she was…
            assert!(
                closest_approach(from, &plan, target) < here,
                "the walk passes closer to the tap"
            );
            // …and ends nearer than any other qualifying corridor's.
            let best = manhattan(walk_end(from, &plan), target);
            for dir in Direction::ALL {
                if let Some(alternative) = corridor_walk(&maze, &planked, from, dir) {
                    if closest_approach(from, &alternative, target) < here {
                        assert!(
                            best <= manhattan(walk_end(from, &alternative), target),
                            "the walk picks the qualifying corridor ending nearest the tap"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn taps_on_adjacent_corridors_are_never_denied() {
        // A click on any cell of a corridor open from her cell — even in a
        // ringy maze where the walk stops at a ring short of the tap —
        // walks that corridor.
        for seed in 0..10u64 {
            let maze = Maze::generate(9, 7, seed, 0.8);
            let planked = Planked::default();
            for y in 0..maze.height() {
                for x in 0..maze.width() {
                    for dir in Direction::ALL {
                        let Some(walk) = corridor_walk(&maze, &planked, (x, y), dir) else {
                            continue;
                        };
                        let (mut cx, mut cy) = (x as i32, y as i32);
                        for step in &walk {
                            let (dx, dy) = step.delta();
                            cx += dx;
                            cy += dy;
                            let plan = walk_plan(&maze, &planked, (x, y), (cx as u32, cy as u32));
                            assert!(
                                !plan.is_empty(),
                                "a tap at {cx},{cy} from {x},{y} walks a corridor"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn corridor_walks_run_to_the_next_decision_point() {
        // The shared engine of taps and presses: a walk follows the single
        // way onward through bends and stops at the first decision point —
        // a branch with a choice of ways, or a dead end — never revisiting
        // a cell.
        for seed in 0..10u64 {
            let maze = Maze::generate(9, 7, seed, 0.3);
                for y in 0..maze.height() {
                    for x in 0..maze.width() {
                        for dir in Direction::ALL {
                            let Some(plan) =
                                corridor_walk(&maze, &Planked::default(), (x, y), dir)
                            else {
                                continue;
                            };
                        assert_eq!(plan[0], dir, "the walk leads the pressed way");
                        let (mut cx, mut cy) = (x as i32, y as i32);
                        let mut visited = vec![(x, y)];
                        for (i, &step) in plan.iter().enumerate() {
                            let (dx, dy) = step.delta();
                            cx += dx;
                            cy += dy;
                            assert!(
                                !visited.contains(&(cx as u32, cy as u32)),
                                "the walk never revisits"
                            );
                            visited.push((cx as u32, cy as u32));
                            let onward: Vec<Direction> = Direction::ALL
                                .into_iter()
                                .filter(|&d| d != step.opposite())
                                .filter(|&d| maze.is_open(cx, cy, d))
                                .collect();
                            if i + 1 == plan.len() {
                                // A branch or dead end — or a ring closure:
                                // the one way onward lands where she has
                                // already been.
                                if onward.len() == 1 {
                                    let (dx, dy) = onward[0].delta();
                                    let next = ((cx + dx) as u32, (cy + dy) as u32);
                                    assert!(
                                        visited.contains(&next),
                                        "the walk stops at a decision point or ring"
                                    );
                                }
                            } else {
                                assert_eq!(
                                    onward.len(),
                                    1,
                                    "the walk continues through bends"
                                );
                                assert_eq!(onward[0], plan[i + 1]);
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn walk_plans_terminate_on_ring_corridors() {
        // A corridor that loops back on itself must not walk forever: the
        // plan stops before revisiting a cell. Heavy braiding makes rings.
        for seed in 0..30u64 {
            let maze = Maze::generate(9, 7, seed, 0.8);
            let planked = Planked::default();
            for y in 0..maze.height() {
                for x in 0..maze.width() {
                    let plan = walk_plan(&maze, &planked, (x, y), (0, 0));
                    let (mut cx, mut cy) = (x as i32, y as i32);
                    let mut visited = vec![(x, y)];
                    for step in &plan {
                        let (dx, dy) = step.delta();
                        cx += dx;
                        cy += dy;
                        assert!(
                            !visited.contains(&(cx as u32, cy as u32)),
                            "the walk from {x},{y} never revisits"
                        );
                        visited.push((cx as u32, cy as u32));
                    }
                }
            }
        }
    }

    #[test]
    fn a_tap_across_the_current_hops_out_of_the_ride() {
        let open = |dir: Direction| dir == Direction::North;
        // A tap across the stream (here: north) is taken as the hop-out…
        assert_eq!(
            ride_step(Direction::East, Some(Direction::North), open),
            Some(Direction::North)
        );
        // …a tap along the current rides, upstream is refused, and a
        // hopped-shut side branch is refused too.
        assert_eq!(ride_step(Direction::East, Some(Direction::East), open), None);
        assert_eq!(ride_step(Direction::East, Some(Direction::West), open), None);
        assert_eq!(ride_step(Direction::East, Some(Direction::South), open), None);
        // No tap: the current carries her.
        assert_eq!(ride_step(Direction::East, None, open), None);
    }

    #[test]
    fn clicking_the_duck_plans_nothing() {
        let maze = Maze::generate(5, 5, 1, 0.0);
        assert!(walk_plan(&maze, &Planked::default(), (2, 2), (2, 2)).is_empty());
    }

    #[test]
    fn corridor_walks_stop_before_a_log() {
        // A queued walk never plans the flight: the log ends the walk, and
        // crossing takes a deliberate move.
        for seed in 0..20u64 {
            let maze = Maze::with_obstacles(11, 9, seed, 0.2, 0, 3);
            let Some((from, dir)) = logged_passage(&maze) else {
                continue;
            };
            assert!(corridor_walk(&maze, &Planked::default(), from, dir).is_none());
            return;
        }
        panic!("no seed in 0..20 laid a log to stop before");
    }

    #[test]
    fn corridor_walks_never_fly_logs_or_enter_water() {
        // No step of a queued walk crosses a logged passage or lands on
        // the water — those moves are confirmations, never automatic.
        for seed in 0..10u64 {
            let maze = Maze::with_obstacles(9, 7, seed, 0.3, 2, 2);
            let planked = Planked::default();
            for y in 0..maze.height() {
                for x in 0..maze.width() {
                    for dir in Direction::ALL {
                        let Some(plan) = corridor_walk(&maze, &planked, (x, y), dir) else {
                            continue;
                        };
                        let (mut cx, mut cy) = (x as i32, y as i32);
                        for step in &plan {
                            assert!(!maze.is_logged(cx, cy, *step), "no step flies a log");
                            let (dx, dy) = step.delta();
                            cx += dx;
                            cy += dy;
                            assert!(
                                maze.flow_at(cx as u32, cy as u32).is_none(),
                                "no step enters the water"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn special_moves_need_a_deliberate_ask() {
        // A log flight or a step into the current is confirmed by aiming
        // straight at it — the tapped cell is the move's own target — and
        // is that single hop; a tap past it never flies the log.
        for seed in 0..20u64 {
            let maze = Maze::with_obstacles(11, 9, seed, 0.2, 0, 3);
            let Some((from, dir)) = logged_passage(&maze) else {
                continue;
            };
            let (dx, dy) = dir.delta();
            let across = ((from.0 as i32 + dx) as u32, (from.1 as i32 + dy) as u32);
            let beyond = ((across.0 as i32 + dx) as u32, (across.1 as i32 + dy) as u32);
            assert_eq!(
                walk_plan(&maze, &Planked::default(), from, across),
                vec![dir],
                "the tap on the far tile confirms the flight"
            );
            // A tap past the log either stops short or takes another
            // corridor — never the logged step.
            let plan = walk_plan(&maze, &Planked::default(), from, beyond);
            let (mut px, mut py) = (from.0 as i32, from.1 as i32);
            for step in &plan {
                assert!(!maze.is_logged(px, py, *step));
                let (sdx, sdy) = step.delta();
                px += sdx;
                py += sdy;
            }
            // The water too: a tap on the water cell itself rides.
            let maze = Maze::with_streams(9, 7, seed, 0.3, 2);
            let mut found = None;
            for y in 0..maze.height() {
                for x in 0..maze.width() {
                    if maze.flow_at(x, y).is_some() {
                        continue;
                    }
                    for dir in Direction::ALL {
                        if !maze.is_open(x as i32, y as i32, dir) {
                            continue;
                        }
                        let (dx, dy) = dir.delta();
                        let next = ((x as i32 + dx) as u32, (y as i32 + dy) as u32);
                        if maze.flow_at(next.0, next.1).is_some() {
                            found = Some(((x, y), next, dir));
                        }
                    }
                }
            }
            let Some((bank, water_cell, dir)) = found else {
                continue;
            };
            assert_eq!(
                walk_plan(&maze, &Planked::default(), bank, water_cell),
                vec![dir],
                "the tap on the water confirms the ride"
            );
            return;
        }
        panic!("no seed in 0..20 laid a log or water to confirm");
    }

    /// The first logged passage found, as (west/south cell, direction).
    fn logged_passage(maze: &Maze) -> Option<((u32, u32), Direction)> {
        for dir in [Direction::East, Direction::North] {
            for y in 0..maze.height() {
                for x in 0..maze.width() {
                    if maze.is_logged(x as i32, y as i32, dir) {
                        return Some(((x, y), dir));
                    }
                }
            }
        }
        None
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
