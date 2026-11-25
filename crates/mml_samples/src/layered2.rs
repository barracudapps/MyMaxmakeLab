//! More layered decorations, each on its own theme: the ocean inside a
//! whale, the pack ice inside a penguin, a city at night, a porthole on
//! space and a Halloween pumpkin.

use crate::Builder;
use crate::layered::*;
use crate::shapes::*;
use crate::tests_projects::text_language;
use mml_core::Document;
use mml_core::geometry::{Path, Point};
use mml_core::region::offset_paths;

/// Closed ellipse.
pub fn ellipse(centre: Point, rx: f64, ry: f64) -> Path {
    let count = (((rx + ry) * 2.0).ceil() as usize).clamp(24, 240);
    Path::new(
        (0..count)
            .map(|index| {
                let angle = index as f64 / count as f64 * std::f64::consts::TAU;
                centre + point(rx * angle.cos(), ry * angle.sin())
            })
            .collect(),
        true,
    )
}

/// A little fish `length` long, nose at the right unless `left`.
pub fn fish(centre: Point, length: f64, left: bool) -> Path {
    let side = if left { -1.0 } else { 1.0 };
    let body = ellipse(
        centre + point(side * length * 0.12, 0.0),
        length * 0.36,
        length * 0.2,
    );
    let tail_root = centre + point(-side * length * 0.18, 0.0);
    let tail = Path::new(
        vec![
            tail_root,
            tail_root + point(-side * length * 0.32, length * 0.2),
            tail_root + point(-side * length * 0.26, 0.0),
            tail_root + point(-side * length * 0.32, -length * 0.2),
        ],
        true,
    );
    union(vec![vec![body], vec![tail]])
        .into_iter()
        .next()
        .expect("a fish")
}

/// A strand of seaweed from `base`, waving, narrowing to its tip.
pub fn seaweed(base: Point, height: f64, width: f64, phase: f64) -> Path {
    let steps = 24;
    let centre = |t: f64| {
        let y = base.y + height * t;
        let x = base.x + width * 1.2 * (t * 7.0 + phase).sin() * t;
        (x, y)
    };
    let mut left = Vec::new();
    let mut right = Vec::new();
    for step in 0..=steps {
        let t = step as f64 / steps as f64;
        let (x, y) = centre(t);
        let half = width / 2.0 * (1.0 - t * 0.85);
        left.push(point(x - half, y));
        right.push(point(x + half, y));
    }
    let mut points = left;
    points.extend(right.into_iter().rev());
    Path::new(points, true)
}

/// A branching shape from `base` towards `degrees`: a trunk and
/// `depth` levels of forks, `width` at the root.
pub fn branches(
    random: &mut Random,
    base: Point,
    length: f64,
    width: f64,
    degrees: f64,
    depth: usize,
) -> Vec<Vec<Path>> {
    let tip = polar(base, length, degrees);
    let mut shapes = vec![offset_paths(
        &[Path::new(vec![base, tip], false)],
        width / 2.0,
    )];
    if depth > 0 {
        for turn in [-1.0, 1.0] {
            let spread = random.between(18.0, 38.0);
            let next = length * random.between(0.6, 0.78);
            shapes.extend(branches(
                random,
                tip,
                next,
                (width * 0.68).max(1.2),
                degrees + turn * spread,
                depth - 1,
            ));
        }
    }
    shapes
}

/// Scene limits inside a frame: x0, x1, bottom of the elements, the
/// room's bottom and height.
struct Room {
    x0: f64,
    x1: f64,
    bottom: f64,
    y0: f64,
    height: f64,
}

impl Room {
    fn of(outline: &[Path]) -> Self {
        let room = scene_room(outline);
        Room {
            x0: room.min.x - 5.0,
            x1: room.max.x + 5.0,
            bottom: room.min.y - 10.0,
            y0: room.min.y,
            height: room.height(),
        }
    }

    fn span(&self) -> f64 {
        self.x1 - self.x0
    }

