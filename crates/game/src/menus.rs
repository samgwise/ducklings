//! Shared screen idioms: the confirm input (Enter/Space, pad A, taps), the
//! confirm debounce so a finishing input cannot skip its own screen, the
//! overlay panel builder, and the level-complete screen — the first screen,
//! and the pattern future title and pause screens will reuse.

use bevy::input::gamepad::{Gamepad, GamepadButton};
use bevy::prelude::*;

use crate::audio::SfxEvent;
use crate::level::GamePhase;

/// How long after a screen appears before confirm input is heeded, in
/// seconds — the tap that finished a level must not also skip its overlay.
const CONFIRM_DEBOUNCE_SECS: f32 = 0.3;

/// Arms when a screen appears; confirm input before `armed_until` is
/// ignored.
#[derive(Resource, Debug, Default)]
pub struct ConfirmGate {
    armed_until: f32,
}

impl ConfirmGate {
    /// Arm the gate `delay` seconds from now.
    fn arm(&mut self, time: &Time, delay: f32) {
        self.armed_until = time.elapsed_secs() + delay;
    }

    /// Whether confirm input may be heeded yet.
    fn ready(&self, time: &Time) -> bool {
        time.elapsed_secs() >= self.armed_until
    }
}

/// Whether "continue" was just pressed: Enter or Space on the keyboard, a
/// left click, South (A) on any connected pad, or a just-started touch
/// anywhere.
fn confirm_just_pressed(
    keys: &ButtonInput<KeyCode>,
    clicks: &ButtonInput<MouseButton>,
    pads: &Query<&Gamepad>,
    touches: &Touches,
) -> bool {
    keys.just_pressed(KeyCode::Enter)
        || keys.just_pressed(KeyCode::Space)
        || clicks.just_pressed(MouseButton::Left)
        || pads
            .iter()
            .any(|pad| pad.just_pressed(GamepadButton::South))
        || touches.iter_just_pressed().next().is_some()
}

/// Spawn a centred full-screen overlay panel with a title and subtitle
/// line. The returned entity is the screen's toggle: spawn hidden, flip
/// `Visibility` to show and hide.
fn spawn_overlay(commands: &mut Commands, title: &str, subtitle: &str) -> Entity {
    commands
        .spawn((
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
            BackgroundColor(Color::srgba(1.0, 0.98, 0.92, 0.6)),
            Visibility::Hidden,
        ))
        .with_children(|overlay| {
            overlay.spawn((
                Node::default(),
                Text::new(title),
                TextFont {
                    font_size: FontSize::Px(40.0),
                    ..default()
                },
                TextColor(Color::srgb(0.3, 0.25, 0.2)),
            ));
            overlay.spawn((
                Node::default(),
                Text::new(subtitle),
                TextFont {
                    font_size: FontSize::Px(24.0),
                    ..default()
                },
                TextColor(Color::srgb(0.45, 0.4, 0.35)),
            ));
        })
        .id()
}

/// Marks the level-complete overlay's root node.
#[derive(Component)]
struct LevelCompleteOverlay;

/// The level-complete overlay's root entity, spawned at startup and
/// toggled by visibility ever after.
#[derive(Resource, Debug, Default)]
struct LevelCompleteScreen {
    root: Option<Entity>,
}

pub struct MenusPlugin;

impl Plugin for MenusPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ConfirmGate>()
            .init_resource::<LevelCompleteScreen>()
            .add_systems(Startup, spawn_level_complete_screen)
            .add_systems(OnEnter(GamePhase::Complete), enter_level_complete)
            .add_systems(
                Update,
                advance_from_level_complete.run_if(in_state(GamePhase::Complete)),
            )
            .add_systems(OnExit(GamePhase::Complete), exit_level_complete);
    }
}

/// Build the overlay once; it toggles via visibility ever after.
fn spawn_level_complete_screen(mut commands: Commands, mut screen: ResMut<LevelCompleteScreen>) {
    let root = spawn_overlay(
        &mut commands,
        "Level complete!",
        "Tap or press Enter / A for the next pond",
    );
    commands.entity(root).insert(LevelCompleteOverlay);
    screen.root = Some(root);
}

/// The level-complete screen appears: fanfare, overlay up, gate armed.
fn enter_level_complete(
    mut commands: Commands,
    mut gate: ResMut<ConfirmGate>,
    time: Res<Time>,
    mut sfx: MessageWriter<SfxEvent>,
    screen: Res<LevelCompleteScreen>,
) {
    sfx.write(SfxEvent::Fanfare);
    gate.arm(&time, CONFIRM_DEBOUNCE_SECS);
    if let Some(root) = screen.root {
        commands.entity(root).insert(Visibility::Visible);
    }
}

/// Confirm (once the debounce passes) moves on to the next pond.
fn advance_from_level_complete(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    clicks: Res<ButtonInput<MouseButton>>,
    pads: Query<&Gamepad>,
    touches: Res<Touches>,
    gate: Res<ConfirmGate>,
    mut next: ResMut<NextState<GamePhase>>,
) {
    if gate.ready(&time) && confirm_just_pressed(&keys, &clicks, &pads, &touches) {
        next.set(GamePhase::Playing);
    }
}

fn exit_level_complete(mut commands: Commands, screen: Res<LevelCompleteScreen>) {
    if let Some(root) = screen.root {
        commands.entity(root).insert(Visibility::Hidden);
    }
}
