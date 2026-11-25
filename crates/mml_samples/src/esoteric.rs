//! Symbols and esoteric objects: sacred geometry, runes, a talking
//! board.

use crate::shapes::*;
use crate::tests_projects::text_language;
use crate::{BoardOps, Builder, settings};
use mml_core::geometry::{Path, Point, Transform};
use mml_core::layer::ProfileSide;
use mml_core::region::{Boolean, combine};
use mml_core::{Document, MachineKind};

/// Flower of life: 19 circles of `radius` on a hexagonal grid.
fn flower_of_life(centre: Point, radius: f64) -> Vec<Path> {
    let mut lines = Vec::new();
    let (dx, dy) = (radius, radius * 3f64.sqrt() / 2.0);
    for row in -2i32..=2 {
        let count = 5 - row.unsigned_abs() as i32;
        for column in 0..count {
            let x = (column as f64 - (count - 1) as f64 / 2.0) * dx;
            lines.push(circle(centre + point(x, row as f64 * dy), radius));
        }
    }
    lines.push(circle(centre, radius * 3.0));
    lines.push(circle(centre, radius * 3.0 + 1.5));
    lines
}

/// Metatron's cube: the 13 circles of the fruit of life and every line
/// between their centres.
fn metatron(centre: Point, radius: f64) -> Vec<Path> {
    let mut centres = vec![centre];
    for distance in [2.0, 4.0] {
        for index in 0..6 {
            centres.push(polar(centre, radius * distance, 90.0 + index as f64 * 60.0));
        }
    }
    let mut lines: Vec<Path> = centres.iter().map(|c| circle(*c, radius)).collect();
    for (index, a) in centres.iter().enumerate() {
        for b in &centres[index + 1..] {
            lines.push(line(*a, *b));
        }
    }
    lines
}

/// Tree of life: ten circles (sephirot) and the 22 paths between them,
/// each path stopping at the circles.
fn tree_of_life(centre: Point, unit: f64, radius: f64) -> Vec<Path> {
    // Classic layout on the vesica construction, y downwards from the
    // crown; columns at -1, 0 and 1.
    let nodes = [
        (0.0, 0.0),
        (1.0, 0.58),
        (-1.0, 0.58),
        (1.0, 1.73),
        (-1.0, 1.73),
        (0.0, 2.31),
        (1.0, 2.89),
        (-1.0, 2.89),
        (0.0, 3.46),
        (0.0, 4.62),
    ];
    let at = |index: usize| {
        let (x, y) = nodes[index];
        centre + point(x * unit, (2.31 - y) * unit)
    };
    const PATHS: [(usize, usize); 22] = [
        (1, 2),
        (1, 3),
        (1, 6),
        (2, 3),
        (2, 4),
        (2, 6),
        (3, 5),
        (3, 6),
        (4, 5),
        (4, 6),
        (4, 7),
        (5, 6),
        (5, 8),
        (6, 7),
        (6, 8),
        (6, 9),
        (7, 8),
        (7, 9),
        (7, 10),
        (8, 9),
        (8, 10),
        (9, 10),
    ];
    let mut lines = Vec::new();
    for (a, b) in PATHS {
        let (a, b) = (at(a - 1), at(b - 1));
        let along = (b - a).normalized() * radius;
        lines.push(line(a + along, b - along));
    }
    for index in 0..nodes.len() {
        lines.push(circle(at(index), radius));
        lines.push(circle(at(index), radius * 0.6));
    }
    lines
}

/// Three medallions of sacred geometry: flower of life, Metatron's
/// cube and the tree of life, engraved and cut in 3 mm plywood.
pub fn sacred_geometry(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "sacred_geometry");
    b.stock_board();
    let centre = point(0.0, 0.0);
    let disc = 40.0;
    let designs: [(&str, Vec<Path>); 3] = [
        ("part.flower", flower_of_life(centre, 11.0)),
        ("part.metatron", metatron(centre, 6.6)),
        ("part.tree", tree_of_life(centre, 12.5, 5.0)),
    ];
    let mut parts = Vec::new();
    for (key, mut lines) in designs {
        // The flower has its own border circles.
        if key != "part.flower" {
            lines.push(circle(centre, 34.5));
        }
        // A hole to hang it, clear of the border.
        parts.push(
            Part::new(b.t(key), circle(centre, disc))
                .with_holes(vec![circle(point(0.0, disc - 2.4), 1.2)])
                .with_engraving(lines),
        );
    }
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
    b.finish(MachineKind::Laser)
}

