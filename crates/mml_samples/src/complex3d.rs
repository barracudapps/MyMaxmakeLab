//! Larger 3D projects in plywood, over several boards: a book nook (a
//! lit alley to slide between books), a pair of bookends and a code box
//! whose lid opens only when its four letter dials show the code.

use crate::Builder;
use crate::fit::{real, slot};
use crate::layered::{Random, forest};
use crate::models3d::cut_then_steps;
use crate::shapes::*;
use crate::tests_projects::text_language;
use mml_core::Document;
use mml_core::geometry::{Path, Point, Transform};
use mml_core::makers::boxes::{BoxParams, panels};
use mml_core::region::{clip_lines, region_of};

/// A part from an area: the largest outline, the other paths as holes.
fn part(name: String, area: Vec<Path>) -> Part {
    Part::from_area(name, area)
}

/// `area` without the small gaps closed in where shapes overlap (firs
/// side by side): they would only be cut out as specks.
fn without_specks(area: Vec<Path>, smallest: f64) -> Vec<Path> {
    let largest = area
        .iter()
        .map(|path| path.signed_area().abs())
        .fold(0.0, f64::max);
    area.into_iter()
        .filter(|path| {
            let size = path.signed_area().abs();
            size >= largest || size > smallest
        })
        .collect()
}

/// `paths` mirrored left to right inside `0..width`.
fn mirrored(paths: &[Path], width: f64) -> Vec<Path> {
    transform(
        paths,
        &Transform::scale(-1.0, 1.0).then(Transform::translate(width, 0.0)),
    )
}

/// Window with a round top: `width` wide, `height` tall from `bottom`.
fn arched(x: f64, bottom: f64, width: f64, height: f64) -> Vec<Path> {
    union(vec![
        vec![rect(x - width / 2.0, bottom, width, height - width / 2.0)],
        vec![circle(point(x, bottom + height - width / 2.0), width / 2.0)],
    ])
}

/// Inside of the book nook: width, height, depth.
pub const NOOK: (f64, f64, f64) = (80.0, 145.0, 110.0);
/// Height and depth of the bridge across the alley.
const BRIDGE_AT: (f64, f64) = (96.0, 55.0);

/// The book nook: a box open at the front (the box generator's bottom
/// becomes the back, its walls the floor, the ceiling and the sides),
/// two layers of house fronts on each side wall, a bridge across with
/// tabs through the walls, and a town and the moon on the back.
pub struct Nook {
    pub parts: Vec<Part>,
    /// Holes the bridge tabs go through, in side wall coordinates.
    #[allow(dead_code)]
    pub bridge_holes: Vec<Path>,
}

/// House fronts of the right side: outline (u = depth from the back,
/// v = height), windows and door.
fn facade() -> (Vec<Path>, Vec<Path>) {
    let (_, height, depth) = NOOK;
    let v = |fraction: f64| height * fraction;
    let houses = union(vec![
        vec![polygon(&[
            (0.0, 0.5),
            (36.0, 0.5),
            (36.0, v(0.8)),
            (18.0, v(0.9)),
            (0.0, v(0.8)),
        ])],
        vec![rect(36.0, 0.5, 36.0, v(0.94))],
        vec![rect(60.0, v(0.9), 6.0, v(0.08))],
        vec![polygon(&[
            (72.0, 0.5),
            (depth, 0.5),
            (depth, v(0.74)),
            (depth - 8.0, v(0.74)),
            (depth - 8.0, v(0.8)),
            (depth - 19.0, v(0.86)),
            (78.0, v(0.8)),
            (78.0, v(0.74)),
            (72.0, v(0.74)),
        ])],
    ]);
    let mut openings = Vec::new();
    for (centre, rows) in [
        (18.0, [50.0, 85.0]),
        (54.0, [50.0, 118.0]),
        (91.0, [50.0, 85.0]),
    ] {
        for row in rows {
            openings.extend(arched(centre, row, 11.0, 15.0));
        }
    }
    // Doors on the floor, a shop window on the middle house.
    openings.extend(arched(18.0, 0.5, 13.0, 26.0));
    openings.extend(arched(91.0, 0.5, 13.0, 26.0));
    openings.push(rect(44.0, 12.0, 20.0, 16.0));
    (houses, openings)
}

