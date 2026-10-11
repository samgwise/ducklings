//! Pure-Rust maze generation for the ducklings game: a recursive backtracker
//! with braiding, plus BFS distance fields. Deliberately engine-free so it
//! can run off the ECS thread and be tested in isolation.

use std::collections::VecDeque;

use rand::RngExt;
use rand::SeedableRng;
use rand::rngs::SmallRng;

/// The four-neighbourhood of a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    North,
    East,
    South,
    West,
}

impl Direction {
    /// All directions, in a fixed order.
    pub const ALL: [Direction; 4] = [Self::North, Self::East, Self::South, Self::West];

    /// The step `dir` moves a cell by.
    pub fn delta(self) -> (i32, i32) {
        match self {
            Self::North => (0, 1),
            Self::East => (1, 0),
            Self::South => (0, -1),
            Self::West => (-1, 0),
        }
    }

    /// The direction pointing back the other way.
    pub fn opposite(self) -> Self {
        match self {
            Self::North => Self::South,
            Self::East => Self::West,
            Self::South => Self::North,
            Self::West => Self::East,
        }
    }

    /// The bit in a cell's open-passage mask.
    const fn bit(self) -> u8 {
        1 << match self {
            Self::North => 0,
            Self::East => 1,
            Self::South => 2,
            Self::West => 3,
        }
    }
}

/// A maze on a `width x height` grid of cells. Open passages are stored as a
/// per-cell bit mask; closed bits are hedges the duck cannot hop. Cells may
/// also carry a one-way stream current: anything landing there is carried
/// along the flow until the run ends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Maze {
    width: u32,
    height: u32,
    open: Vec<u8>,
    /// Per-cell stream flow, if any. Straight runs only — no cycles.
    currents: Vec<Option<Direction>>,
}

impl Maze {
    /// Generate a maze: the recursive backtracker (run iteratively), then
    /// braided — a `braid` share of dead ends opens one extra passage,
    /// creating loops the clutch can route around. The same seed always
    /// yields the same maze.
    pub fn generate(width: u32, height: u32, seed: u64, braid: f32) -> Self {
        let mut maze = Self {
            width,
            height,
            open: vec![0; (width * height) as usize],
            currents: vec![None; (width * height) as usize],
        };
        if width == 0 || height == 0 {
            return maze;
        }
        let mut rng = SmallRng::seed_from_u64(seed);
        let mut visited = vec![false; (width * height) as usize];
        let mut stack = vec![(0u32, 0u32)];
        visited[0] = true;
        while let Some(&(x, y)) = stack.last() {
            let unvisited: Vec<Direction> = Direction::ALL
                .into_iter()
                .filter(|&dir| {
                    let Some((nx, ny)) = maze.neighbour(x, y, dir) else {
                        return false;
                    };
                    !visited[maze.index(nx, ny)]
                })
                .collect();
            if unvisited.is_empty() {
                stack.pop();
            } else {
                let dir = unvisited[rng.random_range(0..unvisited.len())];
                let next = maze.neighbour(x, y, dir).expect("neighbour is in bounds");
                maze.carve(x, y, dir);
                visited[maze.index(next.0, next.1)] = true;
                stack.push(next);
            }
        }
        maze.braid(&mut rng, braid);
        maze
    }

    /// Generate a maze and carve `stream_count` one-way stream runs.
    /// Streams can seal dry cells off behind a one-way barrier, so the
    /// result is gated: the movement graph — dry walking plus one-way
    /// rides with junction exits and let-offs — must reach every dry cell
    /// from `(0, 0)`. An unsolvable seed retries with a salted seed;
    /// every few rounds the stream count relaxes by one, guaranteeing
    /// termination (a dry maze is always solvable).
    pub fn with_streams(width: u32, height: u32, seed: u64, braid: f32, stream_count: u32) -> Self {
        let mut salt = 0u64;
        let mut streams = stream_count;
        loop {
            let mut maze = Maze::generate(width, height, seed ^ salt, braid);
            maze.carve_streams(SmallRng::seed_from_u64(seed ^ salt ^ 0x5EA7), streams);
            if maze.dry_strongly_connected_from((0, 0)) {
                return maze;
            }
            salt += 1;
            if salt.is_multiple_of(4) && streams > 0 {
                streams -= 1;
            }
        }
    }

