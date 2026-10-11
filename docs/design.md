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
  one move can be buffered mid-hop so quick turns feel responsive. Input
  comes from the keyboard (WASD/arrows), any connected controller's D-pad,
  or a click/tap: the duck hops along the corridor toward the point — as
  far as the click along the dominant axis, truncated at the first hedge,
  never around corners. A key or D-pad press cancels a queued walk; a new
  click replaces it. A walk that ends short of its click — or a click
  straight into a hedge — gets an out-of-range cue: the duck shakes her
  head (a decaying beak wag) with the bump thud. Hopping into a hedge is
  the same bump: nothing moves, a soft thud plays.
- Duckling trail. The duck's completed hops are recorded as a path history;
  duckling *i* drifts toward the position `SPACING × (i + 1)` steps back along
  that history, smoothed exponentially. The clutch forms a smooth conga line.
- Pickup. Ducklings wait at dead ends and distant cells. Hopping onto one
  collects it: it joins the trail and chirps, pitched a little higher for each
  sibling already in the line.
- Streams. One-way runs of water cells (2–5 straight cells, from level
  index 1, count `1 + n/3` capped at 4) carry anything on them along the
  flow at `STREAM_HOP_SECS` per cell. The duck rides with her conga
  trailing through the water; waiting ducklings caught in the current get
  swept. Riding, a manual input toward an open side branch hops out (never
  along the flow, never back upstream); at the run's end the current
  carries one cell past onto the adjoining path. A swept duckling passing
  onto or beside mum's cell is plucked into the trail. Same-direction runs
  never leave one-cell dry gaps (the filler merges them). Streams are gated
  for solvability: the movement graph (walking + one-way rides with
  junction exits and let-offs) must be strongly connected from the start —
  every dry cell reachable, and able to get back — so a one-way stream can
  never seal a pocket; an unsolvable seed retries with a salt (the
  escalation-ladder pattern).
- Level flow. Reaching the exit with the full clutch completes the level.
  The next level's maze grows; seeds derive from the run seed plus the level
  index, so a run is reproducible from its seed.

## Screens

Screens are phases of the one `GamePhase` state machine with shared idioms
in `crates/game/src/menus.rs`: the overlay panel builder, the confirm input
(Enter/Space, pad A, or a tap) and a `CONFIRM_DEBOUNCE_SECS` gate so the
input that opened a screen cannot also dismiss it.

- **Title** (the boot phase): a decorative lettering map spelling "Ooh
  Ducklings!" (a hand-drawn pixel font in `glyphs.rs`, rendered at
  `TITLE_TILE_SIZE`) behind three difficulty buttons — Pond / Meadow /
  Marsh. Up/down + confirm or click/tap chooses; choosing builds the first
  pond and sweeps the lettering away.

## Tuning values

- `TILE_SIZE = 40.0` — world units (pixels) per tile.
- `HOP_SECS = 0.15` — duration of one duck hop.
- `FOLLOW_SMOOTHING = 14.0` — exponential smoothing rate for the duckling
  trail (higher = tighter line).
- `TRAIL_SPACING = 1` — history steps between consecutive ducklings.
- `HISTORY_CAP = 512` — path history entries kept per level.
- `CAMERA_SMOOTHING = 6.0` — exponential smoothing rate for camera follow.
- `SHAKE_SECS = 0.36`, `SHAKE_AMPLITUDE = 0.6`, `SHAKE_WAGS = 3.0` — the
  out-of-range head shake's duration, peak yaw swing (radians) and wags.
- `STREAM_HOP_SECS = 0.07` — duration of one stream-carried hop (the
  current is roughly double walking speed).
- `CONFIRM_DEBOUNCE_SECS = 0.3` — how long after a screen appears before
  confirm input is heeded.
- Difficulty curves (index `n`, zero-based; shared growth `+2n` width,
  `+n` height, `+n` ducklings; caps 25 / 17 / 12):
  - Pond: 5×5 base, `1+n` ducklings, braid `0.4`.
  - Meadow: 7×5 base, `2+n` ducklings, braid `0.2`.
  - Marsh: 9×7 base, `3+n` ducklings, braid `0.05`.
- `TITLE_TILE_SIZE = 24.0` — the lettering map's tile size (gameplay is
  `TILE_SIZE = 40.0`).

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
- Real art, hop animations beyond the lerp, shaders, title screen,
  persistence.