/// Engraving of the back layer of a facade: bricks, and window panes
/// and planks seen through the openings of the front layer.
fn facade_engraving(houses: &[Path], openings: &[Path]) -> Vec<Path> {
    let (_, height, depth) = NOOK;
    let mut lines = Vec::new();
    let mut row = 0;
    let mut y = 4.0;
    while y < height {
        lines.push(Path::new(vec![point(0.0, y), point(depth, y)], false));
        let shift = if row % 2 == 0 { 0.0 } else { 5.0 };
        let mut x = shift;
        while x < depth {
            lines.push(Path::new(vec![point(x, y), point(x, y + 4.0)], false));
            x += 10.0;
        }
        y += 4.0;
        row += 1;
    }
    let walls = difference(vec![houses.to_vec(), openings.to_vec()]);
    let mut engraving = clip_lines(&lines, &region_of(&walls));
    // A cross in every opening: window bars or door planks.
    for opening in openings {
        let bounds = opening.bounds();
        let middle = bounds.center();
        engraving.push(Path::new(
            vec![point(middle.x, bounds.min.y), point(middle.x, bounds.max.y)],
            false,
        ));
        engraving.push(Path::new(
            vec![point(bounds.min.x, middle.y), point(bounds.max.x, middle.y)],
            false,
        ));
    }
    engraving
}

