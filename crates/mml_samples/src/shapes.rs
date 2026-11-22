//! Geometry helpers shared by the sample projects, and the parts they
//! are made of.

use mml_core::geometry::{Path, Point, Rect, Transform, bounds_of};
use mml_core::nest::{NestParams, nest};
use mml_core::region::{Boolean, combine};
use std::f64::consts::TAU;

pub fn point(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

/// Point at `radius` from `centre`, `degrees` counter-clockwise from +X.
pub fn polar(centre: Point, radius: f64, degrees: f64) -> Point {
    let angle = degrees.to_radians();
    centre + Point::new(angle.cos(), angle.sin()) * radius
}

pub fn circle(centre: Point, radius: f64) -> Path {
    let count = ((radius * 4.0).ceil() as usize).clamp(24, 360);
    Path::new(
        (0..count)
            .map(|index| polar(centre, radius, index as f64 / count as f64 * 360.0))
            .collect(),
        true,
    )
}

pub fn rect(x: f64, y: f64, width: f64, height: f64) -> Path {
    Path::new(
        vec![
            point(x, y),
            point(x + width, y),
            point(x + width, y + height),
            point(x, y + height),
        ],
        true,
    )
}

pub fn rounded_rect(x: f64, y: f64, width: f64, height: f64, radius: f64) -> Path {
    let radius = radius.min(width / 2.0).min(height / 2.0);
    if radius <= 0.0 {
        return rect(x, y, width, height);
    }
    let corners = [
        (point(x + width - radius, y + radius), -90.0),
        (point(x + width - radius, y + height - radius), 0.0),
        (point(x + radius, y + height - radius), 90.0),
        (point(x + radius, y + radius), 180.0),
    ];
    let steps = 8;
    let mut points = Vec::new();
    for (centre, start) in corners {
        for step in 0..=steps {
            points.push(polar(
                centre,
                radius,
                start + 90.0 * step as f64 / steps as f64,
            ));
        }
    }
    Path::new(points, true)
}

pub fn polygon(points: &[(f64, f64)]) -> Path {
    Path::new(points.iter().map(|(x, y)| point(*x, *y)).collect(), true)
}

pub fn line(a: Point, b: Point) -> Path {
    Path::new(vec![a, b], false)
}

/// A bar from `a` to `b`, `width` wide, with round ends.
pub fn bar(a: Point, b: Point, width: f64) -> Vec<Path> {
    let side = (b - a).normalized().perpendicular() * (width / 2.0);
    let body = Path::new(vec![a + side, b + side, b - side, a - side], true);
    union(vec![
        vec![body],
        vec![circle(a, width / 2.0)],
        vec![circle(b, width / 2.0)],
    ])
}

pub fn transform(paths: &[Path], transform: &Transform) -> Vec<Path> {
    paths
        .iter()
        .map(|path| path.transformed(transform))
        .collect()
}

pub fn translate(paths: &[Path], dx: f64, dy: f64) -> Vec<Path> {
    transform(paths, &Transform::translate(dx, dy))
}

/// Area covered by any of the shapes (each a list of closed paths).
pub fn union(shapes: Vec<Vec<Path>>) -> Vec<Path> {
    combine(&shapes, Boolean::Union).unwrap_or_default()
}

/// The first shape minus the others.
pub fn difference(shapes: Vec<Vec<Path>>) -> Vec<Path> {
    combine(&shapes, Boolean::Difference).unwrap_or_default()
}

/// Text outlines (or single strokes for the Hershey families) whose
/// bounding box is centred on `centre`, `size` mm tall as a font size.
pub fn text(text: &str, family: &str, size: f64, centre: Point) -> Vec<Path> {
    let style = mml_io::TextStyle {
        family: family.into(),
        size,
        ..mml_io::TextStyle::default()
    };
    let paths = mml_io::text_to_paths(text, &style);
    let bounds = bounds_of(&paths);
    if bounds.is_empty() {
        return paths;
    }
    let middle = bounds.center();
    translate(&paths, centre.x - middle.x, centre.y - middle.y)
}

/// Single-stroke text, for engraving with a line or a V-bit.
pub fn stroke_text(value: &str, size: f64, centre: Point) -> Vec<Path> {
    text(value, "Hershey Sans", size, centre)
}

/// A part cut out of the stock: its outline, holes (cut first, from
/// the inside) and lines engraved on it.
#[derive(Debug, Clone)]
pub struct Part {
    pub name: String,
    pub outline: Path,
    pub holes: Vec<Path>,
    pub engrave: Vec<Path>,
}

impl Part {
    pub fn new(name: impl Into<String>, outline: Path) -> Self {
        Self {
            name: name.into(),
            outline,
            holes: Vec::new(),
            engrave: Vec::new(),
        }
    }

    /// The part of an area made of several closed paths: the largest is
    /// the outline, the others are holes.
    pub fn from_area(name: impl Into<String>, mut area: Vec<Path>) -> Self {
        area.sort_by(|a, b| b.signed_area().abs().total_cmp(&a.signed_area().abs()));
        let outline = area.remove(0);
        Self {
            name: name.into(),
            outline,
            holes: area,
            engrave: Vec::new(),
        }
    }

    pub fn with_holes(mut self, holes: Vec<Path>) -> Self {
        self.holes.extend(holes);
        self
    }

    pub fn with_engraving(mut self, lines: Vec<Path>) -> Self {
        self.engrave.extend(lines);
        self
    }

    pub fn bounds(&self) -> Rect {
        self.outline.bounds()
    }

    pub fn moved(&self, transform: &Transform) -> Part {
        Part {
            name: self.name.clone(),
            outline: self.outline.transformed(transform),
            holes: self::transform(&self.holes, transform),
            engrave: self::transform(&self.engrave, transform),
        }
    }

    pub fn at(&self, x: f64, y: f64) -> Part {
        let bounds = self.bounds();
        self.moved(&Transform::translate(x - bounds.min.x, y - bounds.min.y))
    }
}

/// Usable size of one board: inside the HiMill travel (280 x 200) and
/// most diode laser beds, with a margin.
pub const BOARD: (f64, f64) = (260.0, 180.0);
/// Where the front left corner of the board goes, from the origin.
pub const BOARD_AT: (f64, f64) = (10.0, 10.0);

/// Shares `parts` out between as many boards as needed, each nested
/// from the board corner; a part too big for an empty board is left
/// out (none of ours is).
pub fn boards(parts: Vec<Part>, spacing: f64) -> Vec<Vec<Part>> {
    let params = NestParams {
        sheet: BOARD,
        margin: 5.0,
        spacing,
        rotate: true,
    };
    let mut left = parts;
    let mut result = Vec::new();
    while !left.is_empty() {
        let outlines: Vec<Vec<Path>> = left.iter().map(|part| vec![part.outline.clone()]).collect();
        let nesting = nest(&outlines, &params);
        let mut board = Vec::new();
        let mut rest = Vec::new();
        for (part, placement) in left.into_iter().zip(nesting.placements) {
            match placement {
                Some(placement) => board.push(
                    part.moved(&placement.then(Transform::translate(BOARD_AT.0, BOARD_AT.1))),
                ),
                None => rest.push(part),
            }
        }
        if board.is_empty() {
            break;
        }
        result.push(board);
        left = rest;
    }
    result
}

/// Outline of the stock, drawn on a reference layer.
pub fn board_outline(width: f64, height: f64) -> Path {
    rect(BOARD_AT.0, BOARD_AT.1, width, height)
}

/// Spur gear centred on `centre`, with its bore.
pub fn gear(teeth: usize, module: f64, bore: f64, centre: Point) -> (Path, Vec<Path>) {
    let params = mml_core::makers::gear::GearParams {
        teeth,
        module,
        pressure_angle: 20.0,
        bore: 0.0,
        backlash: 0.15,
    };
    let outline = mml_core::makers::gear::generate(&params)
        .into_iter()
        .max_by(|a, b| a.signed_area().abs().total_cmp(&b.signed_area().abs()))
        .expect("a gear has an outline");
    let outline = outline.transformed(&Transform::translate(centre.x, centre.y));
    let holes = if bore > 0.0 {
        vec![circle(centre, bore / 2.0)]
    } else {
        Vec::new()
    };
    (outline, holes)
}

/// Windows between `spokes` spokes of a wheel, from `hub` to `rim`
/// radius, spokes `width` wide.
pub fn spoke_windows(centre: Point, hub: f64, rim: f64, spokes: usize, width: f64) -> Vec<Path> {
    if rim - hub < 6.0 {
        return Vec::new();
    }
    let ring = difference(vec![vec![circle(centre, rim)], vec![circle(centre, hub)]]);
    let arms: Vec<Vec<Path>> = (0..spokes)
        .map(|index| {
            let angle = 90.0 + index as f64 * 360.0 / spokes as f64;
            bar(centre, polar(centre, rim + 2.0, angle), width)
        })
        .collect();
    let mut shapes = vec![ring];
    shapes.extend(arms);
    let windows = difference(shapes);
    // Rounded corners: shrink then grow by the cutter radius.
    let shrunk = mml_core::region::offset_paths(&windows, -1.5);
    mml_core::region::offset_paths(&shrunk, 1.5)
}

pub fn full_turn(count: usize) -> impl Iterator<Item = f64> {
    (0..count).map(move |index| index as f64 * TAU / count as f64)
}