    /// Maze width in cells.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Maze height in cells.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Whether the passage toward `dir` is open from cell `(x, y)`.
    /// Out-of-bounds cells read as walled.
    pub fn is_open(&self, x: i32, y: i32, dir: Direction) -> bool {
        let (dx, dy) = dir.delta();
        let (nx, ny) = (x + dx, y + dy);
        if nx < 0 || ny < 0 || nx >= self.width as i32 || ny >= self.height as i32 {
            return false;
        }
        self.open[self.index(x as u32, y as u32)] & dir.bit() != 0
    }

    /// The stream flow at cell `(x, y)`, if any.
    pub fn flow_at(&self, x: u32, y: u32) -> Option<Direction> {
        if self.has_cell(x, y) {
            self.currents[self.index(x, y)]
        } else {
            None
        }
    }

    /// The cell one past the end of this cell's stream run — where the
    /// current deposits whatever it carries. A dry cell lets off to itself.
    pub fn let_off(&self, x: u32, y: u32) -> (u32, u32) {
        let Some(dir) = self.flow_at(x, y) else {
            return (x, y);
        };
        let mut cur = (x, y);
        loop {
            let (dx, dy) = dir.delta();
            let next = (cur.0 as i32 + dx, cur.1 as i32 + dy);
            let flowing = next.0 >= 0
                && next.1 >= 0
                && self.flow_at(next.0 as u32, next.1 as u32) == Some(dir);
            if flowing {
                cur = (next.0 as u32, next.1 as u32);
            } else {
                // The run's end: the let-off lies one cell past. Generation
                // guarantees it is in bounds, open, and dry.
                return (next.0 as u32, next.1 as u32);
            }
        }
    }

    /// Cells with exactly one open passage.
    pub fn dead_ends(&self) -> Vec<(u32, u32)> {
        let mut ends = Vec::new();
        for y in 0..self.height {
            for x in 0..self.width {
                if self.open[self.index(x, y)].count_ones() == 1 {
                    ends.push((x, y));
                }
            }
        }
        ends
    }

    /// BFS distances in cells from `start`; `None` for unreachable cells
    /// (impossible in a generated maze, but the API stays total).
    pub fn distances_from(&self, start: (u32, u32)) -> Vec<Option<u32>> {
        let mut distances = vec![None; (self.width * self.height) as usize];
        if !self.has_cell(start.0, start.1) {
            return distances;
        }
        let mut queue = VecDeque::new();
        distances[self.index(start.0, start.1)] = Some(0);
        queue.push_back(start);
        while let Some((x, y)) = queue.pop_front() {
            let steps = distances[self.index(x, y)].expect("visited cells know their distance");
            for dir in Direction::ALL {
                if !self.is_open(x as i32, y as i32, dir) {
                    continue;
                }
                let (nx, ny) = self
                    .neighbour(x, y, dir)
                    .expect("open passages are in bounds");
                if distances[self.index(nx, ny)].is_none() {
                    distances[self.index(nx, ny)] = Some(steps + 1);
                    queue.push_back((nx, ny));
                }
            }
        }
        distances
    }

    /// Whether a cell exists at `(x, y)`.
    fn has_cell(&self, x: u32, y: u32) -> bool {
        x < self.width && y < self.height
    }

    /// The cell across `dir`, if it exists.
    fn neighbour(&self, x: u32, y: u32, dir: Direction) -> Option<(u32, u32)> {
        let (dx, dy) = dir.delta();
        let (nx, ny) = (x as i32 + dx, y as i32 + dy);
        (nx >= 0 && ny >= 0 && (nx as u32) < self.width && (ny as u32) < self.height)
            .then_some((nx as u32, ny as u32))
    }

    /// Row-major index into the passage masks.
    fn index(&self, x: u32, y: u32) -> usize {
        (y * self.width + x) as usize
    }

    /// Open the passage between a cell and its neighbour (and back).
    fn carve(&mut self, x: u32, y: u32, dir: Direction) {
        let (nx, ny) = self
            .neighbour(x, y, dir)
            .expect("carve target is in bounds");
        let here = self.index(x, y);
        let there = self.index(nx, ny);
        self.open[here] |= dir.bit();
        self.open[there] |= dir.opposite().bit();
    }

