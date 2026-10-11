//! Level flow: the Playing/Complete phase machine, the run seed, the level
//! index, and the difficulty curve.

use bevy::prelude::*;

use crate::duck::Duck;
use crate::ducklings::Clutch;
use crate::world::{LevelEntity, MazeRes, build_level};

/// The shape of one level.
#[derive(Debug)]
pub struct LevelPlan {
    pub width: u32,
    pub height: u32,
    pub ducklings: usize,
    pub braid: f32,
}

/// The three maze difficulties, chosen on the title screen.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Difficulty {
    Pond,
    #[default]
    Meadow,
    Marsh,
}

/// The difficulty curve: each level's maze grows and gains a duckling.
/// Growth and caps are shared; the bases and braid carry the challenge —
/// a lower braid leaves more dead-end traps, a higher one loops more.
pub fn level_plan(index: u32, difficulty: Difficulty) -> LevelPlan {
    let (base_w, base_h, base_ducklings, braid) = match difficulty {
        Difficulty::Pond => (5u32, 5u32, 1usize, 0.4),
        Difficulty::Meadow => (7, 5, 2, 0.2),
        Difficulty::Marsh => (9, 7, 3, 0.05),
    };
    LevelPlan {
        width: (base_w + 2 * index).min(25),
        height: (base_h + index).min(17),
        ducklings: (base_ducklings + index as usize).min(12),
        braid,
    }
}

/// How the app divides its time: the title screen, the live level, or the
/// complete overlay.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GamePhase {
    #[default]
    Title,
    Playing,
    Complete,
}

/// Base seed for the run; each level offsets it by its index. Pinned by the
/// `DUCKLINGS_SEED` environment variable, else taken from the clock.
#[derive(Resource, Debug)]
pub struct RunSeed(pub u64);

impl Default for RunSeed {
    fn default() -> Self {
        Self(
            std::env::var("DUCKLINGS_SEED")
                .ok()
                .and_then(|seed| seed.parse().ok())
                .unwrap_or_else(|| {
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()
                }),
        )
    }
}

/// Zero-based index of the current level.
#[derive(Resource, Debug, Default)]
pub struct LevelIndex(pub u32);

/// The level state machine and progression; screens live in menus.
pub struct LevelPlugin;

impl Plugin for LevelPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<GamePhase>()
            .init_resource::<RunSeed>()
            .init_resource::<LevelIndex>()
            .add_systems(
                Update,
                (check_level_complete, debug_complete_level).run_if(in_state(GamePhase::Playing)),
            )
            // The complete screen itself lives in menus; leaving it
            // rebuilds the next pond.
            .add_systems(OnExit(GamePhase::Complete), rebuild_level);
    }
}

/// Reaching the exit with the full clutch completes the level.
fn check_level_complete(
    duck: Single<&Duck>,
    clutch: Res<Clutch>,
    maze_res: Res<MazeRes>,
    mut next: ResMut<NextState<GamePhase>>,
) {
    if duck.cell == maze_res.exit && clutch.collected == clutch.total {
        next.set(GamePhase::Complete);
    }
}

/// Debug: jump straight to the level-complete screen so the overlay and
/// its advance input can be tested without finishing a clutch.
fn debug_complete_level(keys: Res<ButtonInput<KeyCode>>, mut next: ResMut<NextState<GamePhase>>) {
    if keys.just_pressed(KeyCode::KeyN) {
        next.set(GamePhase::Complete);
    }
}

/// Advance the level index and build the next pond. Runs on exit from the
/// Complete phase; the new entities land before the next Playing frame.
#[allow(clippy::too_many_arguments)]
fn rebuild_level(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut level: ResMut<LevelIndex>,
    difficulty: Res<Difficulty>,
    seed: Res<RunSeed>,
    finished: Query<Entity, With<LevelEntity>>,
) {
    for entity in finished.iter() {
        commands.entity(entity).despawn();
    }
    level.0 += 1;
    let plan = level_plan(level.0, *difficulty);
    build_level(
        &mut commands,
        &mut images,
        &mut meshes,
        &mut materials,
        &plan,
        seed.0.wrapping_add(level.0 as u64),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn difficulties_start_at_their_bases() {
        let pond = level_plan(0, Difficulty::Pond);
        assert_eq!((pond.width, pond.height), (5, 5));
        assert_eq!(pond.ducklings, 1);
        assert_eq!(pond.braid, 0.4);
        let meadow = level_plan(0, Difficulty::Meadow);
        assert_eq!((meadow.width, meadow.height), (7, 5));
        assert_eq!(meadow.ducklings, 2);
        assert_eq!(meadow.braid, 0.2);
        let marsh = level_plan(0, Difficulty::Marsh);
        assert_eq!((marsh.width, marsh.height), (9, 7));
        assert_eq!(marsh.ducklings, 3);
        assert_eq!(marsh.braid, 0.05);
    }

    #[test]
    fn curves_grow_and_cap() {
        // Deep in the curve every cap has bitten.
        let plan = level_plan(50, Difficulty::Meadow);
        assert_eq!((plan.width, plan.height, plan.ducklings), (25, 17, 12));
        // Before the caps, each level adds its increment.
        let a = level_plan(3, Difficulty::Pond);
        let b = level_plan(4, Difficulty::Pond);
        assert_eq!(b.width, a.width + 2);
        assert_eq!(b.height, a.height + 1);
        assert_eq!(b.ducklings, a.ducklings + 1);
    }
}
