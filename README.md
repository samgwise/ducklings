# ducklings

A cosy puzzle game: pick your pond, then guide a mother duck through
procedurally generated mazes, collecting her ducklings into a trailing conga
line and leading the whole clutch to the exit. One-way streams sweep
anything on them downstream — ride them as fast-travel highways, hop out
at junctions, and pluck swept ducklings as they pass. Fallen logs bar the
clutch's way: mum can fly over one while she's still alone, but ducklings
can't follow, so logs never guard the route home.

Three difficulties shape the ponds: **Pond** (gentle loops), **Meadow**
(the classic maze) and **Marsh** (dead ends and wits).

## Workspace

- `crates/game` — Bevy client: grid-hop movement, the duckling trail, level
  flow, in-process audio (music loop and sound effects via `bevy_audio`) and
  the HUD.
- `crates/maze` — pure-Rust maze generation (recursive backtracker with
  braiding) plus BFS distance fields, deliberately engine-free so it can be
  tested in isolation.

## Prerequisites

- Rust stable.
- Linux only: ALSA and udev dev packages for Bevy's audio and gamepad
  support — `sudo apt-get install --no-install-recommends libasound2-dev
  libudev-dev pkg-config`.

## Building and testing

```sh
cargo build
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

## Running

```sh
cargo run -p game
```

Set `DUCKLINGS_SEED` to a whole number for a reproducible run; unset, each
launch picks a fresh seed.

## CI builds

Every push to `main` and pull request runs the tests and publishes release
artifacts (Actions → Rust run → Artifacts): a Windows zip and a Linux
tarball built against glibc 2.35, so it runs on SteamOS / Steam Deck. Each
archive holds the game binary with the `assets` directory beside it —
extract it and run from inside the folder.

## Eyeballing generated mazes and the title lettering

Print an ASCII rendering of a generated maze for a given seed:

```sh
cargo run -p maze --example ascii_map 42
```

Or the title screen's lettering map:

```sh
cargo run -p game --example ascii_title
```

## Regenerating the placeholder sound effects

The committed SFX files under `assets/audio/sfx` are synthesised
placeholders; regenerate them after tweaking the synthesis (the music loop
is a hand-made take and is never touched):

```sh
cargo run -p game --example gen_placeholder_audio
```

## Controls

| Input | Action |
| --- | --- |
| Up / Down (keys or D-pad), click or tap | Choose a difficulty on the title screen |
| WASD / arrow keys | Hop one tile |
| D-pad (Steam Deck / Xbox controllers) | Hop one tile |
| Click / tap | Hop along the corridor toward the point — stops at the click or the first hedge, never around corners; unreachable clicks get a head shake |
| Tap / Enter / Space / A (controller) | Next level (on the level-complete overlay) |
| N | Debug: jump to the level-complete overlay |
