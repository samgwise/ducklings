# ducklings

A cosy puzzle game: a mother duck hops through procedurally generated mazes,
collects her ducklings into a trailing conga line, and leads the whole clutch
to the exit.

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

## Eyeballing generated mazes

Print an ASCII rendering of a generated maze for a given seed:

```sh
cargo run -p maze --example ascii_map 42
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
| WASD / arrow keys | Hop one tile |
| D-pad (Steam Deck / Xbox controllers) | Hop one tile |
| Click / tap | Hop along the corridor toward the point — stops at the click or the first hedge, never around corners |
| Enter / Space / A (controller) | Next level (on the level-complete overlay) |
