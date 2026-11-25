//! Puzzles and games.

use crate::shapes::*;
use crate::tests_projects::text_language;
use crate::{BoardOps, Builder, settings};
use mml_core::geometry::Path;
use mml_core::layer::ProfileSide;
use mml_core::makers::maze::{MazeOutput, MazeParams, generate};
use mml_core::region::offset_paths;
use mml_core::{Document, MachineKind};

/// Marble maze milled in 12 mm plywood under a clear 3 mm cover.
pub fn marble_maze(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "marble_maze");
    b.stock(190.0, 150.0);
    let (columns, rows, cell) = (10, 7, 15.0);
    let (mx, my) = (30.0, 32.5);
    let maze = generate(&MazeParams {
        columns,
        rows,
        cell,
        output: MazeOutput::Corridors,
        seed: 2026,
    });
    let corridors = b.layer(
        b.t("layer.corridors"),
        settings::profile(6.0, 5.0, ProfileSide::On, 0),
    );
    let wells = b.layer(b.t("layer.wells"), settings::pocket(6.0, 6.0));
    let outline = b.layer(
        b.t("layer.outline"),
        settings::profile(3.175, 12.3, ProfileSide::Outside, 4),
    );
    let cover_holes = b.layer(
        b.t("layer.cover_holes"),
        settings::profile(3.175, 3.3, ProfileSide::Inside, 0),
    );
    let cover = b.layer(
        b.t("layer.cover"),
        settings::profile(3.175, 3.3, ProfileSide::Outside, 4),
    );
    b.add(
        corridors,
        b.t("layer.corridors"),
        translate(&maze.paths, mx, my),
    );
    let start = point(mx + cell / 2.0, my + cell / 2.0);
    let finish = point(
        mx + (columns as f64 - 0.5) * cell,
        my + (rows as f64 - 0.5) * cell,
    );
    b.add(
        wells,
        b.t("layer.wells"),
        vec![circle(start, 5.0), circle(finish, 5.0)],
    );
    let board = rounded_rect(15.0, 15.0, 180.0, 140.0, 8.0);
    b.add(outline, b.t("layer.outline"), vec![board.clone()]);
    let corners = [(25.0, 25.0), (185.0, 25.0), (25.0, 145.0), (185.0, 145.0)];
    b.add(
        cover_holes,
        b.t("layer.cover_holes"),
        corners
            .iter()
            .map(|(x, y)| circle(point(*x, *y), 2.0))
            .collect(),
    );
    b.add(cover, b.t("layer.cover"), vec![board]);
    b.step("s1", Vec::new());
    b.step("s2", vec![corridors, wells]);
    b.step("s3", vec![outline]);
    b.step("s4", vec![cover_holes, cover]);
    b.step("s5", Vec::new());
    b.finish(MachineKind::Cnc)
}

/// Tower of Hanoi: seven discs and a base in 12 mm plywood.
pub fn hanoi(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "hanoi");
    b.stock_board();
    let mut parts = Vec::new();
    let mut base = Part::new(b.t("part.base"), rounded_rect(0.0, 0.0, 240.0, 70.0, 10.0));
    base = base.with_holes(
        [40.0, 120.0, 200.0]
            .iter()
            .map(|x| circle(point(*x, 35.0), 5.0))
            .collect(),
    );
    parts.push(base);
    for diameter in [90.0, 80.0, 70.0, 60.0, 50.0, 40.0, 30.0] {
        let centre = point(0.0, 0.0);
        parts.push(
            Part::new(
                b.tf("part.disc", &[&format!("{diameter:.0}")]),
                circle(centre, diameter / 2.0),
            )
            .with_holes(vec![circle(centre, 5.4)]),
        );
    }
    b.step("s1", Vec::new());
    b.cut_boards(
        boards(parts, 8.0),
        BoardOps {
            engrave: None,
            engrave_alone: false,
            holes: settings::profile(3.175, 12.3, ProfileSide::Inside, 0),
            outline: settings::profile(3.175, 12.3, ProfileSide::Outside, 3),
        },
        "board",
    );
    b.step("s2", Vec::new());
    b.step("s3", Vec::new());
    b.finish(MachineKind::Cnc)
}

/// The twelve pentominoes, cells as (column, row), and the cell that
/// carries the letter.
type Cell = (i32, i32);

