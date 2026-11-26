//! Larger structures in plywood: a Ferris wheel and a windmill that turn
//! on axles, and a castle with a drawbridge and a portcullis.

use crate::Builder;
use crate::fit::{real, slot};
use crate::layered::Random;
use crate::mechanisms::{AXLE, axle_hole, bearing, box_panel};
use crate::models3d::cut_then_steps;
use crate::puzzles3d::vertical_slot;
use crate::shapes::*;
use crate::tests_projects::text_language;
use mml_core::Document;
use mml_core::geometry::{Path, Point};
use mml_core::makers::boxes::BoxParams;
use mml_core::region::{clip_lines, offset_paths, region_of};

/// Rectangle `along` × `across` centred on `centre`, turned so `along`
/// points to `degrees`.
fn turned_rect(centre: Point, along: f64, across: f64, degrees: f64) -> Path {
    let (a, b) = (along / 2.0, across / 2.0);
    let turn = degrees.to_radians();
    let (c, s) = (turn.cos(), turn.sin());
    Path::new(
        [(-a, -b), (a, -b), (a, b), (-a, b)]
            .iter()
            .map(|(x, y)| centre + point(x * c - y * s, x * s + y * c))
            .collect(),
        true,
    )
}

/// An A-frame standing on a rail: legs to a boss at `apex`, a bearing
/// for an axle there, a cross bar, slots from below for two braces.
fn a_frame(length: f64, apex_height: f64, brace_at: [f64; 2], meet: f64) -> Vec<Path> {
    let apex = point(length / 2.0, apex_height);
    let rail = 14.0;
    let leg = |foot: Point| offset_paths(&[Path::new(vec![foot, apex], false)], 5.0);
    let bar = apex_height * 0.45;
    let mut frame = vec![union(vec![
        vec![rect(0.0, 0.0, length, rail)],
        leg(point(20.0, rail - 2.0)),
        leg(point(length - 20.0, rail - 2.0)),
        vec![circle(apex, 12.0)],
        vec![rect(length * 0.27, bar, length * 0.46, 8.0)],
    ])];
    frame.push(vec![bearing(apex)]);
    for x in brace_at {
        frame.push(vertical_slot(x, -5.0, meet));
    }
    difference(frame)
}

/// A brace seen from the front: wide feet, two slots from its top where
/// the frames cross it, `gap` apart inside.
fn brace(gap: f64, meet: f64) -> Vec<Path> {
    let t = slot();
    let mut brace = vec![vec![rounded_rect(-65.0, 0.0, 130.0, 24.0, 4.0)]];
    for side in [-1.0, 1.0] {
        brace.push(vertical_slot(side * (gap / 2.0 + t / 2.0), meet, 30.0));
    }
    difference(brace)
}

/// Collars, a crank with its three-disc handle, axle strips: the turning
/// parts every axle project shares.
fn axle_parts(name: &dyn Fn(&str) -> String, length: f64, spacers: usize) -> Vec<Part> {
    let mut parts = Vec::new();
    for _ in 0..2 {
        parts.push(Part::new(name("part.axle"), rect(0.0, 0.0, length, AXLE)));
    }
    for _ in 0..spacers {
        parts.push(
            Part::new(name("part.spacer"), circle(point(0.0, 0.0), 8.0))
                .with_holes(vec![axle_hole(point(0.0, 0.0))]),
        );
    }
    let crank = union(vec![
        vec![circle(point(0.0, 0.0), 9.0)],
        vec![rect(0.0, -6.0, 34.0, 12.0)],
        vec![circle(point(34.0, 0.0), 7.0)],
    ]);
    parts.push(
        Part::new(name("part.crank"), crank[0].clone())
            .with_holes(vec![axle_hole(point(0.0, 0.0))]),
    );
    for _ in 0..3 {
        parts.push(Part::new(name("part.handle"), circle(point(0.0, 0.0), 4.5)));
    }
    parts
}

/// Ferris wheel sizes: rim radius, gap between the two wheels, axle
/// height.
pub const WHEEL: (f64, f64, f64) = (68.0, 40.0, 104.0);
const GONDOLA_PIN: f64 = 62.0;
const GONDOLA_DROP: f64 = 32.0;
const RUNG_AT: f64 = 36.0;

