//! Layered decorations: several sheets of 3 mm plywood cut to the same
//! outline and glued in a stack, each opening onto the ones behind (a
//! forest in front of mountains in front of the sky…). Every sheet keeps
//! a frame along the outline, so each scene element is held by it, and
//! two pin holes through all the frames line the sheets up for gluing.

use crate::shapes::*;
use crate::tests_projects::text_language;
use crate::{BoardOps, Builder, settings};
use mml_core::geometry::{Path, Point};
use mml_core::region::{Boolean, combine, offset_paths, region_of, to_path};
use mml_core::{Document, MachineKind};

/// Small deterministic random numbers, so a project is the same every
/// time it is opened.
pub struct Random(u64);

impl Random {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(6364136223846793005).wrapping_add(1))
    }

    /// A number between 0 and 1.
    pub fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn between(&mut self, low: f64, high: f64) -> f64 {
        low + (high - low) * self.next()
    }
}

/// A spiky fir standing on `base`: a short trunk and tiers of branches
/// drooping to points, narrower towards the top.
pub fn fir(base: Point, height: f64) -> Path {
    fir_with(base, height, 0.42)
}

/// `fir` whose branches come back in to `inner` of their reach (wider
/// trees for wider values).
pub fn fir_with(base: Point, height: f64, inner: f64) -> Path {
    let width = height * 0.46;
    let trunk = height * 0.08;
    let tiers = ((height / 7.0).round() as usize).clamp(4, 9);
    let tier = (height - trunk) / tiers as f64;
    let mut right = vec![(width * 0.06, 0.0), (width * 0.06, trunk)];
    for index in 0..tiers {
        let y = trunk + index as f64 * tier;
        let half = width / 2.0 * (1.0 - index as f64 / tiers as f64 * 0.88);
        // Out to the drooping tip, then back in under the next tier.
        right.push((half, y - tier * 0.25));
        right.push((half * inner, y + tier * 0.55));
    }
    right.push((0.0, height));
    let mut points: Vec<(f64, f64)> = right.clone();
    // The left side, from the apex down to the foot of the trunk.
    points.extend(right.iter().rev().skip(1).map(|(x, y)| (-x, *y)));
    Path::new(
        points
            .into_iter()
            .map(|(x, y)| point(base.x + x, base.y + y))
            .collect(),
        true,
    )
}

/// A row of firs from `x0` to `x1` standing on a band of ground whose
/// top is at `ground`, the band going down to `bottom`.
pub fn forest(
    random: &mut Random,
    x0: f64,
    x1: f64,
    ground: f64,
    bottom: f64,
    heights: (f64, f64),
    spacing: f64,
) -> Vec<Path> {
    let mut shapes = vec![vec![rect(x0, bottom, x1 - x0, ground - bottom + 1.0)]];
    let mut x = x0 + random.between(0.0, spacing);
    while x < x1 {
        let height = random.between(heights.0, heights.1);
        shapes.push(vec![fir(point(x, ground), height)]);
        x += spacing * random.between(0.7, 1.3);
    }
    union(shapes)
}

/// Mountains: a ridge through `peaks` (x, height above `bottom`) from
/// `x0` to `x1`, filled down to `bottom`.
pub fn mountains(x0: f64, x1: f64, bottom: f64, peaks: &[(f64, f64)]) -> Vec<Path> {
    let mut points = vec![(x0, bottom)];
    points.extend(peaks.iter().map(|(x, height)| (*x, bottom + height)));
    points.push((x1, bottom));
    vec![polygon(&points)]
}

/// Flower-shaped closed curve: `petals` round petals between `inner`
/// and `outer` radius.
pub fn rosette(centre: Point, inner: f64, outer: f64, petals: usize, phase: f64) -> Path {
    let steps = petals * 24;
    Path::new(
        (0..steps)
            .map(|step| {
                let t = step as f64 / steps as f64;
                let wave = ((t * petals as f64 + phase) * std::f64::consts::TAU)
                    .cos()
                    .abs()
                    .powf(0.7);
                polar(centre, inner + (outer - inner) * wave, t * 360.0)
            })
            .collect(),
        true,
    )
}

