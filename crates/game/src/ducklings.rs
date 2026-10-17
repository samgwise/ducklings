//! The clutch: ducklings waiting in the maze, the trail-following conga line
//! once collected, and pickup when mum hops onto them.

use bevy::prelude::*;

use maze::Maze;

use crate::audio::SfxEvent;
use crate::duck::{Duck, PathHistory, STREAM_HOP_SECS};
use crate::level::GamePhase;
use crate::world::{MazeRes, TILE_SIZE, cell_world_pos};

/// History steps between consecutive ducklings in the trail.
pub const TRAIL_SPACING: usize = 1;

/// Exponential smoothing rate for the trail (higher = tighter line).
pub const FOLLOW_SMOOTHING: f32 = 14.0;

/// Scale of a duckling sharing its cell with siblings — a slightly smaller
/// bird in a stack.
const STACK_SCALE: f32 = 0.8;

/// A duckling: where it waits until mum arrives, and its trail position
/// once collected.
#[derive(Component)]
pub struct Duckling {
    /// The maze cell the duckling waits on.
    pub cell: (u32, u32),
    /// Position in the trail — 0 right behind mum. Set on pickup.
    pub index: usize,
}

/// Still waiting in the maze; removed the moment mum collects the
/// duckling. Waiting ducklings caught in a stream get swept downstream.
#[derive(Component)]
pub struct Waiting;

/// A waiting duckling mid flow-hop along a stream.
#[derive(Component)]
struct Swept {
    from: Vec2,
    to: Vec2,
    /// The cell committed when the flow-hop completes.
    to_cell: (u32, u32),
    /// 0–1 through the flow-hop.
    progress: f32,
}

/// How many ducklings are in the trail and how many the level holds.
#[derive(Resource, Debug, Default)]
pub struct Clutch {
    pub collected: usize,
    pub total: usize,
}

pub struct DucklingsPlugin;

impl Plugin for DucklingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Messages<TrailStranded>>().add_systems(
            Update,
            (strand, sweep, pickup, follow, stack).run_if(in_state(GamePhase::Playing)),
        );
    }
}

/// Mum flew a fallen log with the clutch aboard: the trail cannot follow.
/// Written when the flight starts; the strand system detaches the line.
#[derive(Message, Debug)]
pub struct TrailStranded;

/// The cell duckling `index` drifts toward: `spacing × (index + 1)` steps
/// back along the duck's path history. `None` while the history is too
/// short to reach the slot — the duckling waits in place rather than
/// bunching up at the spawn.
pub fn trail_target(history: &[(u32, u32)], index: usize, spacing: usize) -> Option<(u32, u32)> {
    history
        .len()
        .checked_sub(1 + spacing * (index + 1))
        .map(|from_end| history[from_end])
}

/// One exponential-smoothing step toward `target` — frame-rate independent,
/// and never overshoots.
pub fn follow_step(current: Vec2, target: Vec2, delta_secs: f32, smoothing: f32) -> Vec2 {
    let t = 1.0 - (-smoothing * delta_secs).exp();
    current + (target - current) * t
}

/// Mum's cell changed: collect any duckling she landed on.
fn pickup(
    mut commands: Commands,
    duck: Single<&Duck, Changed<Duck>>,
    mut clutch: ResMut<Clutch>,
    waiting: Query<(Entity, &Duckling), With<Waiting>>,
    mut sfx: MessageWriter<SfxEvent>,
) {
    for (entity, duckling) in waiting.iter() {
        if duckling.cell != duck.cell {
            continue;
        }
        collect(&mut commands, entity, duckling.cell, clutch.collected);
        clutch.collected += 1;
        // Each sibling in the line chirps a little brighter than the last.
        sfx.write(SfxEvent::Chirp {
            speed: 1.0 + 0.06 * clutch.collected as f32,
        });
    }
}

/// Collect a duckling into the trail at `index`.
fn collect(commands: &mut Commands, entity: Entity, cell: (u32, u32), index: usize) {
    commands
        .entity(entity)
        .remove::<Waiting>()
        .insert(Duckling { cell, index });
}

/// Mum flew a log with the clutch aboard: the whole trail detaches — each
/// duckling waits at the slot it trailed toward, and the clutch count drops
/// to zero until she re-collects them.
fn strand(
    mut stranded: MessageReader<TrailStranded>,
    head: Single<&PathHistory, With<Duck>>,
    mut trail: Query<(Entity, &mut Duckling), Without<Waiting>>,
    mut commands: Commands,
    mut clutch: ResMut<Clutch>,
) {
    if stranded.read().next().is_none() {
        return;
    }
    for (entity, mut duckling) in trail.iter_mut() {
        // The slot always exists in practice — a duckling at index `i` was
        // collected after `i` others, so the history is long enough — but
        // a too-short history leaves it waiting where it was picked up.
        duckling.cell =
            trail_target(&head.0, duckling.index, TRAIL_SPACING).unwrap_or(duckling.cell);
        commands.entity(entity).insert(Waiting);
    }
    clutch.collected = 0;
}

