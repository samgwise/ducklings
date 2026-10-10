# Ducklings — design

The living, as-built reference: mechanics, tuning values, and the audio event
table. Kept current as the game grows.

## Premise

A mother duck hops through a procedurally generated maze, gathering her
scattered ducklings. Each duckling found joins the end of a conga line behind
her. The level is complete when the duck stands on the exit mat with the whole
clutch in tow.

## Mechanics

- Grid-step movement. The duck hops one cell at a time with a short lerp;
  one move can be buffered mid-hop so quick turns feel responsive. Hopping
  into a hedge is a bump: nothing moves, a soft thud plays.
- Duckling trail. The duck's completed hops are recorded as a path history;
  duckling *i* drifts toward the position `SPACING × (i + 1)` steps back along
  that history, smoothed exponentially. The clutch forms a smooth conga line.
- Pickup. Ducklings wait at dead ends and distant cells. Hopping onto one
  collects it: it joins the trail and chirps, pitched a little higher for each
  sibling already in the line.
- Level flow. Reaching the exit with the full clutch completes the level.
  The next level's maze grows; seeds derive from the run seed plus the level
  index, so a run is reproducible from its seed.

## Tuning values

- `TILE_SIZE = 40.0` — world units (pixels) per tile.
- `HOP_SECS = 0.15` — duration of one duck hop.
- `FOLLOW_SMOOTHING = 14.0` — exponential smoothing rate for the duckling
  trail (higher = tighter line).
- `TRAIL_SPACING = 1` — history steps between consecutive ducklings.
- `HISTORY_CAP = 512` — path history entries kept per level.
- `CAMERA_SMOOTHING = 6.0` — exponential smoothing rate for camera follow.
- Level curve (index `n`, zero-based): width `7 + 2n` capped at 25, height
  `5 + n` capped at 17, ducklings `2 + n` capped at 12, braid `0.2`.

## Audio

All audio plays in-process through `bevy_audio`; there is no sidecar. Music
loops continuously; effects are fire-and-forget one-shots.

| Event | Sound | Notes |
| --- | --- | --- |
| Hop | soft tick | one per completed hop |
| Pickup | rising chirp | playback speed `1.0 + 0.06 × index` per clutch position |
| Bump | low thud | hedge collision |
| Level complete | bell arpeggio fanfare | on the complete overlay |

Volumes: music `0.6`, effects `0.9` (per-playback, set in `audio.rs`).

## Generation

- `crates/maze`: recursive backtracker on a rectangular cell grid, then
  braiding — a `braid` share of dead ends opens one extra wall, creating
  loops so the clutch can route around each other.
- Ducklings spawn on dead ends, farthest first (BFS from the start), padded
  with the next-most-distant cells when braiding leaves too few.
- The exit is the cell farthest from the start.

## Out of scope (for now)

- Generative in-process score built on `scalevec` (ambience driven by clutch
  size is the natural stretch).
- Real art, hop animations beyond the lerp, shaders, title screen, gamepad,
  persistence.