    fn x(&self, fraction: f64) -> f64 {
        self.x0 + self.span() * fraction
    }

    fn y(&self, fraction: f64) -> f64 {
        self.y0 + self.height * fraction
    }

    /// The whole room, to cut a sky or a sea out of.
    fn backdrop(&self) -> Vec<Path> {
        vec![rect(self.x0, self.bottom, self.span(), self.height + 30.0)]
    }
}

/// Builds a project from its stack: `count` sheets.
fn project(
    language: &str,
    prefix: &'static str,
    count: usize,
    build: fn(Vec<String>) -> Stack,
) -> Document {
    let b = Builder::new(text_language(language), prefix);
    let names = sheet_names(&b, count);
    cut_stack(b, build(names))
}

/// Under the sea, inside a whale: a school of fish cut in the water,
/// rocks and seaweed, tall seaweed and bubbles in front.
pub fn ocean_whale(language: &str) -> Document {
    project(language, "ocean_whale", 5, ocean_stack)
}

pub(crate) fn ocean_stack(names: Vec<String>) -> Stack {
    let outline = crate::art::picked(crate::art::WHALE_SHAPES, crate::art::WHALE, 125.0, false);
    let room = Room::of(&outline);
    let mut random = Random::new(31);
    let mut water = vec![room.backdrop()];
    for (x, y, size, left) in [
        (0.42, 0.72, 13.0, false),
        (0.55, 0.78, 11.0, false),
        (0.5, 0.62, 12.0, false),
        (0.64, 0.68, 10.0, false),
        (0.6, 0.55, 9.0, false),
        (0.3, 0.5, 15.0, true),
    ] {
        water.push(vec![fish(point(room.x(x), room.y(y)), size, left)]);
    }
    for (x, y, r) in [(0.78, 0.6, 1.6), (0.8, 0.7, 1.2), (0.77, 0.78, 0.9)] {
        water.push(vec![circle(point(room.x(x), room.y(y)), r * 1.4)]);
    }
    let water = difference(water);
    // Rocks along the bottom and short seaweed.
    let mut rocks = vec![vec![polygon(&[
        (room.x0, room.bottom),
        (room.x0, room.y(0.28)),
        (room.x(0.12), room.y(0.36)),
        (room.x(0.25), room.y(0.24)),
        (room.x(0.4), room.y(0.3)),
        (room.x(0.55), room.y(0.2)),
        (room.x(0.72), room.y(0.32)),
        (room.x(0.86), room.y(0.22)),
        (room.x1, room.y(0.3)),
        (room.x1, room.bottom),
    ])]];
    let mut x = room.x(0.05);
    while x < room.x1 {
        let tall = room.height * random.between(0.25, 0.4);
        rocks.push(vec![seaweed(
            point(x, room.y(0.2)),
            tall,
            3.0,
            random.between(0.0, 6.0),
        )]);
        x += random.between(7.0, 12.0);
    }
    // In front: sand, tall seaweed and a coral.
    let mut front = vec![vec![polygon(&[
        (room.x0, room.bottom),
        (room.x0, room.y(0.12)),
        (room.x(0.5), room.y(0.06)),
        (room.x1, room.y(0.14)),
        (room.x1, room.bottom),
    ])]];
    for (fraction, tall) in [(0.08, 0.65), (0.15, 0.5), (0.88, 0.6), (0.95, 0.45)] {
        front.push(vec![seaweed(
            point(room.x(fraction), room.y(0.06)),
            room.height * tall,
            4.0,
            random.between(0.0, 6.0),
        )]);
    }
    front.extend(branches(
        &mut random,
        point(room.x(0.62), room.y(0.07)),
        room.height * 0.16,
        3.4,
        90.0,
        3,
    ));
    stack(outline, vec![water, union(rocks), union(front)], names)
}

/// The pack ice inside a penguin: northern lights in the sky, icebergs,
/// two penguins on a floe.
pub fn penguin_ice(language: &str) -> Document {
    project(language, "penguin_ice", 5, ice_stack)
}