/// Teardrop window, round end at `centre`, its point `length` away
/// towards `degrees`.
pub fn teardrop(centre: Point, radius: f64, length: f64, degrees: f64) -> Path {
    let tip = polar(centre, length, degrees);
    let mut points = Vec::new();
    let steps = 24;
    for step in 0..=steps {
        let angle = degrees + 90.0 + 180.0 * step as f64 / steps as f64;
        points.push(polar(centre, radius, angle));
    }
    points.push(tip);
    Path::new(points, true)
}

/// The sheets of a layered decoration, back to front.
pub struct Stack {
    pub sheets: Vec<Part>,
}

/// Width of the frame every sheet keeps along the outline.
pub const FRAME: f64 = 8.0;

/// Builds the sheets: the back is the whole `outline`, then one sheet
/// per scene element (each held by the frame, loose bits dropped), then
/// the front frame alone. `names` gives each sheet its name, back to
/// front. Pin holes go through all frames; numbers are engraved where
/// the next sheet hides them.
pub fn stack(outline: Vec<Path>, scene: Vec<Vec<Path>>, names: Vec<String>) -> Stack {
    let inner = offset_paths(&outline, -FRAME);
    let frame = difference(vec![outline.clone(), inner.clone()]);
    // Scene elements reach under the frame to hold on to it.
    let reach = offset_paths(&outline, -FRAME / 2.0);
    let mut areas = vec![outline.clone()];
    for element in scene {
        let held = combine(&[element, reach.clone()], Boolean::Intersection).unwrap_or_default();
        areas.push(union(vec![frame.clone(), held]));
    }
    areas.push(frame.clone());
    assemble(&outline, areas, names)
}

/// Sheets from their areas, back to front: one piece each, pin holes
/// through the frame, a number where the next sheet hides it.
pub fn assemble(outline: &[Path], areas: Vec<Vec<Path>>, names: Vec<String>) -> Stack {
    assert_eq!(areas.len(), names.len(), "one name per sheet");
    let pins = pin_holes(outline);
    let label_at = label_point(outline);
    let count = areas.len();
    let sheets = areas
        .into_iter()
        .zip(names)
        .enumerate()
        .map(|(index, (area, name))| {
            let mut part = Part::from_area(name, connected(area));
            part.holes
                .extend(pins.iter().map(|pin| circle(*pin, crate::fit::real(1.0))));
            // The front sheet shows its face: no number on it.
            if index + 1 < count {
                part.engrave = stroke_text(&(index + 1).to_string(), 3.5, label_at);
            }
            part
        })
        .collect();
    Stack { sheets }
}

/// The piece of an area that holds together (the largest polygon and
/// its holes); anything not joined to it would fall out.
fn connected(area: Vec<Path>) -> Vec<Path> {
    use geo::{Area, Polygon};
    let region = region_of(&area);
    let Some(polygon) = region
        .0
        .iter()
        .max_by(|a, b| a.unsigned_area().total_cmp(&b.unsigned_area()))
    else {
        return area;
    };
    // Tiny gaps closed in by overlapping branches would only scorch.
    let holes = polygon
        .interiors()
        .iter()
        .filter(|ring| Polygon::new((*ring).clone(), Vec::new()).unsigned_area() > 4.0);
    std::iter::once(polygon.exterior())
        .chain(holes)
        .map(to_path)
        .filter(|path| path.points.len() > 2)
        .collect()
}

/// The middle line of the frame.
fn frame_middle(outline: &[Path]) -> Path {
    offset_paths(outline, -FRAME / 2.0)
        .into_iter()
        .max_by(|a, b| a.signed_area().abs().total_cmp(&b.signed_area().abs()))
        .expect("the outline is wider than its frame")
}

/// Two pin holes in the middle of the frame, high on the left and on
/// the right: dowels of 2 mm line the sheets up, a cord can hang it.
fn pin_holes(outline: &[Path]) -> Vec<Point> {
    let middle = frame_middle(outline);
    let bounds = middle.bounds();
    let centre = (bounds.min.x + bounds.max.x) / 2.0;
    let highest = |left: bool| {
        middle
            .points
            .iter()
            .filter(|point| (point.x < centre - bounds.width() * 0.15) == left)
            .filter(|point| (point.x > centre + bounds.width() * 0.15) != left)
            .copied()
            .max_by(|a, b| a.y.total_cmp(&b.y))
    };
    [highest(true), highest(false)]
        .into_iter()
        .flatten()
        .collect()
}