/// Ferris wheel parts and figures the tests check.
pub struct FerrisWheel {
    pub parts: Vec<Part>,
    /// Lowest point of a gondola at the bottom of the turn.
    #[allow(dead_code)]
    pub lowest_gondola: f64,
    /// Inside gap between the frames.
    #[allow(dead_code)]
    pub frame_gap: f64,
}

pub fn ferris_parts(name: &dyn Fn(&str) -> String) -> FerrisWheel {
    let t = slot();
    let clear = real(0.15);
    let (radius, gap, height) = WHEEL;
    let centre = point(0.0, 0.0);
    // A wheel: rim, eight spokes, a hub; holes for the gondola pins and
    // for the rungs that tie the two wheels together.
    let mut spokes = vec![vec![circle(centre, 12.0)]];
    for index in 0..8 {
        let angle = index as f64 * 45.0;
        spokes.push(vec![turned_rect(
            polar(centre, radius / 2.0, angle),
            radius - 4.0,
            8.0,
            angle,
        )]);
    }
    let rim = difference(vec![
        vec![circle(centre, radius)],
        vec![circle(centre, radius - 7.0)],
    ]);
    let mut wheel = vec![union(vec![rim, union(spokes)])];
    for index in 0..8 {
        let angle = index as f64 * 45.0;
        wheel.push(vec![circle(
            polar(centre, GONDOLA_PIN + 2.5, angle + 22.5),
            real(1.05),
        )]);
        wheel.push(vec![turned_rect(
            polar(centre, RUNG_AT, angle),
            4.0 + 2.0 * clear,
            t + 2.0 * clear,
            angle,
        )]);
    }
    wheel.push(vec![axle_hole(centre)]);
    let wheel = difference(wheel);
    let mut parts = vec![
        Part::from_area(format!("{} 1", name("part.wheel")), wheel.clone()),
        Part::from_area(format!("{} 2", name("part.wheel")), wheel),
    ];
    // Rungs: plates `gap` long with a tab at each end into the spokes.
    for number in 1..=8 {
        let rung = union(vec![
            vec![rect(0.0, 0.0, gap, 10.0)],
            vec![rect(-t, 3.0, t + 0.5, 4.0)],
            vec![rect(gap - 0.5, 3.0, t + 0.5, 4.0)],
        ]);
        parts.push(Part::from_area(
            format!("{} {number}", name("part.rung")),
            rung,
        ));
    }
    // Gondolas: a loose ring round the pin, a hanger, a cabin with two
    // windows.
    for number in 1..=8 {
        let top = point(0.0, 0.0);
        let cabin = union(vec![
            vec![circle(top, 4.0)],
            vec![rect(-1.5, -14.0, 3.0, 14.0)],
            vec![rounded_rect(-13.0, -GONDOLA_DROP, 26.0, 19.0, 4.0)],
            vec![rect(-15.0, -15.0, 30.0, 3.0)],
        ]);
        let gondola = difference(vec![
            cabin,
            vec![circle(top, real(1.6))],
            vec![rounded_rect(-10.0, -24.0, 8.0, 8.0, 1.5)],
            vec![rounded_rect(2.0, -24.0, 8.0, 8.0, 1.5)],
        ]);
        parts.push(Part::from_area(
            format!("{} {number}", name("part.gondola")),
            gondola,
        ));
    }
    // Stand.
    let length = 170.0;
    let meet = 7.0;
    let brace_at = [15.0, length - 15.0];
    let frame_gap = gap + 2.0 * t + 4.0 * t;
    let frame = a_frame(length, height, brace_at, meet);
    for number in 1..=2 {
        parts.push(Part::from_area(
            format!("{} {number}", name("part.frame")),
            frame.clone(),
        ));
        parts.push(Part::from_area(
            format!("{} {number}", name("part.brace")),
            brace(frame_gap, meet),
        ));
    }
    // Axle: crank, frame, two spacers, wheel, gap, wheel, two spacers,
    // frame, two spacers: the frame gap and five thicknesses.
    let axle_length = frame_gap + 5.0 * t + 2.0;
    parts.extend(axle_parts(name, axle_length, 6));
    let lowest_gondola = height - GONDOLA_PIN - 2.5 - GONDOLA_DROP;
    FerrisWheel {
        parts,
        lowest_gondola,
        frame_gap,
    }
}