/// A waving band between two curves, `x0` to `x1`, ends tapered.
fn aurora(x0: f64, x1: f64, level: f64, thickness: f64, phase: f64) -> Path {
    let steps = 40;
    let mut top = Vec::new();
    let mut bottom = Vec::new();
    for step in 0..=steps {
        let t = step as f64 / steps as f64;
        let x = x0 + (x1 - x0) * t;
        let y = level + 4.0 * (t * 9.0 + phase).sin();
        let half = thickness / 2.0 * (t * std::f64::consts::PI).sin();
        top.push(point(x, y + half));
        bottom.push(point(x, y - half));
    }
    let mut points = top;
    points.extend(bottom.into_iter().rev());
    Path::new(points, true)
}

pub(crate) fn ice_stack(names: Vec<String>) -> Stack {
    let outline = crate::art::picked(crate::art::ANIMALS_A, crate::art::PENGUIN, 160.0, false);
    let room = Room::of(&outline);
    let mut sky = vec![room.backdrop()];
    sky.push(vec![aurora(
        room.x(0.05),
        room.x(0.9),
        room.y(0.78),
        6.0,
        0.0,
    )]);
    sky.push(vec![aurora(
        room.x(0.15),
        room.x(0.98),
        room.y(0.66),
        4.0,
        2.0,
    )]);
    for (x, y, size) in [
        (0.25, 0.9, 2.6),
        (0.7, 0.92, 2.2),
        (0.85, 0.56, 2.0),
        (0.12, 0.6, 1.8),
    ] {
        sky.push(vec![star(point(room.x(x), room.y(y)), size, size * 0.42)]);
    }
    let sky = difference(sky);
    let icebergs = mountains(
        room.x0,
        room.x1,
        room.bottom,
        &[
            (room.x(0.0), room.height * 0.3),
            (room.x(0.12), room.height * 0.42),
            (room.x(0.2), room.height * 0.38),
            (room.x(0.3), room.height * 0.52),
            (room.x(0.38), room.height * 0.33),
            (room.x(0.62), room.height * 0.36),
            (room.x(0.74), room.height * 0.5),
            (room.x(0.8), room.height * 0.46),
            (room.x(0.9), room.height * 0.3),
            (room.x(1.0), room.height * 0.28),
        ],
    );
    // The floe, flat on top with a lip of snow, and two penguins.
    let floe_top = room.y(0.16);
    let mut floe = vec![vec![polygon(&[
        (room.x0, room.bottom),
        (room.x0, floe_top - 2.0),
        (room.x(0.1), floe_top),
        (room.x(0.85), floe_top + 1.0),
        (room.x1, floe_top - 3.0),
        (room.x1, room.bottom),
    ])]];
    for (fraction, height, mirror) in [(0.3, 0.2, false), (0.52, 0.15, true)] {
        let penguin = crate::art::picked(
            crate::art::ANIMALS_A,
            crate::art::PENGUIN,
            room.height * height,
            mirror,
        );
        floe.push(translate(&penguin, room.x(fraction), floe_top - 1.0));
    }
    stack(outline, vec![sky, icebergs, union(floe)], names)
}

/// Height of the arched window of the city.
const CITY: (f64, f64) = (130.0, 165.0);

/// A city at night in an arched window: the moon and stars, two rows of
/// buildings with lit windows, trees and street lamps in front.
pub fn city_night(language: &str) -> Document {
    project(language, "city_night", 6, city_stack)
}