/// Where a sheet number goes: the lowest point of the frame middle.
fn label_point(outline: &[Path]) -> Point {
    frame_middle(outline)
        .points
        .iter()
        .copied()
        .min_by(|a, b| a.y.total_cmp(&b.y))
        .expect("a frame has points")
}

/// Cuts the sheets of a stack, a few per board, with the guide steps
/// around: preparing, the boards, then gluing and finishing.
pub(crate) fn cut_stack(mut b: Builder, stack: Stack) -> Document {
    b.stock_board();
    b.step("s1", Vec::new());
    b.cut_boards(
        boards(stack.sheets, 4.0),
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

/// Names of the sheets, back to front: "1 — back", "2", …, "N — front".
pub(crate) fn sheet_names(b: &Builder, count: usize) -> Vec<String> {
    (1..=count)
        .map(|number| {
            let key = match number {
                1 => "sheet_back",
                _ if number == count => "sheet_front",
                _ => "sheet",
            };
            b.common_format(key, &[&number.to_string()])
        })
        .collect()
}

/// Radius of the layered mandala.
const MANDALA: f64 = 60.0;

/// Layered mandala: six discs with a scalloped edge. Each opens onto
/// the ones behind through the same flower (eight petals, eight thin
/// petals between them, sixteen dots), grown by 2 mm from one sheet to
/// the next, so the flower goes down in steps.
pub fn layered_mandala(language: &str) -> Document {
    let b = Builder::new(text_language(language), "layered_mandala");
    let names = sheet_names(&b, 6);
    cut_stack(b, mandala_stack(names))
}

/// The flower all the openings of the mandala are cut from.
fn mandala_flower(centre: Point) -> Vec<Path> {
    let mut shapes = vec![vec![rosette(centre, 13.0, 36.0, 4, 0.0)]];
    for index in 0..8 {
        let angle = 22.5 + index as f64 * 45.0;
        shapes.push(vec![teardrop(polar(centre, 20.0, angle), 2.6, 24.0, angle)]);
    }
    for index in 0..16 {
        shapes.push(vec![circle(
            polar(centre, 46.0, index as f64 * 22.5 + 11.25),
            1.6,
        )]);
    }
    union(shapes)
}

pub(crate) fn mandala_stack(names: Vec<String>) -> Stack {
    let centre = point(0.0, 0.0);
    let outline = vec![rosette(centre, MANDALA - 2.5, MANDALA, 24, 0.0)];
    let flower = mandala_flower(centre);
    let mut areas = vec![outline.clone()];
    for grow in [-4.0, -2.0, 0.0, 2.0, 4.0] {
        let opening = offset_paths(&flower, grow);
        areas.push(difference(vec![outline.clone(), opening]));
    }
    assemble(&outline, areas, names)
}

/// Height of the moose.
const MOOSE_HEIGHT: f64 = 165.0;

/// Moose in the forest: the moose's outline frames a winter scene of
/// mountains, two rows of firs and two stags on a snowy slope.
pub fn moose_forest(language: &str) -> Document {
    let b = Builder::new(text_language(language), "moose_forest");
    let names = sheet_names(&b, 6);
    cut_stack(b, moose_stack(names))
}

pub(crate) fn moose_stack(names: Vec<String>) -> Stack {
    let outline = crate::art::silhouette(crate::art::MOOSE, MOOSE_HEIGHT, false);
    // The scene fills the body and neck, the part wider than the frame.
    let inner = offset_paths(&outline, -FRAME);
    let room = inner
        .iter()
        .max_by(|a, b| a.signed_area().abs().total_cmp(&b.signed_area().abs()))
        .expect("the moose is wider than its frame")
        .bounds();
    let (x0, x1) = (room.min.x - 5.0, room.max.x + 5.0);
    let (y0, height) = (room.min.y, room.height());
    let at = |fraction: f64| y0 + height * fraction;
    let mut random = Random::new(7);
    let span = x1 - x0;
    let peaks: Vec<(f64, f64)> = [
        (0.0, 0.55),
        (0.12, 0.78),
        (0.22, 0.62),
        (0.36, 0.92),
        (0.47, 0.70),
        (0.6, 0.86),
        (0.72, 0.58),
        (0.85, 0.80),
        (1.0, 0.60),
    ]
    .iter()
    .map(|(x, h)| (x0 + span * x, height * h))
    .collect();
    let far = forest(
        &mut random,
        x0,
        x1,
        at(0.32),
        y0 - 10.0,
        (height * 0.22, height * 0.38),
        7.0,
    );
    let near = forest(
        &mut random,
        x0,
        x1,
        at(0.14),
        y0 - 10.0,
        (height * 0.3, height * 0.55),
        10.0,
    );
    // A snowy slope rising to the right, two stags on it and a few firs
    // at the ends.
    let slope = polygon(&[
        (x0, y0 - 10.0),
        (x0, at(0.1)),
        (x0 + span * 0.5, at(0.17)),
        (x1, at(0.26)),
        (x1, y0 - 10.0),
    ]);
    let stag = |x: f64, mirror: bool| {
        let deer = crate::art::silhouette(crate::art::STAG, height * 0.38, mirror);
        let ground = at(0.1) + (at(0.26) - at(0.1)) * (x - x0) / span;
        crate::art::placed(&deer, x, ground - 0.8)
    };
    let mut front = vec![
        vec![slope],
        stag(x0 + span * 0.33, false),
        stag(x0 + span * 0.58, true),
    ];
    for (fraction, size) in [(0.04, 0.7), (0.1, 0.5), (0.9, 0.62), (0.97, 0.8)] {
        let x = x0 + span * fraction;
        let ground = at(0.1) + (at(0.26) - at(0.1)) * fraction;
        front.push(vec![fir(point(x, ground - 1.0), height * size)]);
    }
    let scene = vec![
        mountains(x0, x1, y0 - 10.0, &peaks),
        far,
        near,
        union(front),
    ];
    stack(outline, scene, names)
}

/// The largest area of `outline` once its frame is taken off: where
/// the scene shows.
pub(crate) fn scene_room(outline: &[Path]) -> mml_core::geometry::Rect {
    offset_paths(outline, -FRAME)
        .iter()
        .max_by(|a, b| a.signed_area().abs().total_cmp(&b.signed_area().abs()))
        .expect("the outline is wider than its frame")
        .bounds()
}

/// Ridge points from fractions of the room: (x, height).
fn ridge(x0: f64, span: f64, height: f64, points: &[(f64, f64)]) -> Vec<(f64, f64)> {
    points
        .iter()
        .map(|(x, h)| (x0 + span * x, height * h))
        .collect()
}

/// Bear at sunset: the sun is a hole in the sky sheet, mountains and two
/// rows of firs in front of it, a lake shore at the bottom.
pub fn bear_sunset(language: &str) -> Document {
    let b = Builder::new(text_language(language), "bear_sunset");
    let names = sheet_names(&b, 7);
    cut_stack(b, bear_stack(names))
}

pub(crate) fn bear_stack(names: Vec<String>) -> Stack {
    let outline = crate::art::picked(crate::art::FOREST_ANIMALS, crate::art::BEAR, 118.0, false);
    let room = scene_room(&outline);
    let (x0, x1) = (room.min.x - 5.0, room.max.x + 5.0);
    let (y0, height, span) = (room.min.y, room.height(), x1 - x0);
    let bottom = y0 - 10.0;
    let at = |fraction: f64| y0 + height * fraction;
    let mut random = Random::new(11);
    let sun = point(x0 + span * 0.62, at(0.62));
    let mut sky = vec![vec![rect(x0, bottom, span, height + 20.0)]];
    sky.push(vec![circle(sun, height * 0.2)]);
    // Rays as thin wedges around the sun.
    for index in 0..10 {
        let angle = index as f64 * 36.0 + 18.0;
        sky.push(vec![Path::new(
            vec![
                polar(sun, height * 0.26, angle - 3.0),
                polar(sun, height * 0.42, angle),
                polar(sun, height * 0.26, angle + 3.0),
            ],
            true,
        )]);
    }
    let sky = difference(sky);
    let peaks = ridge(
        x0,
        span,
        height,
        &[
            (0.0, 0.5),
            (0.15, 0.7),
            (0.3, 0.52),
            (0.45, 0.66),
            (0.58, 0.45),
            (0.75, 0.62),
            (0.9, 0.48),
            (1.0, 0.56),
        ],
    );
    let far = forest(
        &mut random,
        x0,
        x1,
        at(0.25),
        bottom,
        (height * 0.15, height * 0.3),
        6.5,
    );
    let near = forest(
        &mut random,
        x0,
        x1,
        at(0.1),
        bottom,
        (height * 0.25, height * 0.45),
        9.0,
    );
    // The shore: a gentle bank with reeds.
    let mut shore = vec![polygon(&[
        (x0, bottom),
        (x0, at(0.12)),
        (x0 + span * 0.3, at(0.06)),
        (x0 + span * 0.7, at(0.09)),
        (x1, at(0.04)),
        (x1, bottom),
    ])];
    for index in 0..9 {
        let x = x0 + span * (0.05 + index as f64 * 0.11);
        let lean = random.between(-0.15, 0.15);
        let tall = height * random.between(0.1, 0.18);
        shore.extend(offset_paths(
            &[Path::new(
                vec![point(x, at(0.02)), point(x + tall * lean, at(0.02) + tall)],
                false,
            )],
            0.7,
        ));
    }
    let scene = vec![
        sky,
        mountains(x0, x1, bottom, &peaks),
        far,
        union(vec![near]),
        union(shore.into_iter().map(|path| vec![path]).collect()),
    ];
    stack(outline, scene, names)
}

/// Owl in the moonlight: a crescent moon and stars cut in the night sky,
/// two rows of tall firs.
pub fn owl_moon(language: &str) -> Document {
    let b = Builder::new(text_language(language), "owl_moon");
    let names = sheet_names(&b, 5);
    cut_stack(b, owl_stack(names))
}

pub(crate) fn owl_stack(names: Vec<String>) -> Stack {
    let outline = crate::art::picked(crate::art::FOREST_ANIMALS, crate::art::OWL, 160.0, false);
    let room = scene_room(&outline);
    let (x0, x1) = (room.min.x - 5.0, room.max.x + 5.0);
    let (y0, height, span) = (room.min.y, room.height(), x1 - x0);
    let bottom = y0 - 10.0;
    let at = |fraction: f64| y0 + height * fraction;
    let mut random = Random::new(5);
    let moon = point(x0 + span * 0.58, at(0.74));
    let radius = span * 0.17;
    let mut sky = vec![vec![rect(x0, bottom, span, height + 20.0)]];
    sky.push(difference(vec![
        vec![circle(moon, radius)],
        vec![circle(
            moon + point(radius * 0.45, radius * 0.25),
            radius * 0.85,
        )],
    ]));
    for (x, y, size) in [
        (0.25, 0.88, 3.2),
        (0.35, 0.66, 2.4),
        (0.8, 0.9, 2.6),
        (0.22, 0.55, 2.0),
        (0.78, 0.6, 2.2),
    ] {
        sky.push(vec![star(point(x0 + span * x, at(y)), size, size * 0.42)]);
    }
    let sky = difference(sky);
    let far = forest(
        &mut random,
        x0,
        x1,
        at(0.18),
        bottom,
        (height * 0.3, height * 0.5),
        9.0,
    );
    let near = forest(
        &mut random,
        x0,
        x1,
        at(0.04),
        bottom,
        (height * 0.4, height * 0.62),
        14.0,
    );
    stack(outline, vec![sky, far, near], names)
}

/// Five-pointed star, a point upwards.
pub(crate) fn star(centre: Point, outer: f64, inner: f64) -> Path {
    Path::new(
        (0..10)
            .map(|index| {
                let radius = if index % 2 == 0 { outer } else { inner };
                polar(centre, radius, 90.0 + index as f64 * 36.0)
            })
            .collect(),
        true,
    )
}

/// Radius of the Christmas bauble.
const BAUBLE: f64 = 48.0;

/// Christmas bauble: a round frame with its hanging loop, stars in the
/// sky, snowy hills with firs, and a deer leaping over the front hill.
pub fn christmas_bauble(language: &str) -> Document {
    let b = Builder::new(text_language(language), "christmas_bauble");
    let names = sheet_names(&b, 5);
    cut_stack(b, bauble_stack(names))
}

pub(crate) fn bauble_stack(names: Vec<String>) -> Stack {
    let centre = point(0.0, 0.0);
    let loop_at = point(0.0, BAUBLE + 4.0);
    let outline = union(vec![
        vec![circle(centre, BAUBLE)],
        vec![circle(loop_at, 7.0)],
    ]);
    let (x0, x1, bottom) = (-BAUBLE, BAUBLE, -BAUBLE - 5.0);
    let mut random = Random::new(23);
    let mut sky = vec![vec![rect(x0, bottom, 2.0 * BAUBLE, 2.0 * BAUBLE + 5.0)]];
    for (x, y, size) in [
        (-22.0, 26.0, 3.5),
        (-6.0, 34.0, 2.4),
        (14.0, 28.0, 4.2),
        (28.0, 14.0, 2.6),
        (-30.0, 8.0, 2.2),
        (2.0, 16.0, 2.0),
    ] {
        sky.push(vec![star(point(x, y), size, size * 0.42)]);
    }
    let sky = difference(sky);
    let hills = vec![polygon(&[
        (x0, bottom),
        (x0, -2.0),
        (-25.0, 6.0),
        (-5.0, 0.0),
        (18.0, 8.0),
        (x1, -4.0),
        (x1, bottom),
    ])];
    let mut far = vec![hills];
    let mut x = x0 + 4.0;
    while x < x1 {
        let ground = -6.0 + 6.0 * (x / 30.0).cos();
        far.push(vec![fir(point(x, ground), random.between(14.0, 24.0))]);
        x += random.between(6.0, 10.0);
    }
    let mut front = vec![vec![polygon(&[
        (x0, bottom),
        (x0, -18.0),
        (-10.0, -24.0),
        (20.0, -20.0),
        (x1, -28.0),
        (x1, bottom),
    ])]];
    front.push(vec![fir(point(-34.0, -21.0), 26.0)]);
    front.push(vec![fir(point(-26.0, -22.5), 18.0)]);
    front.push(vec![fir(point(36.0, -25.5), 22.0)]);
    // The deer takes off from the hill: its lowest hoof sits on it.
    let deer = crate::art::silhouette(crate::art::LEAPING_DEER, 46.0, false);
    front.push(crate::art::placed(&deer, -16.0, -26.0));
    let mut stack = stack(outline, vec![sky, union(far), union(front)], names);
    for sheet in &mut stack.sheets {
        sheet.holes.push(circle(loop_at, 2.5));
    }
    stack
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fir_stands_on_its_base_and_reaches_its_height() {
        let tree = fir(point(10.0, 5.0), 40.0);
        let bounds = tree.bounds();
        assert!((bounds.min.y - 5.0).abs() < 2.5);
        assert!((bounds.max.y - 45.0).abs() < 1e-9);
        assert!((bounds.min.x + bounds.max.x - 20.0).abs() < 1e-9);
        // Both feet of the trunk stand on the base.
        let feet: Vec<f64> = tree
            .points
            .iter()
            .filter(|point| (point.y - 5.0).abs() < 1e-9)
            .map(|point| point.x - 10.0)
            .collect();
        assert_eq!(feet.len(), 2);
        assert!((feet[0] + feet[1]).abs() < 1e-9);
    }

    #[test]
    fn every_sheet_is_one_piece_with_the_pin_holes() {
        let outline = vec![circle(point(0.0, 0.0), 60.0)];
        let mut random = Random::new(3);
        let scene = vec![
            mountains(-70.0, 70.0, -70.0, &[(-30.0, 80.0), (20.0, 95.0)]),
            forest(&mut random, -70.0, 70.0, -30.0, -70.0, (15.0, 30.0), 12.0),
            // A loose bit in the middle of nowhere: dropped.
            vec![circle(point(0.0, 20.0), 5.0)],
        ];
        let names = (0..5).map(|index| index.to_string()).collect();
        let stack = stack(outline, scene, names);
        assert_eq!(stack.sheets.len(), 5);
        for sheet in &stack.sheets {
            assert_eq!(
                sheet
                    .holes
                    .iter()
                    .filter(|hole| (hole.bounds().width() - 2.0).abs() < 0.05)
                    .count(),
                2
            );
        }
        // The loose disc is gone: the sheet is the frame alone.
        let frame = &stack.sheets[4];
        let loose = &stack.sheets[3];
        assert_eq!(loose.holes.len(), frame.holes.len());
        assert!(stack.sheets[4].engrave.is_empty());
        assert!(!stack.sheets[0].engrave.is_empty());
    }
}