pub fn nook_parts(names: &dyn Fn(&str) -> String) -> Nook {
    let (width, height, depth) = NOOK;
    let t = slot();
    let params = BoxParams {
        inner: [width, height, depth],
        thickness: t,
        finger: 14.0,
        lid: false,
        kerf: 0.0,
        spacing: 4.0,
    };
    let box_panels = panels(&params);
    let panel = |name: &str| {
        box_panels
            .iter()
            .find(|panel| panel.name == name)
            .expect("a box panel")
            .outline
            .clone()
    };
    // The bridge: tabs through both layers of house fronts and the wall.
    let (bridge_v, bridge_u) = BRIDGE_AT;
    let tab = (bridge_v + 2.0, 8.0);
    let clearance = real(0.15);
    let bridge_hole = |offset: f64| {
        rect(
            tab.0 - clearance,
            offset + bridge_u - clearance,
            tab.1 + 2.0 * clearance,
            t + 2.0 * clearance,
        )
    };
    // Side walls: x is the height above the floor, y the depth (the
    // first `t` is the joint with the back).
    let wall_hole = bridge_hole(t);
    let mut parts = Vec::new();
    for (key, name) in [("left", "part.wall_left"), ("right", "part.wall_right")] {
        parts.push(Part::new(names(name), panel(key)).with_holes(vec![wall_hole.clone()]));
    }
    parts.push(Part::new(names("part.back"), panel("bottom")));
    parts.push(
        Part::new(names("part.floor"), panel("front")).with_engraving(cobbles(width, depth, t)),
    );
    parts.push(
        Part::new(names("part.ceiling"), panel("back")).with_engraving(lights(width, depth, t)),
    );
    // House fronts, in (u = depth, v = height), turned so v runs along
    // x like the wall; left ones mirrored.
    let (houses, openings) = facade();
    let engraving = facade_engraving(&houses, &openings);
    // The facade's own frame: u along y, v along x, as on the wall.
    let to_wall = Transform {
        a: 0.0,
        b: 1.0,
        c: 1.0,
        d: 0.0,
        e: 0.0,
        f: 0.0,
    };
    let back_layer = |mirror: bool| {
        let flip = |paths: &[Path]| {
            if mirror {
                mirrored(paths, depth)
            } else {
                paths.to_vec()
            }
        };
        let outline = transform(&flip(&houses), &to_wall);
        let engraving = transform(&flip(&engraving), &to_wall);
        (outline, engraving)
    };
    for (mirror, back_name, front_name) in [
        (false, "part.houses_right_back", "part.houses_right_front"),
        (true, "part.houses_left_back", "part.houses_left_front"),
    ] {
        let flip = |paths: &[Path]| {
            if mirror {
                mirrored(paths, depth)
            } else {
                paths.to_vec()
            }
        };
        let (outline, lines) = back_layer(mirror);
        // The bridge hole, where the wall has it.
        let facade_hole = vec![rect(
            tab.0 - clearance,
            bridge_u - clearance,
            tab.1 + 2.0 * clearance,
            t + 2.0 * clearance,
        )];
        let facade_hole = if mirror {
            // Mirrored in depth, the hole goes to the other end; the part
            // is turned over when glued, which brings it back.
            transform(
                &facade_hole,
                &Transform::scale(1.0, -1.0).then(Transform::translate(0.0, depth)),
            )
        } else {
            facade_hole
        };
        let mut back = vec![outline.clone()];
        back.push(facade_hole.clone());
        parts.push(part(names(back_name), difference(back)).with_engraving(lines));
        let mut front = vec![outline];
        front.push(transform(&flip(&openings), &to_wall));
        front.push(facade_hole);
        parts.push(part(names(front_name), difference(front)));
    }
    // The bridge seen from the front: an arch with a railing, tabs at
    // both ends through the house fronts and the walls.
    let span = width - 4.0 * t;
    let mut bridge = vec![
        vec![rect(0.0, bridge_v, span, 13.0)],
        vec![rect(-3.0 * t, tab.0, span + 6.0 * t, tab.1)],
    ];
    for index in 0..7 {
        let x = 4.0 + index as f64 * (span - 8.0) / 6.0;
        bridge.push(vec![rect(x - 1.0, bridge_v + 12.0, 2.0, 7.0)]);
    }
    bridge.push(vec![rect(0.0, bridge_v + 18.0, span, 2.0)]);
    let bridge = difference(vec![
        union(bridge),
        vec![crate::layered2::ellipse(
            point(span / 2.0, bridge_v),
            span * 0.38,
            7.0,
        )],
    ]);
    parts.push(part(names("part.bridge"), bridge));
    // The far end of the alley on the back: night sky with the moon and
    // stars, then a little town with its gate.
    let inner = rect(2.0 * t + 0.3, 0.3, width - 4.0 * t - 0.6, height - 0.6);
    let mut sky = vec![vec![inner.clone()]];
    sky.push(vec![circle(point(width * 0.62, height * 0.78), 9.0)]);
    for (x, y, size) in [
        (0.3, 0.88, 2.4),
        (0.45, 0.7, 1.8),
        (0.75, 0.62, 2.0),
        (0.35, 0.6, 1.6),
    ] {
        sky.push(vec![crate::layered::star(
            point(width * x, height * y),
            size,
            size * 0.42,
        )]);
    }
    parts.push(part(names("part.sky"), difference(sky)));
    let mut random = Random::new(61);
    let mut town = vec![vec![rect(2.0 * t + 0.3, 0.3, width - 4.0 * t - 0.6, 40.0)]];
    let mut x = 2.0 * t;
    while x < width - 2.0 * t {
        let w = random.between(7.0, 12.0);
        let h = random.between(45.0, 75.0);
        town.push(vec![rect(x, 0.3, w, h)]);
        town.push(vec![polygon(&[
            (x - 0.5, h),
            (x + w / 2.0, h + w * 0.6),
            (x + w + 0.5, h),
        ])]);
        x += w + random.between(-0.5, 1.5);
    }
    let town = union(town);
    let town = mml_core::region::combine(
        &[
            town,
            vec![rect(
                2.0 * t + 0.3,
                0.3,
                width - 4.0 * t - 0.6,
                height - 0.6,
            )],
        ],
        mml_core::region::Boolean::Intersection,
    )
    .unwrap_or_default();
    // The town gate: an arch cut through the walls.
    let gate = arched(width / 2.0, 0.3, 14.0, 26.0);
    parts.push(part(names("part.town"), difference(vec![town, gate])));
    Nook {
        parts,
        bridge_holes: vec![wall_hole],
    }
}

/// Cobblestones engraved on the floor.
fn cobbles(width: f64, depth: f64, t: f64) -> Vec<Path> {
    let mut stones = Vec::new();
    let mut row = 0;
    let mut y = t + 2.0;
    while y + 6.0 < t + depth {
        let shift = if row % 2 == 0 { 0.0 } else { 4.5 };
        let mut x = t + 1.0 + shift;
        while x + 8.0 < t + width {
            stones.push(rounded_rect(x, y, 8.0, 5.5, 1.8));
            x += 9.0;
        }
        y += 6.5;
        row += 1;
    }
    stones
}