/// A building from `x`, `width` wide, `height` tall over `ground`, with
/// its roof; window holes if `windows`.
fn building(
    random: &mut Random,
    x: f64,
    width: f64,
    height: f64,
    ground: f64,
    bottom: f64,
    windows: bool,
) -> (Vec<Path>, Vec<Path>) {
    let top = ground + height;
    let mut shapes = vec![vec![rect(x, bottom, width, top - bottom)]];
    match (random.next() * 5.0) as usize {
        0 => shapes.push(vec![polygon(&[
            (x, top - 0.5),
            (x + width / 2.0, top + width * 0.45),
            (x + width, top - 0.5),
        ])]),
        1 => shapes.push(vec![circle(point(x + width / 2.0, top), width * 0.38)]),
        2 => {
            shapes.push(vec![rect(
                x + width * 0.2,
                top - 0.5,
                width * 0.6,
                width * 0.35,
            )]);
            shapes.push(vec![polygon(&[
                (x + width * 0.44, top + width * 0.34),
                (x + width * 0.5, top + width * 1.2),
                (x + width * 0.56, top + width * 0.34),
            ])]);
        }
        3 => shapes.push(vec![rect(
            x + width * 0.15,
            top - 0.5,
            width * 0.7,
            height * 0.12,
        )]),
        _ => {}
    }
    let mut holes = Vec::new();
    if windows && width >= 9.0 {
        let columns = ((width - 3.0) / 4.5).floor() as usize;
        let margin = (width - columns as f64 * 4.5 + 2.5) / 2.0;
        let mut y = ground + 4.0;
        while y + 3.0 < top - 3.0 {
            for column in 0..columns {
                if random.next() < 0.55 {
                    holes.push(rect(x + margin + column as f64 * 4.5, y, 2.0, 3.0));
                }
            }
            y += 6.0;
        }
    }
    (union(shapes), holes)
}

/// A row of buildings across the room.
fn skyline(
    random: &mut Random,
    x0: f64,
    x1: f64,
    ground: f64,
    bottom: f64,
    heights: (f64, f64),
    windows: bool,
) -> Vec<Path> {
    let mut shapes = vec![vec![rect(x0, bottom, x1 - x0, ground - bottom + 0.5)]];
    let mut holes = Vec::new();
    let mut x = x0;
    while x < x1 {
        let width = random.between(9.0, 18.0);
        let height = random.between(heights.0, heights.1);
        let (building, windows) = building(random, x, width, height, ground, bottom, windows);
        shapes.push(building);
        holes.extend(windows);
        x += width + random.between(-1.0, 2.0);
    }
    let mut cut = vec![union(shapes)];
    cut.extend(holes.into_iter().map(|hole| vec![hole]));
    difference(cut)
}

/// A street lamp standing on `ground`.
fn lamp(x: f64, ground: f64, height: f64) -> Vec<Vec<Path>> {
    let top = ground + height;
    vec![
        vec![rect(x - 0.8, ground - 1.0, 1.6, height)],
        vec![rect(x - 2.0, ground - 1.0, 4.0, 2.5)],
        offset_paths(
            &[Path::new(
                vec![
                    point(x, top),
                    point(x + 2.5, top + 2.0),
                    point(x + 5.0, top),
                ],
                false,
            )],
            0.8,
        ),
        vec![polygon(&[
            (x + 3.0, top),
            (x + 7.0, top),
            (x + 6.0, top - 2.5),
            (x + 4.0, top - 2.5),
        ])],
    ]
}

pub(crate) fn city_stack(names: Vec<String>) -> Stack {
    let (width, height) = CITY;
    let radius = width / 2.0;
    let outline = union(vec![
        vec![rect(0.0, 0.0, width, height - radius)],
        vec![circle(point(radius, height - radius), radius)],
    ]);
    let room = Room::of(&outline);
    let mut random = Random::new(41);
    let mut sky = vec![room.backdrop()];
    sky.push(difference(vec![
        vec![circle(point(room.x(0.72), room.y(0.8)), 9.0)],
        vec![circle(point(room.x(0.76), room.y(0.82)), 8.0)],
    ]));
    for (x, y, size) in [
        (0.2, 0.86, 2.4),
        (0.35, 0.94, 1.8),
        (0.5, 0.8, 2.0),
        (0.88, 0.62, 1.8),
        (0.12, 0.66, 1.6),
    ] {
        sky.push(vec![star(point(room.x(x), room.y(y)), size, size * 0.42)]);
    }
    let sky = difference(sky);
    let far = skyline(
        &mut random,
        room.x0,
        room.x1,
        room.y(0.2),
        room.bottom,
        (room.height * 0.2, room.height * 0.45),
        false,
    );
    let near = skyline(
        &mut random,
        room.x0,
        room.x1,
        room.y(0.08),
        room.bottom,
        (room.height * 0.15, room.height * 0.38),
        true,
    );
    let mut street = vec![vec![rect(
        room.x0,
        room.bottom,
        room.span(),
        room.y(0.06) - room.bottom,
    )]];
    for fraction in [0.1, 0.9] {
        street.extend(lamp(room.x(fraction), room.y(0.06), room.height * 0.22));
    }
    for fraction in [0.3, 0.5, 0.7] {
        let base = point(room.x(fraction), room.y(0.05));
        street.push(vec![rect(base.x - 0.9, base.y, 1.8, 6.0)]);
        street.push(vec![circle(base + point(0.0, 10.0), 5.5)]);
    }
    stack(outline, vec![sky, far, near, union(street)], names)
}