/// Raised sun: a disc and twelve separate rays.
fn relief_sun() -> Vec<Path> {
    let centre = point(0.0, 0.0);
    let mut shapes = vec![vec![circle(centre, 11.0)]];
    for angle in (0..12).map(|index| index as f64 * 30.0) {
        shapes.push(vec![Path::new(
            vec![
                polar(centre, 14.5, angle - 6.5),
                polar(centre, 27.0, angle),
                polar(centre, 14.5, angle + 6.5),
            ],
            true,
        )]);
    }
    union(shapes)
}

/// Triple moon: waxing crescent, full moon, waning crescent, the
/// crescents' backs turned to the full moon.
fn relief_triple_moon() -> Vec<Path> {
    let crescent = |side: f64| {
        difference(vec![
            vec![circle(point(24.0 * side, 0.0), 9.0)],
            vec![circle(point(27.5 * side, 0.0), 8.5)],
        ])
    };
    let mut shapes = vec![vec![circle(point(0.0, 0.0), 9.0)]];
    shapes.push(crescent(-1.0));
    shapes.push(crescent(1.0));
    // Three small stars above and below.
    for (x, y) in [(0.0, 20.0), (-12.0, -19.0), (12.0, -19.0)] {
        shapes.push(vec![star(point(x, y), 4.2, 1.8)]);
    }
    union(shapes)
}

/// Five-pointed star, `outer` and `inner` radii, a point upwards.
fn star(centre: Point, outer: f64, inner: f64) -> Path {
    let points = (0..10)
        .map(|index| {
            let radius = if index % 2 == 0 { outer } else { inner };
            polar(centre, radius, 90.0 + index as f64 * 36.0)
        })
        .collect();
    Path::new(points, true)
}

/// Hexafoil (daisy wheel, an old protective mark carved on beams and
/// chests): six petals drawn with the compass, kept apart for the tool,
/// inside a ring.
fn relief_hexafoil() -> Vec<Path> {
    let centre = point(0.0, 0.0);
    let radius = 28.0;
    let mut shapes = Vec::new();
    for angle in (0..6).map(|index| index as f64 * 60.0 + 30.0) {
        // The two circles through the centre and the petal tip.
        let petal = combine(
            &[
                vec![circle(polar(centre, radius, angle - 60.0), radius)],
                vec![circle(polar(centre, radius, angle + 60.0), radius)],
            ],
            Boolean::Intersection,
        )
        .unwrap_or_default();
        shapes.push(mml_core::region::offset_paths(&petal, -1.3));
    }
    shapes.push(vec![circle(centre, 33.5), circle(centre, 30.5)]);
    shapes.push(vec![circle(centre, 3.0)]);
    union(shapes)
}

/// Tree of life in relief: the ten sephirot as bosses joined by the 22
/// paths as ridges.
fn relief_tree_of_life() -> Vec<Path> {
    let lines = tree_of_life(point(0.0, 0.0), 12.0, 4.8);
    let (paths, rings): (Vec<Path>, Vec<Path>) = lines.into_iter().partition(|path| !path.closed);
    let mut shapes: Vec<Vec<Path>> = paths
        .iter()
        .map(|path| mml_core::region::offset_paths(std::slice::from_ref(path), 1.3))
        .collect();
    // Only the outer circle of each sephira.
    shapes.extend(rings.into_iter().step_by(2).map(|ring| vec![ring]));
    union(shapes)
}

/// Four medallions carved in relief in hardwood: a raised sun, the
/// triple moon, a hexafoil and the tree of life, then cut out.
pub fn relief_medallions(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "relief_medallions");
    b.stock(190.0, 190.0);
    let thickness = 15.0;
    let relief = b.layer(b.t("layer.relief"), settings::relief(3.0, 2.5));
    let outline = b.layer(
        b.t("layer.outline"),
        settings::profile(6.0, thickness + 0.3, ProfileSide::Outside, 4),
    );
    let designs: [(&str, Vec<Path>); 4] = [
        ("part.sun", relief_sun()),
        ("part.moon", relief_triple_moon()),
        ("part.hexafoil", relief_hexafoil()),
        ("part.tree", relief_tree_of_life()),
    ];
    for (index, (key, motifs)) in designs.into_iter().enumerate() {
        let centre = point(
            60.0 + (index % 2) as f64 * 90.0,
            150.0 - (index / 2) as f64 * 90.0,
        );
        // The carved area, a rim at full height around it, then the
        // motifs standing inside.
        let mut paths = vec![circle(centre, RELIEF_AREA)];
        paths.extend(translate(&motifs, centre.x, centre.y));
        b.add(relief, b.t(key), paths);
        b.add(outline, b.t(key), vec![circle(centre, 40.0)]);
    }
    b.step("s1", Vec::new());
    b.step("s2", vec![relief]);
    b.step("s3", vec![outline]);
    b.step("s4", Vec::new());
    b.step("s5", Vec::new());
    b.finish(MachineKind::Cnc)
}

