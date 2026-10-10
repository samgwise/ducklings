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
/// per-cell bit mask; closed bits are hedges the duck cannot hop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Maze {
    width: u32,
    height: u32,
    open: Vec<u8>,
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