pub fn ferris_wheel(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "ferris_wheel");
    let wheel = ferris_parts(&|key: &str| b.t(key));
    b.stock_board();
    cut_then_steps(b, wheel.parts, 4)
}

/// Windmill tower inside: width, depth, height; gable height.
pub const MILL: (f64, f64, f64) = (64.0, 64.0, 105.0);
const GABLE: f64 = 36.0;
const MILL_AXLE: f64 = 14.0;
const SAIL: f64 = 72.0;

/// `part` with its engraving kept to its wood (not over its holes).
fn engraved_on_wood(mut part: Part) -> Part {
    let mut paths = vec![part.outline.clone()];
    paths.extend(part.holes.iter().cloned());
    part.engrave = clip_lines(&part.engrave, &region_of(&paths));
    part
}

/// Stones engraved over a wall: staggered rows of irregular blocks.
fn stones(random: &mut Random, x0: f64, y0: f64, width: f64, height: f64) -> Vec<Path> {
    let mut lines = Vec::new();
    let mut y = y0 + 6.0;
    let mut row = 0;
    while y < y0 + height {
        lines.push(Path::new(vec![point(x0, y), point(x0 + width, y)], false));
        let mut x = x0 + if row % 2 == 0 { 0.0 } else { 5.0 };
        while x < x0 + width {
            lines.push(Path::new(vec![point(x, y - 6.0), point(x, y)], false));
            x += random.between(8.0, 13.0);
        }
        y += 6.0;
        row += 1;
    }
    lines
}

/// Windmill parts, and figures the tests check.
pub struct Windmill {
    pub parts: Vec<Part>,
    /// Height of the axle over the ground, and the sails' reach.
    #[allow(dead_code)]
    pub axle_height: f64,
}