/// String lights engraved on the ceiling: three garlands and bulbs.
fn lights(width: f64, depth: f64, t: f64) -> Vec<Path> {
    let mut lines = Vec::new();
    for fraction in [0.25, 0.5, 0.75] {
        let y = t + depth * fraction;
        let points: Vec<Point> = (0..=20)
            .map(|step| {
                let s = step as f64 / 20.0;
                point(t + width * s, y + 6.0 * (s * std::f64::consts::PI).sin())
            })
            .collect();
        for bulb in points.iter().step_by(4) {
            lines.push(circle(*bulb + point(0.0, 2.0), 1.2));
        }
        lines.push(Path::new(points, false));
    }
    lines
}

/// Book nook: an alley at night to slide between the books on a shelf.
pub fn book_nook(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "book_nook");
    let nook = nook_parts(&|key: &str| b.t(key));
    b.stock_board();
    cut_then_steps(b, nook.parts, 5)
}

/// Bookend: an upright with a forest along its top and a stag cut out,
/// standing on a base, braced by a gusset behind. Sizes of the upright.
pub const BOOKEND: (f64, f64) = (120.0, 160.0);
/// Where the upright stands on the base, from its back edge.
const BOOKEND_SEAT: f64 = 45.0;

/// The three parts of one bookend.
pub struct Bookend {
    pub upright: Part,
    pub base: Part,
    pub gusset: Part,
}

pub fn bookend_parts(names: [String; 3], mirror: bool) -> Bookend {
    let [upright_name, base_name, gusset_name] = names;
    let (width, height) = BOOKEND;
    let t = slot();
    let clearance = real(0.15);
    let mut random = Random::new(if mirror { 9 } else { 8 });
    let trees = forest(
        &mut random,
        0.0,
        width,
        height * 0.66,
        0.0,
        (22.0, 52.0),
        13.0,
    );
    let clip = mml_core::region::combine(
        &[trees, vec![rect(0.0, 0.0, width, height)]],
        mml_core::region::Boolean::Intersection,
    )
    .unwrap_or_default();
    let stag = crate::art::silhouette(crate::art::STAG, 62.0, mirror);
    let stag_box = mml_core::geometry::bounds_of(&stag);
    let stag = translate(&stag, width / 2.0 - stag_box.width() / 2.0, 42.0);
    let upright = without_specks(
        union(vec![
            clip,
            vec![rect(20.0, -t, 22.0, t + 1.0)],
            vec![rect(width - 42.0, -t, 22.0, t + 1.0)],
        ]),
        // The stag and the gusset slot are cut after.
        80.0,
    );
    let gusset_hole = rect(
        width / 2.0 - t / 2.0 - clearance,
        15.0 - clearance,
        t + 2.0 * clearance,
        22.0 + 2.0 * clearance,
    );
    let mut upright_cut = vec![upright];
    upright_cut.push(stag);
    upright_cut.push(vec![gusset_hole]);
    let upright = part(upright_name, difference(upright_cut));
    // The base: books stand in front of the upright, the gusset behind.
    let depth = 135.0;
    let seat = |x: f64, w: f64, y: f64, h: f64| {
        rect(
            x - clearance,
            y - clearance,
            w + 2.0 * clearance,
            h + 2.0 * clearance,
        )
    };
    let base = Part::new(base_name, rounded_rect(0.0, 0.0, width, depth, 6.0)).with_holes(vec![
        seat(20.0, 22.0, BOOKEND_SEAT, t),
        seat(width - 42.0, 22.0, BOOKEND_SEAT, t),
        seat(width / 2.0 - t / 2.0, t, BOOKEND_SEAT - 35.0, 22.0),
    ]);
    // The gusset, u back from the upright, v up.
    let gusset = union(vec![
        vec![polygon(&[
            (0.0, 0.0),
            (BOOKEND_SEAT - 4.0, 0.0),
            (0.0, 72.0),
        ])],
        vec![rect(13.0, -t, 22.0, t + 0.5)],
        vec![rect(-t, 15.0, t + 0.5, 22.0)],
    ]);
    Bookend {
        upright,
        base,
        gusset: part(gusset_name, gusset),
    }
}

/// A pair of bookends, the stags facing each other.
pub fn bookends(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "bookends");
    let mut parts = Vec::new();
    for (mirror, suffix) in [(false, "1"), (true, "2")] {
        let names = [
            format!("{} {suffix}", b.t("part.upright")),
            format!("{} {suffix}", b.t("part.base")),
            format!("{} {suffix}", b.t("part.gusset")),
        ];
        let end = bookend_parts(names, mirror);
        parts.extend([end.upright, end.base, end.gusset]);
    }
    b.stock_board();
    cut_then_steps(b, parts, 3)
}