/// Radius of the space porthole.
const PORTHOLE: f64 = 70.0;

/// A porthole on space: stars, a ringed planet, a rocket leaving its
/// trail, and the bolts of the porthole engraved on the front.
pub fn space_porthole(language: &str) -> Document {
    project(language, "space_porthole", 5, space_stack)
}

/// Rocket pointing up, `length` long, its nose at `top`: body, two fins
/// and a round window.
pub fn rocket(top: Point, length: f64) -> (Path, Path) {
    let width = length * 0.26;
    let half = width / 2.0;
    let x = top.x;
    let mut points = vec![(x, top.y)];
    // Ogive nose down to the body.
    for step in 1..=8 {
        let t = step as f64 / 8.0;
        points.push((
            x + half * (1.0 - (1.0 - t) * (1.0 - t)),
            top.y - length * 0.35 * t,
        ));
    }
    let base = top.y - length;
    points.extend([
        (x + half, base + length * 0.28),
        (x + half * 2.1, base + length * 0.05),
        (x + half * 2.1, base - length * 0.06),
        (x + half * 0.9, base + length * 0.08),
        (x + half * 0.7, base),
        (x - half * 0.7, base),
        (x - half * 0.9, base + length * 0.08),
        (x - half * 2.1, base - length * 0.06),
        (x - half * 2.1, base + length * 0.05),
        (x - half, base + length * 0.28),
    ]);
    for step in (1..8).rev() {
        let t = step as f64 / 8.0;
        points.push((
            x - half * (1.0 - (1.0 - t) * (1.0 - t)),
            top.y - length * 0.35 * t,
        ));
    }
    (
        polygon(&points),
        circle(point(x, top.y - length * 0.45), width * 0.22),
    )
}

