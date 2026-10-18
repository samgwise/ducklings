//! Exit cues: once the mat has been found, the screen helps you finish —
//! an arrow at the viewport edge points to an off-screen exit, and a soft
//! white outline pulses around the mat while the whole clutch is gathered
//! and the exit is in view.

use bevy::math::primitives::{Rectangle, Triangle2d};
use bevy::prelude::*;

use crate::ducklings::Clutch;
use crate::fog::FogState;
use crate::level::GamePhase;
use crate::world::{LevelEntity, MainCamera, MazeRes, TILE_SIZE, cell_world_pos};

/// Inset of the off-screen arrow from the viewport edge, in pixels.
const ARROW_MARGIN_PX: f32 = 28.0;
/// Radius of the arrow triangle, in pixels.
const ARROW_SIZE: f32 = 26.0;
/// Seconds for one beat of the mat's pulsing outline.
const PULSE_PERIOD_SECS: f32 = 1.1;
/// The outline's alpha range across a beat.
const PULSE_MIN_ALPHA: f32 = 0.15;
const PULSE_MAX_ALPHA: f32 = 0.95;
/// How far the outline's rim reaches beyond the mat tile, as a fraction of
/// a tile.
const OUTLINE_GROWTH: f32 = 0.3;

/// The arrow wears the exit mat's own pink.
const MAT_COLOUR: Color = Color::srgb(0.933, 0.553, 0.69);

/// The edge arrow pointing to an off-screen exit.
#[derive(Component)]
struct ExitArrow;

/// The pulsing outline square behind the mat tile.
#[derive(Component)]
struct ExitOutline;

pub struct ExitPlugin;

impl Plugin for ExitPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (update_exit_arrow, pulse_exit_outline).run_if(in_state(GamePhase::Playing)),
        );
    }
}

/// Spawn the level's exit cues. Both start hidden; their systems reveal
/// them as conditions allow.
pub(crate) fn build_exit_cues(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    exit: (u32, u32),
    map_cells: (u32, u32),
) {
    let at = cell_world_pos(map_cells, exit, TILE_SIZE);
    // The outline: a square a little larger than the mat tile, so only its
    // rim shows around the opaque mat.
    let half = TILE_SIZE * (1.0 + OUTLINE_GROWTH) / 2.0;
    commands.spawn((
        LevelEntity,
        ExitOutline,
        Mesh2d(meshes.add(Mesh::from(Rectangle::new(half, half)))),
        MeshMaterial2d(materials.add(ColorMaterial::from_color(Color::WHITE))),
        Transform::from_xyz(at.x, at.y, 0.5),
        Visibility::Hidden,
    ));
    // The arrow: a small triangle pointing along +x, rotated per frame
    // toward the exit.
    let tip = ARROW_SIZE * 0.7;
    commands.spawn((
        LevelEntity,
        ExitArrow,
        Mesh2d(meshes.add(Mesh::from(Triangle2d::new(
            Vec2::new(tip, 0.0),
            Vec2::new(-tip * 0.6, tip * 0.6),
            Vec2::new(-tip * 0.6, -tip * 0.6),
        )))),
        MeshMaterial2d(materials.add(ColorMaterial::from_color(MAT_COLOUR))),
        Transform::from_xyz(0.0, 0.0, 6.0),
        Visibility::Hidden,
    ));
}

/// The arrow shows only while the revealed exit lies off-screen; then it
/// rides the viewport edge along the camera→exit ray, aimed at the exit.
#[allow(clippy::type_complexity)]
fn update_exit_arrow(
    camera_at: Single<&GlobalTransform, With<MainCamera>>,
    window: Single<&Window>,
    maze_res: Res<MazeRes>,
    fog: Res<FogState>,
    arrow: Single<(&mut Transform, &mut Visibility), With<ExitArrow>>,
) {
    let at = cell_world_pos(
        (maze_res.maze.width(), maze_res.maze.height()),
        maze_res.exit,
        TILE_SIZE,
    );
    // Revealed: fogged levels show the exit once seen; the pond never
    // hides it.
    let revealed = !fog.enabled || fog.seen.contains(&maze_res.exit);
    let camera_at = camera_at.translation().xy();
    let half = viewport_half(&window);
    let (mut transform, mut visibility) = arrow.into_inner();
    if revealed && !exit_on_screen(camera_at, half, at) {
        let (pos, angle) = arrow_placement(camera_at, half, ARROW_MARGIN_PX, at);
        transform.translation = pos.extend(6.0);
        transform.rotation = Quat::from_rotation_z(angle);
        visibility.set_if_neq(Visibility::Visible);
    } else {
        visibility.set_if_neq(Visibility::Hidden);
    }
}