/// Inside of the code box: width, depth, height.
pub const CODE_BOX: (f64, f64, f64) = (140.0, 90.0, 60.0);
/// Letters on the dials, in order round each dial.
pub const LETTERS: &str = "ABCDELMNORST";
/// Centres of the dials (and cams) inside the box, from its front left
/// inner corner.
pub const DIALS: [(f64, f64); 4] = [(22.0, 60.0), (54.0, 60.0), (86.0, 60.0), (118.0, 60.0)];
const DIAL: f64 = 14.5;
const CAM: f64 = 12.0;
const NOTCH: (f64, f64) = (6.0, 4.0);
const FINGER: f64 = 4.0;
/// How far the bolt slides back to unlock.
pub const TRAVEL: f64 = 4.0;
/// Bolt: its bar from y = 30 to 40, the knob pin at (70, 35).
const BAR: (f64, f64) = (30.0, 40.0);
const KNOB: (f64, f64) = (70.0, 35.0);
const TONGUE: f64 = 12.0;

/// The code box's parts, and figures the tests check.
pub struct CodeBox {
    pub parts: Vec<Part>,
    /// Gap between a finger tip and its cam when locked.
    #[allow(dead_code)]
    pub finger_gap: f64,
    /// How far the tongue goes into the front wall when locked.
    #[allow(dead_code)]
    pub tongue_in_wall: f64,
}

/// Twelve-pointed star: the axles' profile, so a cam can be set on its
/// axle in any of twelve positions, one per letter.
fn axle_star(centre: Point, grow: f64) -> Path {
    Path::new(
        (0..24)
            .map(|index| {
                let radius = if index % 2 == 0 { 4.5 } else { 3.7 } + grow;
                polar(centre, radius, index as f64 * 15.0)
            })
            .collect(),
        true,
    )
}