pub fn windmill_parts(name: &dyn Fn(&str) -> String) -> Windmill {
    let t = slot();
    let (width, depth, height) = MILL;
    let params = BoxParams {
        inner: [width, depth, height],
        thickness: t,
        finger: 12.0,
        lid: false,
        kerf: 0.0,
        spacing: 4.0,
    };
    let mut random = Random::new(29);
    let outer = width + 2.0 * t;
    let top = height + t;
    let axle_at = point(outer / 2.0, top + MILL_AXLE);
    let gable = polygon(&[
        (0.0, top - 0.5),
        (outer, top - 0.5),
        (outer / 2.0, top + GABLE),
    ]);
    let arch = |x: f64, bottom: f64, w: f64, h: f64| {
        union(vec![
            vec![rect(x - w / 2.0, bottom, w, h - w / 2.0)],
            vec![circle(point(x, bottom + h - w / 2.0), w / 2.0)],
        ])
    };
    let wall_stones = |random: &mut Random| stones(random, t, t, width, height);
    // Front: gable, bearing for the sails, the door and a window.
    let mut front = vec![union(vec![
        vec![box_panel(&params, "front")],
        vec![gable.clone()],
    ])];
    front.push(vec![bearing(axle_at)]);
    front.push(arch(outer / 2.0, t, 18.0, 32.0));
    front.push(arch(outer / 2.0, t + 62.0, 10.0, 16.0));
    let mut parts = vec![
        Part::from_area(name("part.front"), difference(front))
            .with_engraving(wall_stones(&mut random)),
    ];
    let mut back = vec![union(vec![vec![box_panel(&params, "back")], vec![gable]])];
    back.push(vec![bearing(axle_at)]);
    back.push(arch(outer / 2.0, t + 62.0, 10.0, 16.0));
    parts.push(
        Part::from_area(name("part.back"), difference(back))
            .with_engraving(wall_stones(&mut random)),
    );
    for (key, panel) in [("part.left", "left"), ("part.right", "right")] {
        let window = arch(depth / 2.0, t + 40.0, 10.0, 16.0);
        let side = difference(vec![vec![box_panel(&params, panel)], window]);
        parts.push(Part::from_area(name(key), side).with_engraving(stones(
            &mut random,
            0.0,
            t,
            depth,
            height,
        )));
    }
    parts.push(Part::new(name("part.bottom"), box_panel(&params, "bottom")));
    // Roof: two panels resting on the gables, overhanging.
    let slope = ((outer / 2.0 + 3.0).powi(2) + (GABLE + 2.0).powi(2)).sqrt();
    let roof_depth = depth + 2.0 * t + 12.0;
    let mut shingles = Vec::new();
    let mut y = 5.0;
    while y < slope {
        shingles.push(Path::new(vec![point(0.0, y), point(roof_depth, y)], false));
        y += 5.0;
    }
    for number in 1..=2 {
        parts.push(
            Part::new(
                format!("{} {number}", name("part.roof")),
                rect(0.0, 0.0, roof_depth, slope),
            )
            .with_engraving(shingles.clone()),
        );
    }
    // Sails: four lattice blades round a hub, one piece.
    let centre = point(0.0, 0.0);
    let mut sails = vec![vec![circle(centre, 11.0)]];
    let mut lattice_holes = Vec::new();
    for index in 0..4 {
        let angle = index as f64 * 90.0;
        sails.push(vec![turned_rect(
            polar(centre, SAIL / 2.0, angle),
            SAIL,
            5.0,
            angle,
        )]);
        // The blade beside the spar, from 22 mm out.
        let along =
            |r: f64, off: f64| polar(centre, r, angle) + polar(point(0.0, 0.0), off, angle + 90.0);
        let blade = Path::new(
            vec![
                along(22.0, 2.0),
                along(SAIL, 2.0),
                along(SAIL, 20.0),
                along(22.0, 16.0),
            ],
            true,
        );
        sails.push(vec![blade]);
        for cell in 0..4 {
            let r0 = 26.0 + cell as f64 * 11.5;
            lattice_holes.push(Path::new(
                vec![
                    along(r0, 5.5),
                    along(r0 + 8.5, 5.5),
                    along(r0 + 8.5, 16.5),
                    along(r0, 15.0),
                ],
                true,
            ));
        }
    }
    let mut sails = vec![union(sails)];
    sails.extend(lattice_holes.into_iter().map(|hole| vec![hole]));
    sails.push(vec![axle_hole(centre)]);
    parts.push(Part::from_area(name("part.sails"), difference(sails)));
    // Axle: sails, spacer, front gable, the attic, back gable, collar,
    // crank behind.
    let axle_length = depth + 2.0 * t + 4.0 * t + 2.0;
    parts.extend(axle_parts(name, axle_length, 2));
    let parts = parts.into_iter().map(engraved_on_wood).collect();
    Windmill {
        parts,
        axle_height: top + MILL_AXLE,
    }
}

pub fn windmill(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "windmill");
    let mill = windmill_parts(&|key: &str| b.t(key));
    b.stock_board();
    cut_then_steps(b, mill.parts, 4)
}

/// Castle: courtyard inside width and depth, wall height; tower inside
/// size and height.
pub const COURTYARD: (f64, f64, f64) = (120.0, 120.0, 45.0);
pub const TOWER: (f64, f64) = (26.0, 62.0);
const GATE: (f64, f64) = (30.0, 36.0);

/// Merlons along the top edge of a wall from `x0` to `x1` at `top`.
fn merlons(x0: f64, x1: f64, top: f64) -> Vec<Vec<Path>> {
    let count = ((x1 - x0) / 14.0).floor().max(1.0) as usize;
    let pitch = (x1 - x0) / count as f64;
    (0..count)
        .map(|index| {
            let x = x0 + index as f64 * pitch + pitch * 0.2;
            vec![rect(x, top - 0.5, pitch * 0.6, 7.5)]
        })
        .collect()
}

/// Castle parts, and figures the tests check.
pub struct Castle {
    pub parts: Vec<Part>,
    /// Width of the gate, and of the drawbridge and portcullis.
    #[allow(dead_code)]
    pub gate: f64,
    #[allow(dead_code)]
    pub drawbridge: f64,
    #[allow(dead_code)]
    pub portcullis: f64,
}

