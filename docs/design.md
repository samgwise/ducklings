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
- Stacked clutch. Waiting ducklings sharing a cell render as a little
  stack — two on the diagonal, three in a pyramid, four in a grid, a
  centred grid beyond, members slightly shrunken — so a pile the river
  deposits on one let-off cell reads as a group, not one duck.
- Streams. One-way runs of water cells (2–5 straight cells, from level
  index 1, count `1 + n/3` capped at 4) carry anything on them along the
  flow at `STREAM_HOP_SECS` per cell. The duck rides with her conga
  holding in her wake, but a trail slot the current reaches once she is
  clear of the water washes off the line: the duckling sweeps downstream
  to the run's end and waits there for mum's return — bailing out mid-ride
  leaves the whole clutch to the river. Waiting ducklings caught in the
  current get swept. Riding, a manual input toward an open side branch
  hops out (never along the flow, never back upstream); at the run's end
  the current carries one cell past onto the adjoining path. A washed or
  stranded duckling parks where it lands — stacked with any cellmates —
  until mum hops onto it and gathers it back into the line.
  Same-direction runs never leave one-cell dry gaps (the filler merges
  them). Streams are gated
  for solvability: the movement graph (walking + one-way rides with
  junction exits and let-offs) must be strongly connected from the start —
  every dry cell reachable, and able to get back — so a one-way stream can
  never seal a pocket; an unsolvable seed retries with a salt (the
  escalation-ladder pattern).
- Fallen logs. From level index 2, `1 + (n-2)/4` capped at 3 logs lie
  across open passages (never across a stream, never doubled up). Mum
  always can fly one — a slower, arcing hop (`FLIGHT_HOP_SECS`) with a
  flutter and a size-up mid-air — but the clutch cannot follow: flying
  with ducklings in tow leaves the whole trail waiting on her take-off
  side, and a click-walk crosses the same way (mum flies, the tail
  strands). Generation therefore keeps a log only where the walk graph
  (logs as walls, streams one-way) stays strongly connected from the
  start: every dry cell reachable on foot, so a stranded clutch can
  always walk home. On a barely-braided maze most passages are bridges
  and logs are sparse; loopier ponds fit their full count.
- Fog of war. Meadow and Marsh levels sleep in the dark: a second tilemap
  veils every tile mum's sight has not reached. Sight floods at most
  `FOG_REVEAL_RADIUS` corridor steps through open passages — hedges block
  it, streams carry it — lighting what it touches, and every cell seen
  once stays dimly remembered. Pond plays fully lit.
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
- `FLIGHT_HOP_SECS = 0.24`, `FLIGHT_ARC = 16.0`, `FLIGHT_SCALE = 0.3` —
  the flight over a fallen log: duration, peak arc height (pixels) and
  peak mid-air size gain.
- `CONFIRM_DEBOUNCE_SECS = 0.3` — how long after a screen appears before
  confirm input is heeded.
- Difficulty curves (index `n`, zero-based; shared growth `+2n` width,
  `+n` height, `+n` ducklings; caps 25 / 17 / 12):
  - Pond: 5×5 base, `1+n` ducklings, braid `0.4`.
  - Meadow: 7×5 base, `2+n` ducklings, braid `0.2`.
  - Marsh: 9×7 base, `3+n` ducklings, braid `0.05`.
- `FOG_REVEAL_RADIUS = 4` — how many corridor steps mum's sight reaches;
  the remembered veil sits at alpha 150 of 255. Fog falls on Meadow and
  Marsh only.
- `TITLE_TILE_SIZE = 20.0` — the lettering map's tile size (gameplay is
  `TILE_SIZE = 40.0`).

## Audio

All audio plays in-process through `bevy_audio`; there is no sidecar. Music
loops continuously; effects are fire-and-forget one-shots.

| Event | Sound | Notes |
| --- | --- | --- |
| Hop | soft tick | one per completed hop |
| Pickup | rising chirp | playback speed `1.0 + 0.06 × index` per clutch position |
| Bump | low thud | hedge collision, or a log with the clutch aboard |
| Flutter | wingbeat flutter | mum taking off over a fallen log |
| Level complete | bell arpeggio fanfare | on the complete overlay |

Volumes: music `0.6`, effects `0.9` (per-playback, set in `audio.rs`).

## Generation

- `crates/maze`: recursive backtracker on a rectangular cell grid, then
  braiding — a `braid` share of dead ends opens one extra wall, creating
  loops so the clutch can route around each other. Streams are carved
  first (`with_streams`, salted ladder), then fallen logs (`place_logs`,
  kept only where the walk gate holds) — `with_obstacles` composes both,
  retrying salts when a maze is too tree-ish to host every log (stream
  cells can absorb a braid's loops) and keeping the run with the most.
  The ASCII example renders logs as `==`/`‖` across a passage.
- Ducklings spawn on dry dead ends, farthest first (BFS from the start),
  padded with the next-most-distant dry cells when braiding leaves too few
  — never on the water, where a waiting duckling is swept off before it
  can be found.
- The exit is the cell farthest from the start.

## Out of scope (for now)

- Idle duckling wandering — stray hops around a waiting pile — as a
  Marsh-only treat, once the strays can be kept off logs and water.
- Generative in-process score built on `scalevec` (ambience driven by clutch
  size is the natural stretch).
- Real art, hop animations beyond the lerp, shaders, title screen,
  persistence.
