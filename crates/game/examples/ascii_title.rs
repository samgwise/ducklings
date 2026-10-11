//! Print the title lettering as ASCII — the same open-cell layout the
//! title map renders:
//!
//! ```sh
//! cargo run -p game --example ascii_title
//! ```

use game::glyphs::title_lettering;

fn main() {
    let ((cols, rows), ink) = title_lettering();
    let ink: std::collections::HashSet<(u32, u32)> = ink.into_iter().collect();
    // Exactly the render's decision, tile row by tile row top-down: tile
    // row `ty` shows ink row `rows - 1 - ty`.
    for ty in (0..rows).rev() {
        let line: String = (0..cols)
            .map(|x| {
                if ink.contains(&(x, rows - 1 - ty)) {
                    '#'
                } else {
                    '.'
                }
            })
            .collect();
        println!("{line}");
    }
}
