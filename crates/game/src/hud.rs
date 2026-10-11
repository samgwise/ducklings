//! Heads-up display: the level number and the clutch counter. All bevy_ui
//! nodes over the world; screens live in menus.

use bevy::prelude::*;

use crate::ducklings::Clutch;
use crate::level::LevelIndex;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_hud)
            .add_systems(Update, update_label);
    }
}

/// Marker for the top-left level/clutch label.
#[derive(Component)]
struct LevelLabel;

fn spawn_hud(mut commands: Commands) {
    commands.spawn((
        LevelLabel,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(16.0),
            top: Val::Px(16.0),
            ..default()
        },
        Text::new("Level 1"),
        TextFont {
            font_size: FontSize::Px(22.0),
            ..default()
        },
        TextColor(Color::srgb(0.25, 0.2, 0.15)),
    ));
}

/// Keep the label honest: level index (one-based on screen) and the clutch.
fn update_label(
    level: Res<LevelIndex>,
    clutch: Res<Clutch>,
    mut label: Single<&mut Text, With<LevelLabel>>,
) {
    label.0 = format!(
        "Level {}   Clutch {}/{}",
        level.0 + 1,
        clutch.collected,
        clutch.total
    );
}
