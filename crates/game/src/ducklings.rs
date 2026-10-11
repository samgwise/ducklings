//! The clutch: ducklings waiting in the maze, the trail-following conga line
//! once collected, and pickup when mum hops onto them.

use bevy::prelude::*;

use crate::audio::SfxEvent;
use crate::duck::{Duck, PathHistory, STREAM_HOP_SECS};
use crate::level::GamePhase;
use crate::world::{MazeRes, TILE_SIZE, cell_world_pos};

/// History steps between consecutive ducklings in the trail.
pub const TRAIL_SPACING: usize = 1;

/// Exponential smoothing rate for the trail (higher = tighter line).
pub const FOLLOW_SMOOTHING: f32 = 14.0;

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
        app.add_systems(
            Update,
            (sweep, pickup, pluck, follow).run_if(in_state(GamePhase::Playing)),
        );
    }
}

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

/// A swept duckling's cell changed: if it passed onto or beside mum's
/// cell — the well-timed pluck — it joins the trail.
#[allow(clippy::type_complexity)]
fn pluck(
    mut commands: Commands,
    duck: Single<&Duck>,
    mut clutch: ResMut<Clutch>,
    swept: Query<(Entity, &Duckling), (Changed<Duckling>, With<Waiting>)>,
    mut sfx: MessageWriter<SfxEvent>,
) {
    for (entity, duckling) in swept.iter() {
        let dx = (duckling.cell.0 as i32 - duck.cell.0 as i32).abs();
        let dy = (duckling.cell.1 as i32 - duck.cell.1 as i32).abs();
        if dx + dy > 1 {
            continue;
        }
        collect(&mut commands, entity, duckling.cell, clutch.collected);
        clutch.collected += 1;
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

/// Every collected duckling drifts toward its slot in the conga line.
fn follow(
    time: Res<Time>,
    maze_res: Res<MazeRes>,
    history: Single<&PathHistory, With<Duck>>,
    mut clutch_line: Query<(&Duckling, &mut Transform), Without<Waiting>>,
) {
    let history = history.into_inner();
    let cells = (maze_res.maze.width(), maze_res.maze.height());
    for (duckling, mut transform) in clutch_line.iter_mut() {
        let Some(target_cell) = trail_target(&history.0, duckling.index, TRAIL_SPACING) else {
            continue;
        };
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