pub(crate) fn space_stack(names: Vec<String>) -> Stack {
    let centre = point(0.0, 0.0);
    let outline = vec![circle(centre, PORTHOLE)];
    let room = Room::of(&outline);
    let mut random = Random::new(53);
    let mut sky = vec![room.backdrop()];
    for _ in 0..22 {
        let at = point(room.x(random.next()), room.y(random.next()));
        let size = random.between(1.2, 2.6);
        if (at - centre).length() < PORTHOLE - FRAME - 4.0 {
            sky.push(vec![star(at, size, size * 0.42)]);
        }
    }
    let sky = difference(sky);
    // A big ringed planet coming in from the lower left, a small moon
    // joined to it by nothing: it is cut in the sky instead.
    let planet_at = point(-40.0, -40.0);
    let planet = vec![
        vec![circle(planet_at, 30.0)],
        difference(vec![
            vec![ellipse(planet_at, 50.0, 9.0)],
            vec![ellipse(planet_at, 41.0, 5.5)],
        ]),
    ];
    let planet = transform(
        &union(planet),
        &mml_core::geometry::Transform::translate(-planet_at.x, -planet_at.y)
            .then(mml_core::geometry::Transform::rotate_degrees(18.0))
            .then(mml_core::geometry::Transform::translate(
                planet_at.x,
                planet_at.y,
            )),
    );
    // The rocket climbs to the upper right on its trail.
    let (body, window) = rocket(point(0.0, 0.0), 44.0);
    let turn = mml_core::geometry::Transform::rotate_degrees(-35.0)
        .then(mml_core::geometry::Transform::translate(28.0, 30.0));
    let body = body.transformed(&turn);
    let window = window.transformed(&turn);
    let tail = turn.apply(point(0.0, -46.0));
    // A smooth curve from the frame, low on the right, to the rocket.
    let start = point(30.0, -70.0);
    let bend = point(10.0, -20.0);
    let trail = Path::new(
        (0..=24)
            .map(|step| {
                let t = step as f64 / 24.0;
                let a = start + (bend - start) * t;
                let b = bend + (tail - bend) * t;
                a + (b - a) * t
            })
            .collect(),
        false,
    );
    let mut flight = vec![vec![body]];
    flight.push(offset_paths(&[trail], 2.6));
    let flight = difference(vec![union(flight), vec![window]]);
    let mut stack = stack(outline, vec![sky, planet, flight], names);
    // Bolts engraved round the front frame.
    if let Some(front) = stack.sheets.last_mut() {
        let pins: Vec<Point> = front
            .holes
            .iter()
            .map(|hole| hole.bounds().center())
            .collect();
        for index in 0..12 {
            let at = polar(centre, PORTHOLE - FRAME / 2.0, index as f64 * 30.0 + 15.0);
            if pins.iter().any(|pin| (*pin - at).length() < 6.0) {
                continue;
            }
            front.engrave.push(circle(at, 1.6));
            front.engrave.push(circle(at, 0.6));
        }
    }
    stack
}

/// Halloween pumpkin: the moon and bats in the sky, dead trees on the
/// hills, a haunted house and graves in front.
pub fn halloween_pumpkin(language: &str) -> Document {
    project(language, "halloween_pumpkin", 5, pumpkin_stack)
}

fn pumpkin_outline() -> Vec<Path> {
    union(vec![
        vec![ellipse(point(0.0, 0.0), 40.0, 54.0)],
        vec![ellipse(point(-28.0, -3.0), 38.0, 50.0)],
        vec![ellipse(point(28.0, -3.0), 38.0, 50.0)],
        vec![ellipse(point(-50.0, -6.0), 30.0, 43.0)],
        vec![ellipse(point(50.0, -6.0), 30.0, 43.0)],
        vec![polygon(&[
            (-6.0, 48.0),
            (6.0, 48.0),
            (10.0, 66.0),
            (2.0, 68.0),
        ])],
    ])
}

/// A haunted house standing on `ground`: a gabled house, a tower with a
/// pointed roof, lit windows and a door cut out.
fn haunted_house(x: f64, ground: f64, bottom: f64) -> (Vec<Path>, Vec<Path>) {
    let shapes = vec![
        vec![rect(x, bottom, 34.0, ground + 22.0 - bottom)],
        vec![polygon(&[
            (x - 3.0, ground + 21.0),
            (x + 17.0, ground + 36.0),
            (x + 37.0, ground + 21.0),
        ])],
        vec![rect(x + 26.0, bottom, 12.0, ground + 34.0 - bottom)],
        vec![polygon(&[
            (x + 24.0, ground + 33.0),
            (x + 32.0, ground + 52.0),
            (x + 40.0, ground + 33.0),
        ])],
        vec![rect(x + 6.0, ground + 28.0, 3.0, 8.0)],
    ];
    let arch = |cx: f64, y: f64, w: f64, h: f64| {
        union(vec![
            vec![rect(cx - w / 2.0, y, w, h - w / 2.0)],
            vec![circle(point(cx, y + h - w / 2.0), w / 2.0)],
        ])
    };
    let mut holes = Vec::new();
    holes.extend(arch(x + 9.0, ground + 9.0, 4.0, 7.0));
    holes.extend(arch(x + 19.0, ground + 9.0, 4.0, 7.0));
    holes.extend(arch(x + 32.0, ground + 24.0, 3.5, 6.0));
    holes.extend(arch(x + 32.0, ground + 12.0, 3.5, 6.0));
    holes.extend(arch(x + 14.0, ground - 0.5, 5.0, 8.0));
    (union(shapes), holes)
}