    /// Open one extra passage at a `braid` share of dead ends. A dead end
    /// whose only neighbour is its own passage (a 1xN grid's tips) is left
    /// alone — there is nothing to open into.
    fn braid(&mut self, rng: &mut SmallRng, braid: f32) {
        if braid <= 0.0 {
            return;
        }
        for y in 0..self.height {
            for x in 0..self.width {
                if self.open[self.index(x, y)].count_ones() != 1 || rng.random::<f32>() >= braid {
                    continue;
                }
                let closed: Vec<Direction> = Direction::ALL
                    .into_iter()
                    .filter(|&dir| {
                        self.neighbour(x, y, dir).is_some()
                            && !self.is_open(x as i32, y as i32, dir)
                    })
                    .collect();
                if closed.is_empty() {
                    continue;
                }
                self.carve(x, y, closed[rng.random_range(0..closed.len())]);
            }
        }
    }

    /// Carve up to `count` straight stream runs: each picks a random cell
    /// (never the game's start corner) and direction, then follows open
    /// passages for 2–5 cells without touching existing stream cells. The
    /// run must end with an open, dry let-off one cell past its last cell.
    fn carve_streams(&mut self, mut rng: SmallRng, count: u32) {
        let mut placed = 0;
        let mut attempts = 0;
        while placed < count && attempts < 200 {
            attempts += 1;
            let x = rng.random_range(0..self.width);
            let y = rng.random_range(0..self.height);
            let dir = Direction::ALL[rng.random_range(0..4)];
            let run_len = rng.random_range(2..=5);

            let mut cells = vec![(x, y)];
            let mut cur = (x, y);
            for _ in 1..run_len {
                let Some(next) = self.neighbour(cur.0, cur.1, dir) else {
                    break;
                };
                if !self.is_open(cur.0 as i32, cur.1 as i32, dir)
                    || self.flow_at(next.0, next.1).is_some()
                    || next == (0, 0)
                {
                    break;
                }
                cells.push(next);
                cur = next;
            }
            if cells.len() < 2 {
                continue;
            }
            // The let-off: one cell past the run's end — in bounds, open,
            // and dry, or the run is rejected.
            let Some(let_off) = self.neighbour(cur.0, cur.1, dir) else {
                continue;
            };
            if !self.is_open(cur.0 as i32, cur.1 as i32, dir)
                || self.flow_at(let_off.0, let_off.1).is_some()
                || let_off == (0, 0)
            {
                continue;
            }
            for &(cx, cy) in &cells {
                let cell = self.index(cx, cy);
                self.currents[cell] = Some(dir);
            }
            placed += 1;
        }
        self.fill_gaps();
    }

    /// Fill one-cell dry gaps between two stream runs flowing the same
    /// direction — a rider let off into a gap would stop dead mid-stream.
    /// Filling cascades until no same-direction gaps remain, merging the
    /// runs into one continuous flow (whose let-off is the last run's).
    fn fill_gaps(&mut self) {
        loop {
            let mut filled = false;
            for y in 0..self.height {
                for x in 0..self.width {
                    let Some(flow) = self.currents[self.index(x, y)] else {
                        continue;
                    };
                    let (dx, dy) = flow.delta();
                    let mid = (x as i32 + dx, y as i32 + dy);
                    let beyond = (x as i32 + 2 * dx, y as i32 + 2 * dy);
                    if mid.0 < 0
                        || mid.1 < 0
                        || mid.0 >= self.width as i32
                        || mid.1 >= self.height as i32
                        || beyond.0 < 0
                        || beyond.1 < 0
                        || beyond.0 >= self.width as i32
                        || beyond.1 >= self.height as i32
                    {
                        continue;
                    }
                    let mid_i = self.index(mid.0 as u32, mid.1 as u32);
                    if self.currents[mid_i].is_none()
                        && self.flow_at(beyond.0 as u32, beyond.1 as u32) == Some(flow)
                    {
                        self.currents[mid_i] = Some(flow);
                        filled = true;
                    }
                }
            }
            if !filled {
                break;
            }
        }
    }

