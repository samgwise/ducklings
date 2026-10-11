//! In-process audio: a looping pond bed and fire-and-forget sound effects,
//! all through `bevy_audio` — no sidecar. The WAV files are synthesised by
//! the `gen_placeholder_audio` example; swap the files, keep the code.

use bevy::audio::Volume;
use bevy::prelude::*;

/// Music volume, 0–1.
const MUSIC_VOLUME: f32 = 0.6;

/// Effect volume, 0–1.
const SFX_VOLUME: f32 = 0.9;

/// Sound-effect requests. Writers stay decoupled from playback.
#[derive(Message, Debug)]
pub enum SfxEvent {
    /// The duck landed a hop.
    Hop,
    /// The duck bumped a hedge.
    Bump,
    /// A duckling joined the clutch; `speed` raises the pitch per position.
    Chirp { speed: f32 },
    /// The level is complete.
    Fanfare,
    /// Crossing the waterline: entering a stream, or the let-off.
    Splash,
}

/// Handles to the loaded effects.
#[derive(Resource, Default)]
pub struct SfxHandles {
    hop: Option<Handle<AudioSource>>,
    bump: Option<Handle<AudioSource>>,
    chirp: Option<Handle<AudioSource>>,
    fanfare: Option<Handle<AudioSource>>,
    splash: Option<Handle<AudioSource>>,
}

/// Marks the looping music entity.
#[derive(Component)]
struct Music;

pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Messages<SfxEvent>>()
            .init_resource::<SfxHandles>()
            .add_systems(Startup, (load_sfx, start_music))
            .add_systems(Update, play_sfx);
    }
}

/// Load the effect handles; an unfinished handle simply delays its sound.
fn load_sfx(mut handles: ResMut<SfxHandles>, server: Res<AssetServer>) {
    handles.hop = Some(server.load("audio/sfx/hop.wav"));
    handles.bump = Some(server.load("audio/sfx/bump.wav"));
    handles.chirp = Some(server.load("audio/sfx/chirp.wav"));
    handles.fanfare = Some(server.load("audio/sfx/fanfare.wav"));
    handles.splash = Some(server.load("audio/sfx/splash.wav"));
}

/// Start the looping pond bed.
fn start_music(mut commands: Commands, server: Res<AssetServer>) {
    let music: Handle<AudioSource> = server.load("audio/music/pond.wav");
    commands.spawn((
        Music,
        AudioPlayer::new(music),
        PlaybackSettings::LOOP.with_volume(Volume::Linear(MUSIC_VOLUME)),
    ));
}

/// Spawn a one-shot per queued event.
fn play_sfx(mut events: MessageReader<SfxEvent>, handles: Res<SfxHandles>, mut commands: Commands) {
    for event in events.read() {
        let (handle, speed) = match event {
            SfxEvent::Hop => (handles.hop.clone(), 1.0),
            SfxEvent::Bump => (handles.bump.clone(), 1.0),
            SfxEvent::Chirp { speed } => (handles.chirp.clone(), *speed),
            SfxEvent::Fanfare => (handles.fanfare.clone(), 1.0),
            SfxEvent::Splash => (handles.splash.clone(), 1.0),
        };
        let Some(handle) = handle else {
            continue;
        };
        commands.spawn((
            AudioPlayer::new(handle),
            PlaybackSettings::DESPAWN
                .with_volume(Volume::Linear(SFX_VOLUME))
                .with_speed(speed),
        ));
    }
}
