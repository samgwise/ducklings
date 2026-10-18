//! Heads-up display: the level number and the clutch counter. All bevy_ui
//! nodes over the world; screens live in menus.

use bevy::prelude::*;

use crate::bridge::PlankStock;
use crate::ducklings::Clutch;
use crate::level::{GamePhase, LevelIndex};

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_hud)
            .add_systems(OnEnter(GamePhase::Playing), show_label)
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
        // Hidden until the first pond exists — the title screen is clean.
        Visibility::Hidden,
    ));
}

/// The label appears once the first pond exists (idempotent per level).
fn show_label(mut label: Single<&mut Visibility, With<LevelLabel>>) {
    label.set_if_neq(Visibility::Visible);
}

/// Keep the label honest: level index (one-based on screen), the clutch,
/// and — on bridge levels — the plank stock.
fn update_label(
    level: Res<LevelIndex>,
    clutch: Res<Clutch>,
    stock: Res<PlankStock>,
    mut label: Single<&mut Text, With<LevelLabel>>,
) {
    let planks = if stock.total > 0 {
        format!("   Planks {}/{}", stock.left, stock.total)
    } else {
        String::new()
    };
    label.0 = format!(
        "Level {}   Clutch {}/{}{}",
        level.0 + 1,
        clutch.collected,
        clutch.total,
        planks
    );
}
