//! The mother duck: buffered input, one-tile hops with a smooth lerp, and
//! the path history her ducklings follow.

use bevy::input::gamepad::{Gamepad, GamepadButton};
use bevy::prelude::*;

use maze::Direction;

use crate::audio::SfxEvent;
use crate::level::GamePhase;
use crate::world::{MazeRes, TILE_SIZE, cell_world_pos};

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

pub struct DuckPlugin;

impl Plugin for DuckPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (buffer_input, hop).run_if(in_state(GamePhase::Playing)),
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
        ),
        With<Duck>,
    >,
) {
    let (entity, mut duck, mut transform, mut buffered, hop, mut history) = mover.into_inner();
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
    }
    let Some(dir) = buffered.0.take() else {
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