    /// The movement graph over dry cells: walking edges both ways, plus
    /// one-way ride edges — from a dry cell beside a stream, entering the
    /// current sweeps you downstream past each run cell (hop-out points
    /// at every side branch you pass) until the let-off.
    fn dry_edges(&self) -> Vec<(usize, usize)> {
        let mut edges = Vec::new();
        for y in 0..self.height {
            for x in 0..self.width {
                if self.flow_at(x, y).is_some() {
                    continue;
                }
                let from = self.index(x, y);
                for dir in Direction::ALL {
                    if !self.is_open(x as i32, y as i32, dir) {
                        continue;
                    }
                    let (nx, ny) = self
                        .neighbour(x, y, dir)
                        .expect("an open passage is in bounds");
                    if self.flow_at(nx, ny).is_none() {
                        edges.push((from, self.index(nx, ny)));
                    } else {
                        let flow = self.flow_at(nx, ny).expect("a stream cell flows");
                        let mut cur = (nx, ny);
                        loop {
                            for side in Direction::ALL {
                                if side == flow || side == flow.opposite() {
                                    continue; // downstream is the current's job; upstream fights it
                                }
                                if !self.is_open(cur.0 as i32, cur.1 as i32, side) {
                                    continue;
                                }
                                let (sx, sy) = self
                                    .neighbour(cur.0, cur.1, side)
                                    .expect("a side branch is in bounds");
                                if self.flow_at(sx, sy).is_none() {
                                    edges.push((from, self.index(sx, sy)));
                                }
                            }
                            let (dx, dy) = flow.delta();
                            let next = (cur.0 as i32 + dx, cur.1 as i32 + dy);
                            let next = (next.0 as u32, next.1 as u32);
                            if self.flow_at(next.0, next.1) == Some(flow) {
                                cur = next;
                            } else {
                                edges.push((from, self.index(next.0, next.1)));
                                break;
                            }
                        }
                    }
                }
            }
        }
        edges
    }

    /// Whether the dry cells are strongly connected from `start`: every
    /// dry cell reachable from the start, and the start reachable from
    /// every dry cell (reachability in the graph and in its reverse).
    /// A one-way stream must never seal a pocket the duck can't leave.
    fn dry_strongly_connected_from(&self, start: (u32, u32)) -> bool {
        if self.flow_at(start.0, start.1).is_some() {
            return false;
        }
        let edges = self.dry_edges();
        let reversed: Vec<(usize, usize)> = edges.iter().map(|&(a, b)| (b, a)).collect();
        let dry_count = self.currents.iter().filter(|c| c.is_none()).count();
        let start = self.index(start.0, start.1);
        let node_count = self.currents.len();
        Self::reaches(&edges, start, node_count, dry_count)
            && Self::reaches(&reversed, start, node_count, dry_count)
    }