const PENTOMINOES: [(&str, [Cell; 5], Cell); 12] = [
    ("F", [(1, 0), (1, 1), (1, 2), (0, 1), (2, 2)], (1, 1)),
    ("I", [(0, 0), (0, 1), (0, 2), (0, 3), (0, 4)], (0, 2)),
    ("L", [(0, 0), (0, 1), (0, 2), (0, 3), (1, 0)], (0, 2)),
    ("N", [(0, 0), (0, 1), (1, 1), (1, 2), (1, 3)], (1, 2)),
    ("P", [(0, 0), (0, 1), (0, 2), (1, 1), (1, 2)], (0, 1)),
    ("T", [(0, 2), (1, 2), (2, 2), (1, 1), (1, 0)], (1, 1)),
    ("U", [(0, 0), (1, 0), (2, 0), (0, 1), (2, 1)], (1, 0)),
    ("V", [(0, 0), (1, 0), (2, 0), (0, 1), (0, 2)], (0, 0)),
    ("W", [(0, 0), (1, 0), (1, 1), (2, 1), (2, 2)], (1, 1)),
    ("X", [(1, 0), (0, 1), (1, 1), (2, 1), (1, 2)], (1, 1)),
    ("Y", [(0, 1), (1, 0), (1, 1), (1, 2), (1, 3)], (1, 1)),
    ("Z", [(0, 2), (1, 2), (1, 1), (1, 0), (2, 0)], (1, 1)),
];

/// Pentominoes in 3 mm plywood, with a 6 x 10 tray.
pub fn pentominoes(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "pentominoes");
    b.stock_board();
    let cell = 15.0;
    // Play: each piece 0.15 mm smaller all round, the tray 0.6 mm
    // larger than 6 x 10 cells.
    let clearance = 0.15;
    let mut parts = Vec::new();
    for (letter, cells, label) in PENTOMINOES {
        let squares: Vec<Vec<Path>> = cells
            .iter()
            .map(|(x, y)| vec![rect(*x as f64 * cell, *y as f64 * cell, cell, cell)])
            .collect();
        let area = offset_paths(&union(squares), -clearance);
        let centre = point((label.0 as f64 + 0.5) * cell, (label.1 as f64 + 0.5) * cell);
        parts.push(
            Part::from_area(b.tf("part.piece", &[letter]), area)
                .with_engraving(stroke_text(letter, 8.0, centre)),
        );
    }
    let (inner_w, inner_h, rim) = (6.0 * cell + 0.6, 10.0 * cell + 0.6, 9.0);
    let (outer_w, outer_h) = (inner_w + 2.0 * rim, inner_h + 2.0 * rim);
    parts.push(
        Part::new(b.t("part.frame"), rect(0.0, 0.0, outer_w, outer_h))
            .with_holes(vec![rect(rim, rim, inner_w, inner_h)]),
    );
    parts.push(
        Part::new(b.t("part.base"), rect(0.0, 0.0, outer_w, outer_h)).with_engraving(stroke_text(
            "6 x 10",
            6.0,
            point(outer_w / 2.0, 12.0),
        )),
    );
    b.step("s1", Vec::new());
    b.cut_boards(
        boards(parts, 3.0),
        BoardOps {
            engrave: Some(settings::laser_line()),
            engrave_alone: false,
            holes: settings::laser_cut(),
            outline: settings::laser_cut(),
        },
        "board",
    );
    b.step("s2", Vec::new());
    b.step("s3", Vec::new());
    b.finish(MachineKind::Laser)
}

/// Fifteen puzzle: 15 numbered tiles in a 4 x 4 tray, 3 mm plywood.
pub fn sliding_puzzle(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "sliding_puzzle");
    b.stock_board();
    let tile = 30.0;
    let mut parts = Vec::new();
    for number in 1..=15 {
        let outline = rounded_rect(0.2, 0.2, tile - 0.4, tile - 0.4, 2.0);
        let digits = text(
            &number.to_string(),
            "Montserrat",
            16.0,
            point(tile / 2.0, tile / 2.0),
        );
        parts.push(
            Part::new(b.tf("part.tile", &[&number.to_string()]), outline).with_engraving(digits),
        );
    }
    let (inner, rim) = (4.0 * tile + 0.6, 10.0);
    let outer = inner + 2.0 * rim;
    parts.push(
        Part::new(b.t("part.frame"), rounded_rect(0.0, 0.0, outer, outer, 6.0))
            .with_holes(vec![rect(rim, rim, inner, inner)]),
    );
    parts.push(Part::new(
        b.t("part.base"),
        rounded_rect(0.0, 0.0, outer, outer, 6.0),
    ));
    b.step("s1", Vec::new());
    b.cut_boards(
        boards(parts, 3.0),
        BoardOps {
            engrave: Some(settings::laser_fill()),
            engrave_alone: false,
            holes: settings::laser_cut(),
            outline: settings::laser_cut(),
        },
        "board",
    );
    b.step("s2", Vec::new());
    b.step("s3", Vec::new());
    b.finish(MachineKind::Laser)
}
