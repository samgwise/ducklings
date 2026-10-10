//! Print an ASCII rendering of a generated maze: `+`/`-`/`|` hedges, `S`
//! start, `E` exit, `d` dead end, `.` open cell.
//!
//! ```sh
//! cargo run -p maze --example ascii_map [seed] [width] [height] [braid_percent]
//! ```

use maze::{Direction, Maze};

fn main() {
    let args = std::env::args().skip(1);
    let parsed: Vec<u64> = args.filter_map(|arg| arg.parse().ok()).collect();
    let arg = |index: usize, default: u64| parsed.get(index).copied().unwrap_or(default);

    let (seed, width, height) = (arg(0, 0), arg(1, 11) as u32, arg(2, 9) as u32);
    let braid = arg(3, 20) as f32 / 100.0;
    let maze = Maze::generate(width, height, seed, braid);
    let distances = maze.distances_from((0, 0));

    // The exit is the farthest cell from the start, mirroring the game.
    let exit = (0..width * height)
        .map(|i| (i % width, i / width))
        .filter(|&c| distances[(c.1 * width + c.0) as usize].is_some())
        .max_by_key(|&c| distances[(c.1 * width + c.0) as usize])
        .expect("a generated maze is fully connected");
    let dead_ends = maze.dead_ends();

    // Cell rows print top-down so north reads as up.
    for _ in 0..width {
        print!("+--");
    }
    println!("+");
    for y in (0..height).rev() {
        for x in 0..width {
            print!(
                "{}",
                if maze.is_open(x as i32, y as i32, Direction::West) {
                    ' '
                } else {
                    '|'
                }
            );
            let cell = (x, y);
            let glyph = if cell == (0, 0) {
                'S'
            } else if cell == exit {
                'E'
            } else if dead_ends.contains(&cell) {
                'd'
            } else {
                '.'
            };
            print!("{glyph}");
        }
        println!(
            "{}",
            if maze.is_open((width - 1) as i32, y as i32, Direction::East) {
                ' '
            } else {
                '|'
            }
        );
        for x in 0..width {
            print!(
                "+{}",
                if maze.is_open(x as i32, y as i32, Direction::South) {
                    "  "
                } else {
                    "--"
                }
            );
        }
        println!("+");
    }
    println!("seed: {seed}  {width}x{height}  braid: {braid}");
}