/// Waiting ducklings caught in a stream get swept downstream, one fast
/// flow-hop at a time, until the let-off deposits them on dry ground.
fn sweep(
    time: Res<Time>,
    maze_res: Res<MazeRes>,
    mut commands: Commands,
    mut ducklings: Query<
        (Entity, &mut Duckling, &mut Transform, Option<&mut Swept>),
        With<Waiting>,
    >,
) {
    for (entity, mut duckling, mut transform, swept) in ducklings.iter_mut() {
        match swept {
            Some(mut swept) => {
                swept.progress += time.delta_secs() / STREAM_HOP_SECS;
                if swept.progress >= 1.0 {
                    transform.translation = swept.to.extend(transform.translation.z);
                    duckling.cell = swept.to_cell;
                    commands.entity(entity).remove::<Swept>();
                } else {
                    transform.translation = swept
                        .from
                        .lerp(swept.to, swept.progress)
                        .extend(transform.translation.z);
                }
            }
            None => {
                if let Some(flow) = maze_res.maze.flow_at(duckling.cell.0, duckling.cell.1) {
                    let (dx, dy) = flow.delta();
                    let to_cell = (
                        (duckling.cell.0 as i32 + dx) as u32,
                        (duckling.cell.1 as i32 + dy) as u32,
                    );
                    let cells = (maze_res.maze.width(), maze_res.maze.height());
                    commands.entity(entity).insert(Swept {
                        from: transform.translation.xy(),
                        to: cell_world_pos(cells, to_cell, TILE_SIZE),
                        to_cell,
                        progress: 0.0,
                    });
                }
            }
        }
    }
}

/// Every collected duckling drifts toward its slot in the conga line —
/// unless the current takes it: a slot on the water with mum clear of the
/// stream washes the duckling off the line, and the sweep carries it
/// downstream to the run's let-off.
#[allow(clippy::type_complexity)]
fn follow(
    time: Res<Time>,
    maze_res: Res<MazeRes>,
    head: Single<(&Duck, &PathHistory), With<Duck>>,
    mut clutch_line: Query<(Entity, &mut Duckling, &mut Transform), Without<Waiting>>,
    mut commands: Commands,
    mut clutch: ResMut<Clutch>,
) {
    let (duck, history) = head.into_inner();
    let riding = maze_res.maze.flow_at(duck.cell.0, duck.cell.1).is_some();
    let cells = (maze_res.maze.width(), maze_res.maze.height());
    for (entity, mut duckling, mut transform) in clutch_line.iter_mut() {
        let Some(target_cell) = trail_target(&history.0, duckling.index, TRAIL_SPACING) else {
            continue;
        };
        if washed_off(&maze_res.maze, riding, target_cell) {
            // The current takes it: it waits on the water, and the sweep
            // carries it to the let-off for mum to pluck or pick up later.
            duckling.cell = target_cell;
            commands.entity(entity).insert(Waiting);
            clutch.collected = clutch.collected.saturating_sub(1);
            continue;
        }
        // A duckling plucked from a stack returns to full size.
        transform.scale = Vec3::ONE;
        let target = cell_world_pos(cells, target_cell, TILE_SIZE);
        transform.translation = follow_step(
            transform.translation.xy(),
            target,
            time.delta_secs(),
            FOLLOW_SMOOTHING,
        )
        .extend(transform.translation.z);
    }
}

/// Whether the current takes this trail duckling: its slot is water while
/// mum is clear of the stream. In her wake the line holds — she breaks the
/// current — but a straggler trailing through water once she has stepped
/// off is swept away with it.
fn washed_off(maze: &Maze, riding: bool, slot: (u32, u32)) -> bool {
    !riding && maze.flow_at(slot.0, slot.1).is_some()
}

/// Waiting ducklings sharing a cell render as a little stack — two on the
/// diagonal, three in a pyramid, four in a grid — so a pile the river
/// deposits on one let-off cell reads as a group, not one duck.
#[allow(clippy::type_complexity)]
fn stack(
    maze_res: Res<MazeRes>,
    mut ducklings: Query<(Entity, &Duckling, &mut Transform), (With<Waiting>, Without<Swept>)>,
) {
    let cells = (maze_res.maze.width(), maze_res.maze.height());
    let mut members: Vec<((u32, u32), Entity)> = ducklings
        .iter()
        .map(|(entity, duckling, _)| (duckling.cell, entity))
        .collect();
    // Group by cell — ties by entity, so each bird keeps its perch from
    // frame to frame.
    members.sort();
    let mut start = 0;
    while start < members.len() {
        let cell = members[start].0;
        let end = start + members[start..].iter().take_while(|(c, _)| *c == cell).count();
        let offsets = stack_offsets(end - start);
        for (offset, &(_, entity)) in offsets.iter().zip(&members[start..end]) {
            let (_, _, mut transform) = ducklings.get_mut(entity).unwrap();
            let base = cell_world_pos(cells, cell, TILE_SIZE);
            transform.translation = (base + Vec2::new(offset.0, offset.1) * TILE_SIZE)
                .extend(transform.translation.z);
            transform.scale = if offsets.len() > 1 {
                Vec3::splat(STACK_SCALE)
            } else {
                Vec3::ONE
            };
        }
        start = end;
    }
}