    /// Whether a BFS over `edges` from `start` visits `must_visit` nodes.
    fn reaches(
        edges: &[(usize, usize)],
        start: usize,
        node_count: usize,
        must_visit: usize,
    ) -> bool {
        let mut visited = vec![false; node_count];
        let mut queue = VecDeque::new();
        visited[start] = true;
        let mut seen = 1;
        queue.push_back(start);
        while let Some(n) = queue.pop_front() {
            for &(a, b) in edges {
                if a == n && !visited[b] {
                    visited[b] = true;
                    seen += 1;
                    queue.push_back(b);
                }
            }
        }
        seen == must_visit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sum of open-passage bits across every cell — each passage counts
    /// twice (once per side).
    fn open_bits(maze: &Maze) -> u32 {
        (0..maze.height())
            .flat_map(|y| (0..maze.width()).map(move |x| (x, y)))
            .map(|(x, y)| {
                Direction::ALL
                    .into_iter()
                    .filter(|&dir| maze.is_open(x as i32, y as i32, dir))
                    .count() as u32
            })
            .sum()
    }

    #[test]
    fn generated_mazes_are_fully_connected() {
        for &(width, height) in &[(1u32, 5u32), (5, 5), (7, 4), (13, 11)] {
            for seed in 0..5u64 {
                let maze = Maze::generate(width, height, seed, 0.2);
                let distances = maze.distances_from((0, 0));
                assert!(
                    distances.iter().all(|d| d.is_some()),
                    "every cell is reachable at {width}x{height} seed {seed}"
                );
            }
        }
    }

    #[test]
    fn the_same_seed_yields_the_same_maze() {
        let a = Maze::generate(11, 9, 42, 0.2);
        let b = Maze::generate(11, 9, 42, 0.2);
        assert_eq!(a, b);
        // A different seed almost surely differs — a weak but cheap check
        // that the seed actually feeds the generator.
        let c = Maze::generate(11, 9, 43, 0.2);
        assert_ne!(a, c);
    }

    #[test]
    fn unbraided_mazes_are_perfect() {
        // A perfect maze is a spanning tree: cells - 1 passages, each
        // counted from both sides — and it must keep some dead ends.
        let maze = Maze::generate(7, 7, 7, 0.0);
        assert_eq!(open_bits(&maze), 2 * (7 * 7 - 1));
        assert!(!maze.dead_ends().is_empty());
    }

    #[test]
    fn full_braiding_removes_dead_ends() {
        let maze = Maze::generate(9, 9, 3, 1.0);
        assert!(maze.dead_ends().is_empty());
        // Braiding only ever opens passages: the maze stays connected.
        assert!(maze.distances_from((0, 0)).iter().all(|d| d.is_some()));
    }

    #[test]
    fn open_passages_are_symmetric() {
        let maze = Maze::generate(8, 6, 11, 0.35);
        for y in 0..maze.height() {
            for x in 0..maze.width() {
                for dir in Direction::ALL {
                    let open = maze.is_open(x as i32, y as i32, dir);
                    let neighbour = maze.neighbour(x, y, dir);
                    let back = neighbour
                        .is_some_and(|(nx, ny)| maze.is_open(nx as i32, ny as i32, dir.opposite()));
                    assert_eq!(
                        open, back,
                        "the passage at {x},{y} {dir:?} agrees with its neighbour"
                    );
                }
            }
        }
    }

    #[test]
    fn streams_are_straight_runs_with_a_dry_let_off() {
        for seed in 0..10u64 {
            let maze = Maze::with_streams(11, 9, seed, 0.2, 3);
            for y in 0..maze.height() {
                for x in 0..maze.width() {
                    if maze.flow_at(x, y).is_some() {
                        // Walking the flow must land on an open, dry
                        // let-off — never cycle, never dump into a hedge.
                        let let_off = maze.let_off(x, y);
                        assert!(
                            maze.flow_at(let_off.0, let_off.1).is_none(),
                            "the let-off is dry at {x},{y}"
                        );
                    }
                }
            }
            assert!(maze.flow_at(0, 0).is_none(), "the start corner never flows");
        }
    }

    #[test]
    fn stream_gaps_are_filled() {
        // Two same-direction runs may not leave a one-cell dry gap — the
        // filler merges them into one continuous flow.
        for seed in 0..10u64 {
            let maze = Maze::with_streams(13, 11, seed, 0.2, 4);
            for y in 0..maze.height() {
                for x in 0..maze.width() {
                    let Some(flow) = maze.flow_at(x, y) else {
                        continue;
                    };
                    let (dx, dy) = flow.delta();
                    let mid = (x as i32 + dx, y as i32 + dy);
                    let beyond = (x as i32 + 2 * dx, y as i32 + 2 * dy);
                    if beyond.0 < 0
                        || beyond.1 < 0
                        || beyond.0 >= maze.width() as i32
                        || beyond.1 >= maze.height() as i32
                    {
                        continue;
                    }
                    if maze.flow_at(beyond.0 as u32, beyond.1 as u32) == Some(flow) {
                        assert!(
                            maze.flow_at(mid.0 as u32, mid.1 as u32).is_some(),
                            "a gap at {mid:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn streams_solvability_gate_holds() {
        // The constructor gates generation: the dry cells must be
        // strongly connected — reachable from the start AND able to get
        // back, so a one-way stream can never seal a pocket.
        for seed in 0..20u64 {
            let maze = Maze::with_streams(13, 11, seed, 0.2, 4);
            assert!(
                maze.dry_strongly_connected_from((0, 0)),
                "dry cells strongly connected at seed {seed}"
            );
        }
    }

    #[test]
    fn stream_generation_is_deterministic() {
        let a = Maze::with_streams(11, 9, 42, 0.2, 3);
        let b = Maze::with_streams(11, 9, 42, 0.2, 3);
        assert_eq!(a, b);
    }

    #[test]
    fn distances_count_cells_along_the_path() {
        // A 3x3 maze carved fully open: the far corner is 4 steps away.
        let mut maze = Maze::generate(3, 3, 1, 0.0);
        for y in 0..3 {
            for x in 0..3 {
                if x + 1 < 3 {
                    maze.carve(x, y, Direction::East);
                }
                if y + 1 < 3 {
                    maze.carve(x, y, Direction::North);
                }
            }
        }
        let distances = maze.distances_from((0, 0));
        assert_eq!(distances[maze.index(2, 2)], Some(4));
        assert_eq!(distances[maze.index(0, 0)], Some(0));
    }
}
