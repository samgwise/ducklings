//! Heads-up display: the level number, the clutch counter, and the
//! level-complete overlay. All bevy_ui nodes over the world.

use bevy::prelude::*;

use crate::ducklings::Clutch;
use crate::level::{GamePhase, LevelIndex};

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_hud)
            .add_systems(Update, update_label)
            .add_systems(OnEnter(GamePhase::Complete), show_overlay)
            .add_systems(OnExit(GamePhase::Complete), hide_overlay);
    }
}

/// Marker for the top-left level/clutch label.
#[derive(Component)]
struct LevelLabel;

/// Marker for the full-screen level-complete overlay.
#[derive(Component)]
struct CompleteOverlay;

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

    commands
        .spawn((
            CompleteOverlay,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(Color::srgba(1.0, 0.98, 0.92, 0.85)),
            Visibility::Hidden,
        ))
        .with_children(|overlay| {
            overlay.spawn((
                Node::default(),
                Text::new("Level complete!"),
                TextFont {
                    font_size: FontSize::Px(40.0),
                    ..default()
                },
                TextColor(Color::srgb(0.3, 0.25, 0.2)),
            ));
            overlay.spawn((
                Node::default(),
                Text::new("Press Enter for the next pond"),
                TextFont {
                    font_size: FontSize::Px(20.0),
                    ..default()
                },
                TextColor(Color::srgb(0.45, 0.4, 0.35)),
            ));
        });
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

fn show_overlay(mut overlay: Single<&mut Visibility, With<CompleteOverlay>>) {
    overlay.set_if_neq(Visibility::Visible);
}

fn hide_overlay(mut overlay: Single<&mut Visibility, With<CompleteOverlay>>) {
    overlay.set_if_neq(Visibility::Hidden);
}
