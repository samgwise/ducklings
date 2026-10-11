//! Shared screen idioms: the confirm input (Enter/Space, pad A, taps), the
//! confirm debounce so a finishing input cannot skip its own screen, the
//! overlay panel builder, and the level-complete screen — the first screen,
//! and the pattern future title and pause screens will reuse.

use bevy::input::gamepad::{Gamepad, GamepadButton};
use bevy::prelude::*;

use crate::audio::SfxEvent;
use crate::level::{Difficulty, GamePhase};

/// How long after a screen appears before confirm input is heeded, in
/// seconds — the tap that finished a level must not also skip its overlay.
const CONFIRM_DEBOUNCE_SECS: f32 = 0.3;

/// The title's difficulty choices, in focus order.
const DIFFICULTY_CHOICES: [(Difficulty, &str); 3] = [
    (Difficulty::Pond, "Pond — gentle loops"),
    (Difficulty::Meadow, "Meadow — the classic maze"),
    (Difficulty::Marsh, "Marsh — dead ends and wits"),
];

/// Text colours for focused vs idle difficulty buttons.
const FOCUSED_COLOUR: Color = Color::srgb(0.25, 0.2, 0.15);
const IDLE_COLOUR: Color = Color::srgb(0.65, 0.58, 0.5);

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

/// Marks the title screen's root node (despawned once a pond is chosen).
#[derive(Component)]
struct TitleScreenRoot;

/// Text of one difficulty button — the focus highlight's target.
#[derive(Component)]
struct ButtonLabel;

/// A difficulty choice button.
#[derive(Component)]
struct DifficultyButton(Difficulty);

/// The title screen's difficulty-button text entities, in focus order.
#[derive(Resource, Debug, Default)]
struct TitleScreen {
    labels: Vec<Entity>,
}

/// Which difficulty button holds the focus.
#[derive(Resource, Debug, Default)]
struct TitleFocus {
    index: usize,
}

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
            .init_resource::<TitleScreen>()
            .init_resource::<TitleFocus>()
            .add_systems(Startup, (spawn_level_complete_screen, spawn_title_screen))
            .add_systems(
                Update,
                (
                    navigate_title_menu,
                    highlight_title_focus,
                    activate_title_menu,
                )
                    .chain()
                    .run_if(in_state(GamePhase::Title)),
            )
            .add_systems(OnExit(GamePhase::Title), exit_title_screen)
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
    screen.root = Some(spawn_overlay(
        &mut commands,
        "Level complete!",
        "Tap or press Enter / A for the next pond",
    ));
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

/// Build the title screen: a heading over one button per difficulty. The
/// root stays transparent so the lettering map reads through; each button
/// carries its own small panel for legibility.
fn spawn_title_screen(mut commands: Commands, mut screen: ResMut<TitleScreen>) {
    let root = commands
        .spawn((
            TitleScreenRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                // The buttons live below the lettering map.
                justify_content: JustifyContent::FlexEnd,
                padding: UiRect::bottom(Val::Px(20.0)),
                row_gap: Val::Px(10.0),
                ..default()
            },
        ))
        .id();
    let mut buttons = Vec::new();
    commands.entity(root).with_children(|title| {
        title.spawn((
            Node {
                padding: UiRect::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(1.0, 0.98, 0.92, 0.6)),
            Text::new("choose your pond"),
            TextFont {
                font_size: FontSize::Px(24.0),
                ..default()
            },
            TextColor(Color::srgb(0.45, 0.4, 0.35)),
        ));
        for (difficulty, label) in DIFFICULTY_CHOICES {
            buttons.push((
                title
                    .spawn((
                        DifficultyButton(difficulty),
                        Button,
                        Interaction::default(),
                        Node {
                            padding: UiRect::all(Val::Px(8.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(1.0, 0.98, 0.92, 0.6)),
                    ))
                    .id(),
                label,
            ));
        }
    });
    for (index, (button, label)) in buttons.into_iter().enumerate() {
        let label_entity = commands
            .spawn((
                ButtonLabel,
                Node::default(),
                Text::new(label),
                TextFont {
                    font_size: FontSize::Px(24.0),
                    ..default()
                },
                TextColor(if index == 0 {
                    FOCUSED_COLOUR
                } else {
                    IDLE_COLOUR
                }),
            ))
            .id();
        commands.entity(button).add_child(label_entity);
        screen.labels.push(label_entity);
    }
}

/// Up/down (keys or D-pad) move the button focus, wrapping.
fn navigate_title_menu(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut focus: ResMut<TitleFocus>,
) {
    let up = keys.just_pressed(KeyCode::KeyW)
        || keys.just_pressed(KeyCode::ArrowUp)
        || pads
            .iter()
            .any(|pad| pad.just_pressed(GamepadButton::DPadUp));
    let down = keys.just_pressed(KeyCode::KeyS)
        || keys.just_pressed(KeyCode::ArrowDown)
        || pads
            .iter()
            .any(|pad| pad.just_pressed(GamepadButton::DPadDown));
    if up == down {
        return; // neither, or both — hold the line
    }
    if up {
        focus.index = (focus.index + DIFFICULTY_CHOICES.len() - 1) % DIFFICULTY_CHOICES.len();
    } else {
        focus.index = (focus.index + 1) % DIFFICULTY_CHOICES.len();
    }
}

/// Brighten the focused button's text (only when the focus changes).
fn highlight_title_focus(
    focus: Res<TitleFocus>,
    screen: Res<TitleScreen>,
    mut labels: Query<&mut TextColor, With<ButtonLabel>>,
) {
    if !focus.is_changed() {
        return;
    }
    for (index, &label_entity) in screen.labels.iter().enumerate() {
        if let Ok(mut colour) = labels.get_mut(label_entity) {
            colour.0 = if index == focus.index {
                FOCUSED_COLOUR
            } else {
                IDLE_COLOUR
            };
        }
    }
}

/// A pressed button chooses directly; otherwise confirm activates the
/// focused one.
#[allow(clippy::too_many_arguments)]
fn activate_title_menu(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    clicks: Res<ButtonInput<MouseButton>>,
    pads: Query<&Gamepad>,
    touches: Res<Touches>,
    interactions: Query<(&Interaction, &DifficultyButton), Changed<Interaction>>,
    focus: Res<TitleFocus>,
    mut next: ResMut<NextState<GamePhase>>,
) {
    for (interaction, button) in interactions.iter() {
        if *interaction == Interaction::Pressed {
            choose(button.0, &mut commands, &mut next);
            return;
        }
    }
    if confirm_just_pressed(&keys, &clicks, &pads, &touches) {
        let difficulty = match focus.index {
            0 => Difficulty::Pond,
            1 => Difficulty::Meadow,
            _ => Difficulty::Marsh,
        };
        choose(difficulty, &mut commands, &mut next);
    }
}

/// Commit a difficulty and start the run.
fn choose(difficulty: Difficulty, commands: &mut Commands, next: &mut NextState<GamePhase>) {
    commands.insert_resource(difficulty);
    next.set(GamePhase::Playing);
}

/// The title's work is done: sweep the screen away.
fn exit_title_screen(mut commands: Commands, screen: Query<Entity, With<TitleScreenRoot>>) {
    for entity in screen.iter() {
        commands.entity(entity).despawn();
    }
}
