//! ducklings game library: boots each level's braided maze, hops the mother
//! duck tile to tile, trails her ducklings behind her, and plays music and
//! effects in-process through `bevy_audio` — no sidecar.
//!
//! Organised as a library (with the binary a thin wrapper) so examples such
//! as the audio synthesiser — and future integration tests — can reuse the
//! real modules instead of duplicating them.

pub mod audio;
pub mod duck;
pub mod ducklings;
pub mod hud;
pub mod level;
pub mod world;

use bevy::asset::AssetPlugin;
use bevy::prelude::*;

/// Absolute assets directory, resolved at compile time — Bevy roots asset
/// sources at the executable's directory, which for target/ binaries is
/// nowhere near the workspace assets without this override.
pub const ASSETS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets");

/// Assemble and run the game client.
pub fn run() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(AssetPlugin {
                    file_path: ASSETS_PATH.into(),
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Ducklings".into(),
                        ..default()
                    }),
                    ..default()
                })
                // The procedural tile atlas is tiny pixel art — keep it crisp.
                .set(ImagePlugin::default_nearest()),
        )
        // LevelPlugin first: it initialises the phase state the rest read.
        .add_plugins((
            level::LevelPlugin,
            world::WorldPlugin,
            duck::DuckPlugin,
            ducklings::DucklingsPlugin,
            audio::SoundPlugin,
            hud::HudPlugin,
        ))
        .run();
}
