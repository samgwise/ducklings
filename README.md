# ducklings

A cosy puzzle game: pick your pond, then guide a mother duck through
procedurally generated mazes, collecting her ducklings into a trailing conga
line and leading the whole clutch to the exit.

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

## Eyeballing generated mazes and the title lettering

Print an ASCII rendering of a generated maze for a given seed:

```sh
cargo run -p maze --example ascii_map 42
```

Or the title screen's lettering map:

```sh
cargo run -p game --example ascii_title
```

## Regenerating the placeholder audio

The committed WAV files under `assets/audio` are synthesised placeholders.
Regenerate them after tweaking the synthesis:

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