/// Radius of the carved area of a relief medallion.
const RELIEF_AREA: f64 = 37.0;

/// Pen strokes, each a polyline.
pub type Strokes = &'static [&'static [(f64, f64)]];

/// The 24 runes of the elder futhark as strokes in a box 0.6 wide and
/// 1 tall centred on the origin, in futhark order.
pub const FUTHARK: [(&str, Strokes); 24] = [
    (
        "fehu",
        &[
            &[(-0.2, -0.5), (-0.2, 0.5)],
            &[(-0.2, 0.15), (0.25, 0.45)],
            &[(-0.2, -0.15), (0.25, 0.15)],
        ],
    ),
    (
        "uruz",
        &[&[(-0.25, -0.5), (-0.25, 0.5), (0.25, 0.2), (0.25, -0.5)]],
    ),
    (
        "thurisaz",
        &[
            &[(-0.2, -0.5), (-0.2, 0.5)],
            &[(-0.2, 0.3), (0.2, 0.0), (-0.2, -0.3)],
        ],
    ),
    (
        "ansuz",
        &[
            &[(-0.2, -0.5), (-0.2, 0.5)],
            &[(-0.2, 0.5), (0.25, 0.2)],
            &[(-0.2, 0.2), (0.25, -0.1)],
        ],
    ),
    (
        "raido",
        &[
            &[(-0.2, -0.5), (-0.2, 0.5)],
            &[(-0.2, 0.5), (0.2, 0.25), (-0.2, 0.0), (0.2, -0.5)],
        ],
    ),
    ("kaunan", &[&[(0.15, 0.3), (-0.15, 0.0), (0.15, -0.3)]]),
    (
        "gebo",
        &[&[(-0.25, -0.5), (0.25, 0.5)], &[(-0.25, 0.5), (0.25, -0.5)]],
    ),
    (
        "wunjo",
        &[
            &[(-0.2, -0.5), (-0.2, 0.5)],
            &[(-0.2, 0.5), (0.2, 0.25), (-0.2, 0.0)],
        ],
    ),
    (
        "hagalaz",
        &[
            &[(-0.2, -0.5), (-0.2, 0.5)],
            &[(0.2, -0.5), (0.2, 0.5)],
            &[(-0.2, 0.15), (0.2, -0.15)],
        ],
    ),
    (
        "naudiz",
        &[&[(0.0, -0.5), (0.0, 0.5)], &[(-0.2, 0.2), (0.2, -0.1)]],
    ),
    ("isa", &[&[(0.0, -0.5), (0.0, 0.5)]]),
    (
        "jera",
        &[
            &[(-0.05, 0.4), (-0.25, 0.15), (-0.05, -0.1)],
            &[(0.05, 0.1), (0.25, -0.15), (0.05, -0.4)],
        ],
    ),
    (
        "eihwaz",
        &[
            &[(0.0, -0.5), (0.0, 0.5)],
            &[(0.0, 0.5), (0.2, 0.3)],
            &[(0.0, -0.5), (-0.2, -0.3)],
        ],
    ),
    (
        "perthro",
        &[
            &[(-0.2, -0.5), (-0.2, 0.5)],
            &[(-0.2, 0.5), (0.05, 0.25), (0.25, 0.5)],
            &[(-0.2, -0.5), (0.05, -0.25), (0.25, -0.5)],
        ],
    ),
    (
        "algiz",
        &[
            &[(0.0, -0.5), (0.0, 0.5)],
            &[(0.0, 0.1), (-0.25, 0.45)],
            &[(0.0, 0.1), (0.25, 0.45)],
        ],
    ),
    (
        "sowilo",
        &[&[(0.15, 0.5), (-0.2, 0.15), (0.2, -0.15), (-0.15, -0.5)]],
    ),
    (
        "tiwaz",
        &[
            &[(0.0, -0.5), (0.0, 0.5)],
            &[(-0.25, 0.25), (0.0, 0.5), (0.25, 0.25)],
        ],
    ),
    (
        "berkanan",
        &[
            &[(-0.2, -0.5), (-0.2, 0.5)],
            &[
                (-0.2, 0.5),
                (0.2, 0.25),
                (-0.2, 0.0),
                (0.2, -0.25),
                (-0.2, -0.5),
            ],
        ],
    ),
    (
        "ehwaz",
        &[&[
            (-0.25, -0.5),
            (-0.25, 0.5),
            (0.0, 0.2),
            (0.25, 0.5),
            (0.25, -0.5),
        ]],
    ),
    (
        "mannaz",
        &[
            &[(-0.25, -0.5), (-0.25, 0.5)],
            &[(0.25, -0.5), (0.25, 0.5)],
            &[(-0.25, 0.5), (0.25, 0.1)],
            &[(0.25, 0.5), (-0.25, 0.1)],
        ],
    ),
    (
        "laguz",
        &[&[(-0.15, -0.5), (-0.15, 0.5)], &[(-0.15, 0.5), (0.2, 0.25)]],
    ),
    (
        "ingwaz",
        &[&[
            (0.0, 0.3),
            (0.25, 0.0),
            (0.0, -0.3),
            (-0.25, 0.0),
            (0.0, 0.3),
        ]],
    ),
    (
        "dagaz",
        &[&[
            (-0.25, -0.4),
            (-0.25, 0.4),
            (0.25, -0.4),
            (0.25, 0.4),
            (-0.25, -0.4),
        ]],
    ),
    (
        "othala",
        &[
            &[(0.0, 0.5), (0.25, 0.15), (-0.2, -0.5)],
            &[(0.0, 0.5), (-0.25, 0.15), (0.2, -0.5)],
        ],
    ),
];