/// The mat's outline beats while the whole clutch is gathered and the exit
/// is in view.
#[allow(clippy::too_many_arguments)]
fn pulse_exit_outline(
    time: Res<Time>,
    camera_at: Single<&GlobalTransform, With<MainCamera>>,
    window: Single<&Window>,
    maze_res: Res<MazeRes>,
    clutch: Res<Clutch>,
    mut phase: Local<f32>,
    outline: Single<(&MeshMaterial2d<ColorMaterial>, &mut Visibility), With<ExitOutline>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    *phase += time.delta_secs() * core::f32::consts::TAU / PULSE_PERIOD_SECS;
    let at = cell_world_pos(
        (maze_res.maze.width(), maze_res.maze.height()),
        maze_res.exit,
        TILE_SIZE,
    );
    let camera_at = camera_at.translation().xy();
    let half = viewport_half(&window);
    let (material, mut visibility) = outline.into_inner();
    let show = clutch.collected == clutch.total && exit_on_screen(camera_at, half, at);
    visibility.set_if_neq(if show {
        Visibility::Visible
    } else {
        Visibility::Hidden
    });
    if show
        && let Some(mut material) = materials.get_mut(&material.0)
    {
        material.color = Color::WHITE.with_alpha(pulse_alpha(*phase));
    }
}

/// The visible world rect's half-extent: the camera has no zoom, so world
/// units map one-to-one onto the window's logical pixels.
fn viewport_half(window: &Window) -> Vec2 {
    Vec2::new(window.width(), window.height()) * 0.5
}

/// Whether the world point lies within the camera's view.
fn exit_on_screen(camera_at: Vec2, half: Vec2, at: Vec2) -> bool {
    at.x >= camera_at.x - half.x
        && at.x <= camera_at.x + half.x
        && at.y >= camera_at.y - half.y
        && at.y <= camera_at.y + half.y
}

/// Where the edge arrow sits and what it points at: along the camera→exit
/// ray, clamped inside the viewport edge inset by the margin, aimed along
/// the ray.
fn arrow_placement(camera_at: Vec2, half: Vec2, margin: f32, at: Vec2) -> (Vec2, f32) {
    let reach = half - Vec2::splat(margin);
    let dir = (at - camera_at).normalize_or_zero();
    let scale = if dir == Vec2::ZERO {
        0.0
    } else {
        (reach.x / dir.x.abs().max(f32::EPSILON))
            .min(reach.y / dir.y.abs().max(f32::EPSILON))
    };
    let pos = camera_at + dir * scale;
    let angle = dir.y.atan2(dir.x);
    (pos, angle)
}

/// The outline's alpha on one beat: a slow sweep between the bounds.
fn pulse_alpha(phase: f32) -> f32 {
    let beat = (1.0 - phase.cos()) / 2.0;
    PULSE_MIN_ALPHA + (PULSE_MAX_ALPHA - PULSE_MIN_ALPHA) * beat
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_arrow_appears_only_for_an_off_screen_exit() {
        // An exit well inside the view is on screen; one beyond the edge
        // is not.
        let camera_at = Vec2::ZERO;
        let half = Vec2::new(400.0, 300.0);
        assert!(exit_on_screen(camera_at, half, Vec2::new(390.0, 0.0)));
        assert!(!exit_on_screen(camera_at, half, Vec2::new(410.0, 0.0)));
    }

    #[test]
    fn the_arrow_rides_the_viewport_edge_pointing_at_the_exit() {
        // The arrow sits inside the inset viewport edge along the
        // camera→exit ray, aimed along that ray.
        let camera_at = Vec2::ZERO;
        let half = Vec2::new(400.0, 300.0);
        // An exit far east: the arrow hugs the right edge, pointing east.
        let (pos, angle) = arrow_placement(camera_at, half, ARROW_MARGIN_PX, Vec2::new(4000.0, 0.0));
        assert!((pos.x - (400.0 - ARROW_MARGIN_PX)).abs() < 1e-4, "east at the right edge, got {pos}");
        assert!((pos.y - 0.0).abs() < 1e-4);
        assert!(angle.abs() < 1e-4, "pointing east");
        // An exit far north-west: up the top edge, pointing along the ray.
        let (pos, angle) = arrow_placement(camera_at, half, ARROW_MARGIN_PX, Vec2::new(-4000.0, 3000.0));
        assert!(pos.x >= -(400.0 - ARROW_MARGIN_PX) - 1e-4);
        assert!((pos.y - (300.0 - ARROW_MARGIN_PX)).abs() < 1e-4, "north-west at the top edge, got {pos}");
        assert!((angle - 3000.0f32.atan2(-4000.0)).abs() < 1e-4, "pointing along the ray");
    }

    #[test]
    fn the_pulse_oscillates_between_its_bounds() {
        // The sweep starts at the minimum, peaks at half a period, and
        // repeats.
        let alphas: Vec<f32> = (0..=16)
            .map(|i| pulse_alpha(i as f32 * core::f32::consts::TAU / 16.0))
            .collect();
        assert!(
            alphas
                .iter()
                .all(|&a| (PULSE_MIN_ALPHA..=PULSE_MAX_ALPHA).contains(&a))
        );
        assert!((alphas[0] - PULSE_MIN_ALPHA).abs() < 1e-6);
        assert!((alphas[8] - PULSE_MAX_ALPHA).abs() < 1e-6);
        assert!((alphas[16] - alphas[0]).abs() < 1e-6, "one period later it repeats");
    }
}