/// The offsets — fractions of a tile — that lay `count` waiting ducklings
/// across one cell: one centred, two on the diagonal, three in a pyramid,
/// four in a grid, and a centred grid beyond.
fn stack_offsets(count: usize) -> Vec<(f32, f32)> {
    /// Offset of one member of a stack from the cell centre, tile fraction.
    const D: f32 = 0.22;
    match count {
        1 => vec![(0.0, 0.0)],
        2 => vec![(-D, -D), (D, D)],
        3 => vec![(0.0, D), (-D, -D), (D, -D)],
        4 => vec![(-D, -D), (D, -D), (-D, D), (D, D)],
        _ => {
            let cols = (count as f32).sqrt().ceil() as usize;
            let rows = count.div_ceil(cols);
            (0..count)
                .map(|i| {
                    let (col, row) = (i % cols, i / cols);
                    (
                        (col as f32 - (cols - 1) as f32 / 2.0) * 2.0 * D,
                        (row as f32 - (rows - 1) as f32 / 2.0) * 2.0 * D,
                    )
                })
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trail_targets_walk_back_the_history() {
        let history = vec![(0, 0), (1, 0), (1, 1), (2, 1)];
        // Duckling 0 with spacing 1 sits one step behind the head (the
        // head itself is history's last entry).
        assert_eq!(trail_target(&history, 0, 1), Some((1, 1)));
        assert_eq!(trail_target(&history, 1, 1), Some((1, 0)));
        // Spacing 2 doubles the gaps.
        assert_eq!(trail_target(&history, 0, 2), Some((1, 0)));
    }

    #[test]
    fn trail_slots_past_the_history_wait_in_place() {
        // More ducklings than history: the tail waits until mum has walked
        // enough steps for its slot to exist (index 1 needs three entries).
        let history = vec![(0, 0), (1, 0)];
        assert_eq!(trail_target(&history, 0, 1), Some((0, 0)));
        assert_eq!(trail_target(&history, 1, 1), None);
        assert_eq!(trail_target(&history, 5, 1), None);
    }

    #[test]
    fn trail_targets_wait_for_a_history() {
        // Before mum's first landing there is nothing to follow.
        assert_eq!(trail_target(&[], 0, 1), None);
    }

    /// A seeded maze carrying both water and dry land, with one cell of
    /// each marked.
    fn watery_maze() -> (Maze, (u32, u32), (u32, u32)) {
        (0..20u64)
            .find_map(|seed| {
                let maze = Maze::with_streams(7, 5, seed, 0.2, 2);
                let (mut water, mut dry) = (None, None);
                for y in 0..maze.height() {
                    for x in 0..maze.width() {
                        if water.is_none() && maze.flow_at(x, y).is_some() {
                            water = Some((x, y));
                        }
                        if dry.is_none() && maze.flow_at(x, y).is_none() {
                            dry = Some((x, y));
                        }
                    }
                }
                match (water, dry) {
                    (Some(water), Some(dry)) => Some((maze, water, dry)),
                    _ => None,
                }
            })
            .expect("some seed carries both water and dry land")
    }

    #[test]
    fn the_wash_takes_stragglers_only_when_mum_is_clear() {
        // Mum's wake holds the line while she rides; once she has stepped
        // off, a slot the current reaches is washed off the line, and a dry
        // slot never washes.
        let (maze, water, dry) = watery_maze();
        assert!(!washed_off(&maze, true, water), "the wake holds while riding");
        assert!(
            washed_off(&maze, false, water),
            "a water slot washes once mum is clear"
        );
        assert!(!washed_off(&maze, false, dry), "a dry slot never washes");
    }

    #[test]
    fn stacks_lay_out_their_count() {
        // A stack of one sits centred; every stack keeps its members on
        // distinct offsets — two on the diagonal, three in a pyramid, four
        // in a grid, a centred grid beyond.
        assert_eq!(stack_offsets(1), vec![(0.0, 0.0)]);
        for count in 2..=9 {
            let offsets = stack_offsets(count);
            assert_eq!(offsets.len(), count);
            for (i, a) in offsets.iter().enumerate() {
                for b in &offsets[i + 1..] {
                    assert_ne!(a, b, "a stack of {count} keeps its members apart");
                }
            }
        }
    }

    #[test]
    fn follow_steps_move_toward_the_target_without_overshooting() {
        let start = Vec2::ZERO;
        let target = Vec2::new(10.0, 4.0);
        let after = follow_step(start, target, 1.0 / 60.0, FOLLOW_SMOOTHING);
        assert!(after.x > 0.0 && after.x < target.x);
        assert!(after.y > 0.0 && after.y < target.y);
        // A long step lands close to — never past — the target.
        let after = follow_step(start, target, 10.0, FOLLOW_SMOOTHING);
        assert!(after.x <= target.x && after.y <= target.y);
        // Zero time means no movement.
        assert_eq!(follow_step(start, target, 0.0, FOLLOW_SMOOTHING), start);
    }
}