pub(crate) fn pumpkin_stack(names: Vec<String>) -> Stack {
    let outline = pumpkin_outline();
    let room = Room::of(&outline);
    let mut random = Random::new(13);
    let mut sky = vec![room.backdrop()];
    sky.push(vec![circle(
        point(room.x(0.68), room.y(0.72)),
        room.height * 0.17,
    )]);
    let bat = crate::art::silhouette(crate::art::BAT, 1.0, false);
    let bat_box = mml_core::geometry::bounds_of(&bat);
    for (x, y, size) in [
        (0.25, 0.8, 16.0),
        (0.4, 0.64, 11.0),
        (0.86, 0.55, 12.0),
        (0.15, 0.6, 9.0),
    ] {
        let scale = size / bat_box.width();
        let placed = transform(
            &bat,
            &mml_core::geometry::Transform::scale(scale, scale).then(
                mml_core::geometry::Transform::translate(room.x(x) - size / 2.0, room.y(y)),
            ),
        );
        sky.push(placed);
    }
    let sky = difference(sky);
    let mut hills = vec![mountains(
        room.x0,
        room.x1,
        room.bottom,
        &[
            (room.x(0.0), room.height * 0.3),
            (room.x(0.25), room.height * 0.4),
            (room.x(0.5), room.height * 0.3),
            (room.x(0.8), room.height * 0.38),
            (room.x(1.0), room.height * 0.28),
        ],
    )];
    for (fraction, base, size) in [(0.12, 0.33, 0.28), (0.85, 0.34, 0.24)] {
        let lean = 90.0 + random.between(-8.0, 8.0);
        hills.extend(branches(
            &mut random,
            point(room.x(fraction), room.y(base)),
            room.height * size * 0.45,
            3.0,
            lean,
            3,
        ));
    }
    let ground = room.y(0.12);
    let mut front = vec![vec![polygon(&[
        (room.x0, room.bottom),
        (room.x0, ground - 4.0),
        (room.x(0.4), ground + 2.0),
        (room.x1, ground - 2.0),
        (room.x1, room.bottom),
    ])]];
    let (house, windows) = haunted_house(room.x(0.36), ground, room.bottom);
    front.push(house);
    // Graves on the left.
    for (fraction, tall) in [(0.12, 9.0), (0.2, 7.0)] {
        let x = room.x(fraction);
        front.push(union(vec![
            vec![rect(x - 3.0, ground - 3.0, 6.0, tall - 3.0)],
            vec![circle(point(x, ground + tall - 6.0), 3.0)],
        ]));
    }
    let mut front = vec![union(front)];
    front.extend(windows.into_iter().map(|hole| vec![hole]));
    stack(outline, vec![sky, union(hills), difference(front)], names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_scene_keeps_its_sheets_in_one_piece() {
        let names = |count: usize| (1..=count).map(|n| n.to_string()).collect::<Vec<_>>();
        for (count, stack) in [
            (5, ocean_stack(names(5))),
            (5, ice_stack(names(5))),
            (6, city_stack(names(6))),
            (5, space_stack(names(5))),
            (5, pumpkin_stack(names(5))),
        ] {
            assert_eq!(stack.sheets.len(), count);
            let back = stack.sheets[0].outline.bounds();
            for sheet in &stack.sheets {
                // Every sheet has the same outline as the back one.
                let bounds = sheet.outline.bounds();
                assert!((bounds.width() - back.width()).abs() < 0.5);
                assert!((bounds.height() - back.height()).abs() < 0.5);
            }
            assert!(back.width() <= 250.0 && back.height() <= 170.0, "{back:?}");
        }
    }
}