pub fn castle_parts(name: &dyn Fn(&str) -> String) -> Castle {
    let t = slot();
    let clear = real(0.2);
    let (width, depth, height) = COURTYARD;
    let params = BoxParams {
        inner: [width, depth, height],
        thickness: t,
        finger: 15.0,
        lid: false,
        kerf: 0.0,
        spacing: 4.0,
    };
    let mut random = Random::new(71);
    let top = height + t;
    let outer = width + 2.0 * t;
    let (gate_w, gate_h) = GATE;
    let gate_x = outer / 2.0;
    let mut parts = Vec::new();
    // Front wall: crenellations, the gate, holes for the drawbridge
    // chains above it.
    let mut front = vec![box_panel(&params, "front")];
    front = union(
        std::iter::once(front)
            .chain(merlons(0.0, outer, top))
            .collect(),
    );
    let gate = union(vec![
        vec![rect(
            gate_x - gate_w / 2.0,
            t,
            gate_w,
            gate_h - gate_w / 2.0,
        )],
        vec![circle(
            point(gate_x, t + gate_h - gate_w / 2.0),
            gate_w / 2.0,
        )],
    ]);
    let chains = vec![
        circle(
            point(gate_x - gate_w / 2.0 - 4.0, t + gate_h + 2.0),
            real(1.0),
        ),
        circle(
            point(gate_x + gate_w / 2.0 + 4.0, t + gate_h + 2.0),
            real(1.0),
        ),
    ];
    let mut front_cut = vec![front, gate];
    front_cut.extend(chains.into_iter().map(|hole| vec![hole]));
    parts.push(
        Part::from_area(name("part.front"), difference(front_cut)).with_engraving(stones(
            &mut random,
            0.0,
            t,
            outer,
            height,
        )),
    );
    let back = union(
        std::iter::once(vec![box_panel(&params, "back")])
            .chain(merlons(0.0, outer, top))
            .collect(),
    );
    parts.push(
        Part::from_area(name("part.back"), back).with_engraving(stones(
            &mut random,
            0.0,
            t,
            outer,
            height,
        )),
    );
    for (key, panel) in [("part.left", "left"), ("part.right", "right")] {
        let side = union(
            std::iter::once(vec![box_panel(&params, panel)])
                .chain(merlons(0.0, depth, top))
                .collect(),
        );
        parts.push(Part::from_area(name(key), side).with_engraving(stones(
            &mut random,
            0.0,
            t,
            depth,
            height,
        )));
    }
    parts.push(Part::new(
        name("part.courtyard"),
        box_panel(&params, "bottom"),
    ));
    // Four towers: small open boxes, crenellated, with arrow slits.
    let (tower_side, tower_height) = TOWER;
    let tower = BoxParams {
        inner: [tower_side, tower_side, tower_height],
        thickness: t,
        finger: 10.0,
        lid: false,
        kerf: 0.0,
        spacing: 4.0,
    };
    let tower_top = tower_height + t;
    for number in 1..=4 {
        for (panel, wide) in [
            ("front", true),
            ("back", true),
            ("left", false),
            ("right", false),
        ] {
            let span = if wide {
                tower_side + 2.0 * t
            } else {
                tower_side
            };
            let wall = union(
                std::iter::once(vec![box_panel(&tower, panel)])
                    .chain(merlons(0.0, span, tower_top))
                    .collect(),
            );
            let slit = rect(span / 2.0 - 1.2, t + 34.0, 2.4, 12.0);
            parts.push(Part::from_area(
                format!("{} {number}", name("part.tower")),
                difference(vec![wall, vec![slit]]),
            ));
        }
        parts.push(Part::new(
            format!("{} {number}", name("part.tower_floor")),
            box_panel(&tower, "bottom"),
        ));
    }
    // Drawbridge: planks engraved, two lugs at the bottom for the hinge
    // pin, two holes at the top for the chains.
    let bridge_w = gate_w - 2.0 * clear;
    let bridge_h = gate_h - 2.0;
    let bridge = union(vec![
        vec![rect(0.0, 0.0, bridge_w, bridge_h)],
        vec![rect(2.0, -6.0, 6.0, 6.5)],
        vec![rect(bridge_w - 8.0, -6.0, 6.0, 6.5)],
    ]);
    let mut planks = Vec::new();
    let mut x = 5.0;
    while x < bridge_w {
        planks.push(Path::new(
            vec![point(x, 0.5), point(x, bridge_h - 0.5)],
            false,
        ));
        x += 5.0;
    }
    parts.push(
        Part::from_area(name("part.drawbridge"), bridge)
            .with_holes(vec![
                circle(point(5.0, -3.0), real(1.3)),
                circle(point(bridge_w - 5.0, -3.0), real(1.3)),
                circle(point(3.0, bridge_h - 3.0), real(1.0)),
                circle(point(bridge_w - 3.0, bridge_h - 3.0), real(1.0)),
            ])
            .with_engraving(planks),
    );
    for number in 1..=2 {
        parts.push(
            Part::new(
                format!("{} {number}", name("part.hinge")),
                rect(0.0, 0.0, 6.0, 8.0),
            )
            .with_holes(vec![circle(point(3.0, 4.0), real(1.3))]),
        );
    }
    // Portcullis: a grid that slides up behind the gate between guides.
    let grid_w = gate_w + 8.0;
    let grid_h = gate_h + 6.0;
    let mut grid = vec![vec![rect(0.0, 0.0, grid_w, grid_h)]];
    let mut x = 3.0;
    while x + 4.0 < grid_w - 2.5 {
        let mut y = 3.0;
        while y + 4.0 < grid_h - 2.5 {
            grid.push(vec![rect(x, y, 4.0, 4.0)]);
            y += 6.0;
        }
        x += 6.0;
    }
    parts.push(Part::from_area(name("part.portcullis"), difference(grid)));
    for number in 1..=4 {
        let (w, key) = if number <= 2 {
            (4.0, "part.guide")
        } else {
            (7.0, "part.guide_cap")
        };
        parts.push(Part::new(
            format!("{} {number}", name(key)),
            rect(0.0, 0.0, w, height - 2.0),
        ));
    }
    let parts = parts.into_iter().map(engraved_on_wood).collect();
    Castle {
        parts,
        gate: gate_w,
        drawbridge: bridge_w,
        portcullis: grid_w,
    }
}