pub fn code_box_parts(names: &dyn Fn(&str) -> String) -> CodeBox {
    let (width, depth, height) = CODE_BOX;
    let t = slot();
    let clear = real(0.3);
    let params = BoxParams {
        inner: [width, depth, height],
        thickness: t,
        finger: 12.0,
        lid: false,
        kerf: 0.0,
        spacing: 4.0,
    };
    let box_panels = panels(&params);
    let panel = |name: &str| {
        box_panels
            .iter()
            .find(|panel| panel.name == name)
            .expect("a box panel")
            .outline
            .clone()
    };
    let mut parts = Vec::new();
    // The front wall takes the tongue of the bolt, one layer under the
    // lid's lip (wall coordinates: x across from its left edge, y up).
    let tongue_hole = rect(
        t + KNOB.0 - TONGUE / 2.0 - clear,
        height - t - clear,
        TONGUE + 2.0 * clear,
        t + 2.0 * clear,
    );
    parts.push(Part::new(names("part.front"), panel("front")).with_holes(vec![tongue_hole]));
    parts.push(Part::new(names("part.back"), panel("back")));
    parts.push(Part::new(names("part.left"), panel("left")));
    parts.push(Part::new(names("part.right"), panel("right")));
    parts.push(Part::new(names("part.bottom"), panel("bottom")));
    // Layers under the lid fit inside the walls.
    let lip_gap = real(0.25);
    let lip = rect(
        lip_gap,
        lip_gap,
        width - 2.0 * lip_gap,
        depth - 2.0 * lip_gap,
    );
    let pins: Vec<Path> = [(2.2, depth - 2.2), (width - 2.2, depth - 2.2)]
        .iter()
        .map(|(x, y)| circle(point(*x, *y), real(1.0)))
        .collect();
    let axle_holes: Vec<Path> = DIALS
        .iter()
        .map(|(x, y)| circle(point(*x, *y), 4.5 + clear))
        .collect();
    let knob_slot = rect(
        KNOB.0 - 2.5 - clear,
        KNOB.1 - 2.5 - clear,
        5.0 + 2.0 * clear,
        5.0 + TRAVEL + 2.0 * clear,
    );
    // Lid: as large as the box, on the walls.
    let mut lid_engraving = vec![rect(
        -t + 4.0,
        -t + 4.0,
        width + 2.0 * t - 8.0,
        depth + 2.0 * t - 8.0,
    )];
    for (x, y) in DIALS {
        let tip = y - DIAL - 1.0;
        lid_engraving.push(polygon(&[
            (x, tip),
            (x - 2.5, tip - 4.0),
            (x + 2.5, tip - 4.0),
        ]));
    }
    // An arrow beside the knob: slide back to open.
    lid_engraving.push(Path::new(
        vec![
            point(KNOB.0 + 9.0, KNOB.1 - 3.0),
            point(KNOB.0 + 9.0, KNOB.1 + 7.0),
        ],
        false,
    ));
    lid_engraving.push(Path::new(
        vec![
            point(KNOB.0 + 7.0, KNOB.1 + 5.0),
            point(KNOB.0 + 9.0, KNOB.1 + 7.0),
            point(KNOB.0 + 11.0, KNOB.1 + 5.0),
        ],
        false,
    ));
    let mut lid_holes = axle_holes.clone();
    lid_holes.push(knob_slot.clone());
    lid_holes.extend(pins.clone());
    parts.push(
        Part::new(
            names("part.lid"),
            rect(-t, -t, width + 2.0 * t, depth + 2.0 * t),
        )
        .with_holes(lid_holes)
        .with_engraving(lid_engraving),
    );
    // First layer under the lid: holds the bolt and the cams down.
    let mut cover_holes = axle_holes.clone();
    cover_holes.push(knob_slot);
    cover_holes.extend(pins.clone());
    parts.push(Part::new(names("part.cover"), lip.clone()).with_holes(cover_holes));
    // Second layer: a frame round the mechanism, open at the front for
    // the tongue.
    let frame = difference(vec![
        vec![lip.clone()],
        vec![rect(4.0, 4.0, width - 8.0, depth - 8.0)],
        vec![rect(KNOB.0 - TONGUE / 2.0 - 0.5, -1.0, TONGUE + 1.0, 6.0)],
        pins.clone(),
    ]);
    parts.push(part(names("part.frame"), frame));
    // Third layer: closes the mechanism from below.
    parts.push(Part::new(names("part.base_plate"), lip).with_holes(pins));
    // The bolt: a bar, a finger towards each cam, the tongue forward.
    let finger_tip = DIALS[0].1 - CAM - clear;
    let mut bolt = vec![
        vec![rect(
            4.0 + clear,
            BAR.0,
            width - 8.0 - 2.0 * clear,
            BAR.1 - BAR.0,
        )],
        vec![rect(KNOB.0 - TONGUE / 2.0, -t + 0.5, TONGUE, BAR.0 + t)],
    ];
    for (x, _) in DIALS {
        bolt.push(vec![rect(
            x - FINGER / 2.0,
            BAR.1 - 0.5,
            FINGER,
            finger_tip - BAR.1 + 0.5,
        )]);
    }
    parts.push(part(names("part.bolt"), union(bolt)));
    // Cams, dials and the four discs of each axle.
    for (index, (x, y)) in DIALS.iter().enumerate() {
        let centre = point(*x, *y);
        let number = (index + 1).to_string();
        let cam = difference(vec![
            vec![circle(centre, CAM)],
            vec![rect(
                x - NOTCH.0 / 2.0,
                y - CAM - 1.0,
                NOTCH.0,
                NOTCH.1 + 1.0,
            )],
            vec![axle_star(centre, real(0.15))],
        ]);
        parts.push(part(format!("{} {number}", names("part.cam")), cam));
        // Dial: notched rim for the fingers, letters round it upright
        // when they face the pointer at the front.
        let mut rim = Vec::new();
        for step in 0..96 {
            let angle = step as f64 * 3.75;
            let bump = if (step / 2) % 2 == 0 { 0.0 } else { 0.8 };
            rim.push(polar(centre, DIAL - bump, angle));
        }
        let dial = difference(vec![
            vec![Path::new(rim, true)],
            vec![axle_star(centre, real(0.15))],
            vec![circle(centre, real(1.0))],
        ]);
        let mut letters = Vec::new();
        for (slot_index, letter) in LETTERS.chars().enumerate() {
            let angle = -90.0 + slot_index as f64 * 30.0;
            let at = polar(centre, DIAL - 4.5, angle);
            let glyph = stroke_text(&letter.to_string(), 3.4, point(0.0, 0.0));
            letters.extend(transform(
                &glyph,
                &Transform::rotate_degrees(angle + 90.0).then(Transform::translate(at.x, at.y)),
            ));
        }
        parts.push(part(format!("{} {number}", names("part.dial")), dial).with_engraving(letters));
        for (key, round) in [
            ("part.axle_star", false),
            ("part.axle_round", true),
            ("part.axle_round", true),
            ("part.axle_star", false),
        ] {
            let disc = if round {
                circle(centre, 4.5)
            } else {
                axle_star(centre, 0.0)
            };
            parts.push(
                Part::new(format!("{} {number}", names(key)), disc)
                    .with_holes(vec![circle(centre, real(1.0))]),
            );
        }
    }
    // The knob: a pin through the cover and the lid, and its cap.
    for _ in 0..2 {
        parts.push(Part::new(names("part.pin"), rect(0.0, 0.0, 5.0, 5.0)));
    }
    parts.push(Part::new(names("part.knob"), circle(point(0.0, 0.0), 7.0)));
    CodeBox {
        parts,
        finger_gap: DIALS[0].1 - CAM - finger_tip,
        tongue_in_wall: t - 0.5,
    }
}