/// A rune `height` tall centred on `centre`.
pub fn rune(strokes: &[&[(f64, f64)]], centre: Point, height: f64) -> Vec<Path> {
    strokes
        .iter()
        .map(|stroke| {
            Path::new(
                stroke
                    .iter()
                    .map(|(x, y)| centre + point(x * height, y * height))
                    .collect(),
                false,
            )
        })
        .collect()
}

/// A set of 25 rune stones: the elder futhark V-carved on wooden discs,
/// plus a blank one.
pub fn runes(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "runes");
    b.stock(180.0, 180.0);
    let thickness = 6.0;
    let engrave = b.layer(b.t("layer.engrave"), settings::v_line(0.8));
    let discs = b.layer(
        b.t("layer.discs"),
        settings::profile(3.175, thickness + 0.3, ProfileSide::Outside, 2),
    );
    for index in 0..25 {
        let centre = point(
            32.0 + (index % 5) as f64 * 34.0,
            168.0 - (index / 5) as f64 * 34.0,
        );
        let name = match FUTHARK.get(index) {
            Some((name, strokes)) => {
                b.add(engrave, *name, rune(strokes, centre, 16.0));
                (*name).to_owned()
            }
            None => b.t("part.blank"),
        };
        b.add(discs, name, vec![circle(centre, 14.0)]);
    }
    b.step("s1", Vec::new());
    b.step("s2", vec![engrave]);
    b.step("s3", vec![discs]);
    b.step("s4", Vec::new());
    b.step("s5", Vec::new());
    b.finish(MachineKind::Cnc)
}

/// Sun: a disc and its rays.
fn sun(centre: Point, radius: f64) -> Vec<Path> {
    let mut shapes = vec![vec![circle(centre, radius * 0.55)]];
    for index in 0..12 {
        let angle = index as f64 * 30.0;
        shapes.push(vec![Path::new(
            vec![
                polar(centre, radius * 0.65, angle - 8.0),
                polar(centre, radius, angle),
                polar(centre, radius * 0.65, angle + 8.0),
            ],
            true,
        )]);
    }
    union(shapes)
}

/// Crescent moon opening to the right.
fn moon(centre: Point, radius: f64) -> Vec<Path> {
    difference(vec![
        vec![circle(centre, radius)],
        vec![circle(
            centre + point(radius * 0.45, radius * 0.15),
            radius * 0.85,
        )],
    ])
}

/// `value` with each letter on an arc around `centre` at `radius`,
/// spread over `step` degrees per letter, upright towards the outside.
fn arc_text(
    value: &str,
    family: &str,
    size: f64,
    centre: Point,
    radius: f64,
    step: f64,
) -> Vec<Path> {
    let letters: Vec<char> = value.chars().collect();
    let first = 90.0 + step * (letters.len() as f64 - 1.0) / 2.0;
    let mut paths = Vec::new();
    for (index, letter) in letters.iter().enumerate() {
        let angle = first - step * index as f64;
        let at = polar(centre, radius, angle);
        let glyph = text(&letter.to_string(), family, size, at);
        paths.extend(transform(
            &glyph,
            &Transform::rotate_degrees(angle - 90.0).around(at),
        ));
    }
    paths
}

