//! ducklings game library: boots each level's braided maze, hops the mother
//! duck tile to tile, trails her ducklings behind her, and plays music and
//! effects in-process through `bevy_audio` — no sidecar.
//!
//! Organised as a library (with the binary a thin wrapper) so examples such
//! as the audio synthesiser — and future integration tests — can reuse the
//! real modules instead of duplicating them.

pub mod audio;
pub mod bridge;
pub mod duck;
pub mod ducklings;
pub mod exit;
pub mod fog;
pub mod glyphs;
pub mod hud;
pub mod level;
pub mod menus;
pub mod world;

use std::path::PathBuf;

use bevy::asset::AssetPlugin;
use bevy::prelude::*;

/// Resolve the assets directory at runtime: packaged builds ship `assets`
/// beside the executable, while development runs (via `cargo`) fall back to
/// the workspace copy recorded at compile time — Bevy roots asset sources at
/// the executable's directory, which for target/ binaries is nowhere near
/// the workspace assets without this override.
fn assets_path() -> PathBuf {
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(std::path::Path::to_path_buf))
    {
        let bundled = exe_dir.join("assets");
        if bundled.is_dir() {
            return bundled;
        }
    }
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets").into()
}

/// Assemble and run the game client.
pub fn run() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(AssetPlugin {
                    file_path: assets_path().to_string_lossy().into_owned(),
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
            fog::FogPlugin,
            exit::ExitPlugin,
            bridge::BridgePlugin,
            audio::SoundPlugin,
            hud::HudPlugin,
            menus::MenusPlugin,
        ))
        .run();
}