pub fn castle(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "castle");
    let castle = castle_parts(&|key: &str| b.t(key));
    b.stock_board();
    cut_then_steps(b, castle.parts, 5)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(key: &str) -> String {
        key.to_owned()
    }

    #[test]
    fn gondolas_clear_the_ground_and_the_parts_fit_boards() {
        let wheel = ferris_parts(&names);
        assert!(wheel.lowest_gondola > 3.0, "{}", wheel.lowest_gondola);
        for part in &wheel.parts {
            let size = part.bounds();
            assert!(size.width().max(size.height()) < 240.0);
        }
        // The axle holds what the guide threads on it: crank, two
        // frames, six spacers, two wheels and the gap between them.
        let t = slot();
        let axle = wheel
            .parts
            .iter()
            .find(|part| part.name == "part.axle")
            .unwrap();
        let stack = 1.0 + 2.0 + 6.0 + 2.0;
        assert!((axle.bounds().width() - (WHEEL.1 + stack * t + 2.0)).abs() < 1e-6);
        // Wheels are one piece with their holes.
        let rim = wheel
            .parts
            .iter()
            .find(|part| part.name.starts_with("part.wheel"))
            .unwrap();
        assert!(rim.holes.len() >= 17);
    }

    #[test]
    fn the_sails_turn_clear_of_the_ground() {
        let mill = windmill_parts(&names);
        assert!(mill.axle_height - SAIL > 20.0);
        let sails = mill
            .parts
            .iter()
            .find(|part| part.name == "part.sails")
            .unwrap();
        assert!((sails.bounds().width() - 2.0 * SAIL).abs() < 25.0);
    }

    #[test]
    fn the_drawbridge_fits_the_gate_and_the_portcullis_covers_it() {
        let castle = castle_parts(&names);
        assert!(castle.drawbridge < castle.gate);
        assert!(castle.portcullis > castle.gate + 4.0);
        assert!(castle.parts.len() > 30);
    }
}