/// A talking board and its planchette in 3 mm plywood: letters on two
/// arcs, numbers, yes, no and goodbye in the language of the guide.
pub fn spirit_board(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "spirit_board");
    b.stock_board();
    let font = "Cinzel Decorative";
    // Fits a board once its 5 mm margins are taken off.
    let (width, height) = (244.0, 164.0);
    let arc_centre = point(width / 2.0, -60.0);
    let mut marks = Vec::new();
    marks.extend(arc_text(
        "ABCDEFGHIJKLM",
        font,
        12.5,
        arc_centre,
        180.0,
        4.9,
    ));
    marks.extend(arc_text(
        "NOPQRSTUVWXYZ",
        font,
        12.0,
        arc_centre,
        141.0,
        6.1,
    ));
    marks.extend(text(
        "1 2 3 4 5 6 7 8 9 0",
        font,
        11.0,
        point(width / 2.0, 40.0),
    ));
    marks.extend(text(
        &b.t("word.goodbye"),
        font,
        11.0,
        point(width / 2.0, 19.0),
    ));
    marks.extend(text(&b.t("word.yes"), font, 13.0, point(70.0, 143.0)));
    marks.extend(text(&b.t("word.no"), font, 13.0, point(174.0, 143.0)));
    marks.extend(sun(point(27.0, 142.0), 12.0));
    marks.extend(moon(point(217.0, 142.0), 10.0));
    marks.extend(difference(vec![
        vec![rounded_rect(5.0, 5.0, width - 10.0, height - 10.0, 10.0)],
        vec![rounded_rect(6.5, 6.5, width - 13.0, height - 13.0, 8.5)],
    ]));
    let board = Part::new(
        b.t("part.board"),
        rounded_rect(0.0, 0.0, width, height, 14.0),
    )
    .with_engraving(marks);
    // Planchette: a drop pointing up, with a window near the tip.
    let drop = union(vec![
        vec![circle(point(0.0, 0.0), 30.0)],
        vec![polygon(&[(-27.5, 12.0), (27.5, 12.0), (0.0, 62.0)])],
    ]);
    let window = point(0.0, 24.0);
    let planchette = Part::from_area(b.t("part.planchette"), drop)
        .with_holes(vec![circle(window, 8.0)])
        .with_engraving(difference(vec![
            vec![circle(window, 11.5)],
            vec![circle(window, 10.0)],
        ]));
    b.step("s1", Vec::new());
    b.cut_boards(
        boards(vec![board, planchette], 4.0),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relief_motifs_leave_room_for_the_ball_inside_the_area() {
        // A 3.175 mm ball must pass between the motifs and the edge of
        // the carved area.
        for motifs in [
            relief_sun(),
            relief_triple_moon(),
            relief_hexafoil(),
            relief_tree_of_life(),
        ] {
            assert!(!motifs.is_empty());
            for path in &motifs {
                for point in &path.points {
                    assert!(point.length() <= RELIEF_AREA - 3.2, "{point:?}");
                }
            }
        }
        // The petals joined by the centre boss, and the ring (outside
        // and inside).
        assert_eq!(relief_hexafoil().len(), 3);
        assert_eq!(relief_sun().len(), 13);
    }

    #[test]
    fn runes_fit_their_stones() {
        assert_eq!(FUTHARK.len(), 24);
        for (name, strokes) in FUTHARK {
            for stroke in strokes {
                assert!(stroke.len() >= 2, "{name}");
                for (x, y) in stroke.iter() {
                    assert!(x.abs() <= 0.3 && y.abs() <= 0.5, "{name}");
                }
            }
        }
        // 16 mm runes stay inside 28 mm stones with a margin.
        let half_diagonal = (0.3f64.powi(2) + 0.5f64.powi(2)).sqrt() * 16.0;
        assert!(half_diagonal < 14.0 - 3.0);
    }

    #[test]
    fn sacred_designs_stay_inside_their_medallion() {
        let centre = point(0.0, 0.0);
        for design in [
            flower_of_life(centre, 11.0),
            metatron(centre, 6.6),
            tree_of_life(centre, 12.5, 5.0),
        ] {
            let bounds = mml_core::geometry::bounds_of(&design);
            for corner in [bounds.min, bounds.max] {
                assert!(
                    corner.x.abs() <= 35.0 && corner.y.abs() <= 35.0,
                    "{bounds:?}"
                );
            }
        }
        // 13 circles and the 78 lines between their centres.
        assert_eq!(metatron(centre, 6.6).len(), 13 + 78);
    }
}
