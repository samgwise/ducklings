//! A hand-drawn pixel font for exactly the glyphs needed to spell the
//! title lettering — `#` = open corridor, space = hedge. Rows run top to
//! bottom over 7 rows: caps fill rows 0–4, lowercase x-height sits on
//! rows 1–4, and `g`'s tail descends to row 5. Row 6 stays empty and
//! doubles as the inter-line gap.

/// One letterform: equal-width rows of `#` and space.
struct Glyph {
    rows: &'static [&'static str],
}

/// The letterform for `ch`, if the font covers it.
fn glyph(ch: char) -> Option<Glyph> {
    let rows: &[&str] = match ch {
        'O' => &[
            " ### ", "#   #", "#   #", "#   #", "#   #", "#   #", " ### ",
        ],
        'o' => &["    ", " ## ", "#  #", "#  #", " ## ", "    ", "    "],
        'h' => &["#   ", "#   ", "### ", "#  #", "#  #", "    ", "    "],
        'D' => &[
            "#### ", "#   #", "#   #", "#   #", "#   #", "#   #", "#### ",
        ],
        'u' => &["    ", "#  #", "#  #", "#  #", " ## ", "    ", "    "],
        'c' => &["    ", " ## ", "#   ", "#   ", " ## ", "    ", "    "],
        'k' => &["#   ", "#   ", "### ", "##  ", "# ##", "    ", "    "],
        'l' => &["# ", "# ", "# ", "# ", "# ", "  ", "  "],
        'i' => &["# ", "  ", "# ", "# ", "# ", "  ", "  "],
        'n' => &["    ", "##  ", "# # ", "# # ", "# # ", "    ", "    "],
        'g' => &["    ", " ## ", "#  #", "#  #", " ###", " ## ", "    "],
        's' => &["    ", " ## ", "##  ", " ## ", "  ##", "    ", "    "],
        '!' => &["#", "#", "#", " ", "#", " ", " "],
        ' ' => &["   ", "   ", "   ", "   ", "   ", "   ", "   "],
        _ => return None,
    };
    Some(Glyph { rows })
}

/// The open cells of "Ooh Ducklings!" laid out as two centred lines
/// ("Ooh" / "Ducklings!") with a one-cell hedge margin, returned as the
/// grid size and the ink cells.
pub fn title_lettering() -> ((u32, u32), Vec<(u32, u32)>) {
    const LINE_ROWS: i32 = 7;
    const LINE_GAP: i32 = 1;
    const MARGIN: i32 = 1;

    let lines = ["Ooh", "Ducklings!"];
    let line_width = |line: &str| -> i32 {
        line.chars()
            .map(|ch| glyph(ch).expect("the font covers the phrase").rows[0].len() as i32 + 1)
            .sum::<i32>()
            - 1
    };
    let cols = lines.iter().map(|l| line_width(l)).max().unwrap_or(0) + 2 * MARGIN;
    let rows =
        MARGIN + lines.len() as i32 * LINE_ROWS + (lines.len() as i32 - 1) * LINE_GAP + MARGIN;

    let mut ink = Vec::new();
    for (li, line) in lines.iter().enumerate() {
        let top = MARGIN + li as i32 * (LINE_ROWS + LINE_GAP);
        let left = MARGIN + (cols - 2 * MARGIN - line_width(line)) / 2;
        let mut pen = left;
        for ch in line.chars() {
            let g = glyph(ch).expect("the font covers the phrase");
            for (ry, row) in g.rows.iter().enumerate() {
                for (rx, px) in row.chars().enumerate() {
                    if px == '#' {
                        ink.push(((pen + rx as i32) as u32, (top + ry as i32) as u32));
                    }
                }
            }
            pen += g.rows[0].len() as i32 + 1;
        }
    }
    ((cols as u32, rows as u32), ink)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_phrase_is_fully_covered_by_the_font() {
        for ch in "OohDucklings! ".chars() {
            assert!(glyph(ch).is_some(), "glyph {ch:?} missing");
        }
    }

    #[test]
    fn glyphs_are_seven_untarnished_rows() {
        for ch in "OohDucklings! ".chars() {
            let g = glyph(ch).expect("covered");
            assert_eq!(g.rows.len(), 7, "{ch:?} row count");
            for row in g.rows {
                assert_eq!(row.len(), g.rows[0].len(), "{ch:?} ragged rows");
                assert!(
                    row.chars().all(|px| px == '#' || px == ' '),
                    "{ch:?} stray pixel"
                );
            }
        }
    }

    #[test]
    fn the_lettering_fits_inside_its_grid() {
        let ((cols, rows), ink) = title_lettering();
        assert!(ink.len() > 100, "the lettering has body");
        for (x, y) in ink {
            assert!(x < cols && y < rows, "ink at {x},{y} outside {cols}x{rows}");
        }
    }
}