/// Code box: four letter dials on the lid; under it, a cam on each dial
/// and a bolt that slides back to free the lid only when the four cam
/// notches face its fingers.
pub fn code_box(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "code_box");
    let code_box = code_box_parts(&|key: &str| b.t(key));
    b.stock_board();
    cut_then_steps(b, code_box.parts, 7)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(key: &str) -> String {
        key.to_owned()
    }

    #[test]
    fn the_code_box_locks_until_the_notches_face_the_fingers() {
        let code_box = code_box_parts(&names);
        // Locked: a little play between the fingers and the cams, less
        // than the travel, so the bolt cannot move back past a cam.
        assert!(code_box.finger_gap > 0.0 && code_box.finger_gap < 1.0);
        // Unlocked: the finger goes into the notch but not to its end.
        assert!(TRAVEL > code_box.finger_gap && TRAVEL - code_box.finger_gap < NOTCH.1);
        assert!(FINGER < NOTCH.0);
        // The tongue leaves the wall when the bolt slides back.
        assert!(code_box.tongue_in_wall > 0.0 && code_box.tongue_in_wall < TRAVEL);
        // Twelve letters for twelve star positions.
        assert_eq!(LETTERS.chars().count(), 12);
        // Dials do not touch.
        assert!(DIALS[1].0 - DIALS[0].0 > 2.0 * DIAL);
        let names: Vec<&str> = code_box
            .parts
            .iter()
            .map(|part| part.name.as_str())
            .collect();
        assert!(names.contains(&"part.bolt") && names.contains(&"part.lid"));
    }

    #[test]
    fn the_nook_bridge_goes_through_walls_and_house_fronts() {
        let nook = nook_parts(&names);
        assert_eq!(nook.bridge_holes.len(), 1);
        let wall = nook
            .parts
            .iter()
            .find(|part| part.name == "part.wall_right")
            .expect("a wall");
        let hole = wall.holes[0].bounds();
        // Every house front has a hole of the same size.
        for name in [
            "part.houses_right_back",
            "part.houses_right_front",
            "part.houses_left_back",
            "part.houses_left_front",
        ] {
            let front = nook
                .parts
                .iter()
                .find(|part| part.name == name)
                .expect("a front");
            assert!(
                front.holes.iter().any(|candidate| {
                    let bounds = candidate.bounds();
                    (bounds.width() - hole.width()).abs() < 1e-6
                        && (bounds.height() - hole.height()).abs() < 1e-6
                }),
                "{name}"
            );
        }
        let bridge = nook
            .parts
            .iter()
            .find(|part| part.name == "part.bridge")
            .expect("bridge");
        // Tabs reach through two layers and the wall on each side.
        let (width, _, _) = NOOK;
        assert!((bridge.outline.bounds().width() - (width + 2.0 * slot())).abs() < 1e-6);
    }

    #[test]
    fn bookend_tabs_find_their_seats() {
        let end = bookend_parts(["u".into(), "b".into(), "g".into()], false);
        assert_eq!(end.base.holes.len(), 3);
        // The upright is one piece with only its stag and the gusset
        // slot cut out: no specks between the firs.
        assert_eq!(
            end.upright.holes.len(),
            2,
            "{:?}",
            end.upright
                .holes
                .iter()
                .map(|h| h.bounds())
                .collect::<Vec<_>>()
        );
        assert!(end.gusset.holes.is_empty());
    }
}
