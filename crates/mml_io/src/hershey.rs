//! Hershey single-line fonts: every letter is a stroke, not an outline,
//! which is what pen plotters, V-bits and fast laser marking need.

use mml_core::geometry::{Path, Point};

/// (family name, `.jhf` data). Glyphs are stored for ASCII 32 to 126.
const FONTS: [(&str, &str); 7] = [
    ("Hershey Sans", include_str!("../hershey/futural.jhf")),
    ("Hershey Sans Bold", include_str!("../hershey/futuram.jhf")),
    ("Hershey Roman", include_str!("../hershey/rowmans.jhf")),
    ("Hershey Serif", include_str!("../hershey/timesr.jhf")),
    ("Hershey Script", include_str!("../hershey/scripts.jhf")),
    ("Hershey Cursive", include_str!("../hershey/cursive.jhf")),
    ("Hershey Gothic", include_str!("../hershey/gothiceng.jhf")),
];

/// Hershey glyphs are about 32 units from descender to ascender.
const UNITS_PER_EM: f64 = 32.0;

struct Glyph {
    left: i32,
    right: i32,
    strokes: Vec<Vec<(i32, i32)>>,
}

pub fn hershey_families() -> Vec<String> {
    FONTS.iter().map(|(name, _)| (*name).to_owned()).collect()
}

pub fn is_hershey(family: &str) -> bool {
    FONTS.iter().any(|(name, _)| *name == family)
}

/// Parses the `.jhf` format: 5-character glyph number, 3-character pair
/// count, then character pairs offset from 'R'; " R" lifts the pen.
/// Long glyphs continue on the following lines.
fn parse(data: &str) -> Vec<Glyph> {
    let mut glyphs = Vec::new();
    let mut lines = data.lines().peekable();
    while let Some(line) = lines.next() {
        if line.len() < 8 {
            continue;
        }
        let Ok(count) = line[5..8].trim().parse::<usize>() else {
            continue;
        };
        let mut body: String = line[8..].to_owned();
        while body.len() < count * 2 {
            match lines.next() {
                Some(next) => body.push_str(next),
                None => break,
            }
        }
        let bytes = body.as_bytes();
        if bytes.len() < 2 {
            continue;
        }
        let value = |byte: u8| byte as i32 - b'R' as i32;
        let mut glyph = Glyph {
            left: value(bytes[0]),
            right: value(bytes[1]),
            strokes: vec![Vec::new()],
        };
        for pair in bytes[2..(count * 2).min(bytes.len())].as_chunks::<2>().0 {
            if pair == b" R" {
                glyph.strokes.push(Vec::new());
            } else {
                glyph
                    .strokes
                    .last_mut()
                    .unwrap()
                    .push((value(pair[0]), value(pair[1])));
            }
        }
        glyph.strokes.retain(|stroke| stroke.len() >= 2);
        glyphs.push(glyph);
    }
    glyphs
}

/// Single-line outlines of `text` in mm, first baseline on Y = 0.
pub fn hershey_text(text: &str, family: &str, size: f64, line_spacing: f64) -> Vec<Path> {
    let Some((_, data)) = FONTS.iter().find(|(name, _)| *name == family) else {
        return Vec::new();
    };
    let glyphs = parse(data);
    let scale = size / UNITS_PER_EM;
    let mut paths = Vec::new();
    for (row, line) in text.lines().enumerate() {
        let baseline = -(row as f64) * size * line_spacing;
        let mut cursor = 0.0;
        for character in line.chars() {
            let index = (character as u32).wrapping_sub(32) as usize;
            let Some(glyph) = glyphs
                .get(index)
                .or_else(|| glyphs.get(('?' as u32 - 32) as usize))
            else {
                continue;
            };
            for stroke in &glyph.strokes {
                // Hershey Y points down with the baseline near +9.
                let points = stroke
                    .iter()
                    .map(|(x, y)| {
                        Point::new(
                            cursor + (x - glyph.left) as f64 * scale,
                            baseline + (9 - y) as f64 * scale,
                        )
                    })
                    .collect();
                paths.push(Path::new(points, false));
            }
            cursor += (glyph.right - glyph.left) as f64 * scale;
        }
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_font_has_the_printable_ascii_range() {
        for (name, data) in FONTS {
            assert!(parse(data).len() >= 95, "{name}");
        }
    }

    #[test]
    fn letters_are_open_strokes_of_the_requested_size() {
        let paths = hershey_text("H", "Hershey Sans", 32.0, 1.2);
        assert_eq!(paths.len(), 3, "H is two verticals and a bar");
        assert!(paths.iter().all(|path| !path.closed));
        let bounds = mml_core::geometry::bounds_of(&paths);
        assert!(
            (bounds.height() - 21.0).abs() < 1e-9,
            "cap height of 21 units"
        );
        assert!(bounds.min.y.abs() < 1e-9, "sits on the baseline");
    }
}
