//! Moving 3D projects: a sea automaton turned by a crank, and a working
//! trebuchet. Axles are two plywood strips glued face to face, so they
//! fit the rectangular holes of what turns with them and turn in the
//! round holes of what holds them.

use crate::Builder;
use crate::fit::{real, slot};
use crate::layered::star;
use crate::layered2::fish;
use crate::models3d::cut_then_steps;
use crate::shapes::*;
use crate::tests_projects::text_language;
use mml_core::Document;
use mml_core::geometry::{Path, Point};
use mml_core::makers::boxes::{BoxParams, panels};

/// Width of an axle strip, mm; the axle is two strips thick.
const AXLE: f64 = 6.0;

/// Rectangular hole an axle fits, centred on `centre`.
fn axle_hole(centre: Point) -> Path {
    let (a, b) = (AXLE + real(0.2), 2.0 * slot() + real(0.2));
    rect(centre.x - a / 2.0, centre.y - b / 2.0, a, b)
}

/// Round hole an axle turns in.
fn bearing(centre: Point) -> Path {
    let t = slot();
    let half_diagonal = (AXLE * AXLE + 4.0 * t * t).sqrt() / 2.0;
    circle(centre, half_diagonal + real(0.35))
}

/// Box panels by name.
fn box_panel(params: &BoxParams, name: &str) -> Path {
    panels(params)
        .into_iter()
        .find(|panel| panel.name == name)
        .expect("a box panel")
        .outline
}

/// Inside of the automaton's box: width, depth, height.
pub const AUTOMATON: (f64, f64, f64) = (150.0, 70.0, 78.0);
/// Axle height above the inside of the bottom, and the rods' guide.
const AXLE_HEIGHT: f64 = 30.0;
const GUIDE_HEIGHT: f64 = 58.0;
/// Depths of the three cams and rods, and of the four fixed panels.
pub const CAM_DEPTHS: [f64; 3] = [17.0, 37.0, 57.0];
const PANEL_DEPTHS: [f64; 4] = [8.0, 27.0, 47.0, 66.0];
/// Rod width, along x.
const ROD: f64 = 6.0;

/// A cam's radius at `angle` (radians): the boat bobs twice a turn, the
/// whale rises and sinks once, the fish rises slowly and drops.
fn cam_radius(index: usize, angle: f64) -> f64 {
    match index {
        0 => 15.0 + 3.5 * (2.0 * angle).sin(),
        1 => {
            // Circle of radius 14 centred 5 mm off the axle.
            let (c, s) = (5.0 * angle.cos(), 5.0 * angle.sin());
            c + (14.0 * 14.0 - s * s).sqrt()
        }
        _ => {
            let turn = angle.rem_euclid(std::f64::consts::TAU) / std::f64::consts::TAU;
            11.0 + 13.0 * turn
        }
    }
}

/// Smallest and largest radius of a cam.
pub fn cam_range(index: usize) -> (f64, f64) {
    (0..720)
        .map(|step| cam_radius(index, step as f64 / 720.0 * std::f64::consts::TAU))
        .fold((f64::INFINITY, 0.0), |(low, high), r| {
            (low.min(r), high.max(r))
        })
}

fn cam(index: usize) -> Vec<Path> {
    let centre = point(0.0, 0.0);
    let outline = Path::new(
        (0..360)
            .map(|step| {
                let angle = step as f64 / 360.0 * std::f64::consts::TAU;
                polar(centre, cam_radius(index, angle), angle.to_degrees())
            })
            .collect(),
        true,
    );
    vec![outline, axle_hole(centre)]
}

/// The automaton's parts, and figures the tests check.
pub struct Automaton {
    pub parts: Vec<Part>,
    /// Lowest and highest top of each rod above the lid.
    #[allow(dead_code)]
    pub rod_tops: Vec<(f64, f64)>,
    /// Gap between the highest cam point and the guide.
    #[allow(dead_code)]
    pub cam_clearance: f64,
}

/// A wave band from x = 0 to `width`, crests around `level` above its
/// base, with two tabs below it.
fn wave_panel(width: f64, level: f64, phase: f64) -> Vec<Path> {
    let t = slot();
    let steps = 60;
    let mut points = vec![(0.0, 0.0)];
    for step in 0..=steps {
        let x = width * step as f64 / steps as f64;
        let angle = x / 28.0 + phase;
        let crest = (angle.sin() + 0.35 * (2.0 * angle).sin()) / 1.35;
        points.push((x, level + 4.0 * crest));
    }
    points.push((width, 0.0));
    union(vec![
        vec![polygon(&points)],
        vec![rect(20.0, -t, 14.0, t + 0.5)],
        vec![rect(width - 34.0, -t, 14.0, t + 0.5)],
    ])
}

/// Little sailing boat, hull bottom at `bottom`, centred on `x`.
fn boat(x: f64, bottom: f64) -> Vec<Path> {
    union(vec![
        vec![polygon(&[
            (x - 20.0, bottom + 7.0),
            (x + 22.0, bottom + 7.0),
            (x + 14.0, bottom),
            (x - 14.0, bottom),
        ])],
        vec![rect(x - 1.0, bottom + 6.0, 2.0, 30.0)],
        vec![polygon(&[
            (x + 2.0, bottom + 10.0),
            (x + 2.0, bottom + 34.0),
            (x + 17.0, bottom + 10.0),
        ])],
        vec![polygon(&[
            (x - 2.0, bottom + 12.0),
            (x - 2.0, bottom + 30.0),
            (x - 13.0, bottom + 12.0),
        ])],
    ])
}

pub fn automaton_parts(name: &dyn Fn(&str) -> String) -> Automaton {
    let (width, depth, height) = AUTOMATON;
    let t = slot();
    let clear = real(0.2);
    let params = BoxParams {
        inner: [width, depth, height],
        thickness: t,
        finger: 14.0,
        lid: true,
        kerf: 0.0,
        spacing: 4.0,
    };
    let axle_x = width / 2.0;
    let mut parts = Vec::new();
    // Walls: the axle turns in the front and back, the guide rests in
    // the sides (wall coordinates: x along the wall, y up from its
    // bottom edge, the inside starting at y = t).
    let axle_at = point(t + axle_x, t + AXLE_HEIGHT);
    parts.push(
        Part::new(name("part.front"), box_panel(&params, "front"))
            .with_holes(vec![bearing(axle_at)]),
    );
    parts.push(
        Part::new(name("part.back"), box_panel(&params, "back")).with_holes(vec![bearing(axle_at)]),
    );
    let guide_tabs = [10.0, depth - 22.0];
    let side_holes: Vec<Path> = guide_tabs
        .iter()
        .map(|from| {
            rect(
                from - clear,
                t + GUIDE_HEIGHT - clear,
                12.0 + 2.0 * clear,
                t + 2.0 * clear,
            )
        })
        .collect();
    parts.push(
        Part::new(name("part.left"), box_panel(&params, "left")).with_holes(side_holes.clone()),
    );
    parts.push(Part::new(name("part.right"), box_panel(&params, "right")).with_holes(side_holes));
    parts.push(Part::new(name("part.bottom"), box_panel(&params, "bottom")));
    // Rod holes in the lid and the guide (plate coordinates: x across,
    // y the depth from the front).
    let rod_holes: Vec<Path> = CAM_DEPTHS
        .iter()
        .map(|y| {
            rect(
                axle_x - ROD / 2.0 - clear,
                y - t / 2.0 - clear,
                ROD + 2.0 * clear,
                t + 2.0 * clear,
            )
        })
        .collect();
    let mut lid_holes = rod_holes.clone();
    for y in PANEL_DEPTHS {
        // The panels are 4 mm narrower than the lid, centred.
        for x in [22.0, width - 36.0] {
            lid_holes.push(rect(
                x - clear,
                y - t / 2.0 - clear,
                14.0 + 2.0 * clear,
                t + 2.0 * clear,
            ));
        }
    }
    parts.push(Part::new(name("part.lid"), box_panel(&params, "lid")).with_holes(lid_holes));
    let mut guide = vec![vec![rect(clear, 0.0, width - 2.0 * clear, depth)]];
    for from in guide_tabs {
        guide.push(vec![rect(-t, from, t + 1.0, 12.0)]);
        guide.push(vec![rect(width - 1.0, from, t + 1.0, 12.0)]);
    }
    let mut guide = vec![union(guide)];
    guide.extend(rod_holes.iter().map(|hole| vec![hole.clone()]));
    parts.push(Part::from_area(name("part.guide"), difference(guide)));
    // Axle, collar and crank.
    let length = depth + 4.0 * t + 2.0;
    let mut marks: Vec<Path> = CAM_DEPTHS
        .iter()
        .map(|y| {
            let along = 2.0 * t + y;
            Path::new(vec![point(along, 0.0), point(along, AXLE)], false)
        })
        .collect();
    // A triangle at the front end, the crank's side.
    marks.push(polygon(&[(1.0, 1.0), (1.0, AXLE - 1.0), (4.0, AXLE / 2.0)]));
    parts.push(Part::new(name("part.axle"), rect(0.0, 0.0, length, AXLE)).with_engraving(marks));
    parts.push(Part::new(name("part.axle"), rect(0.0, 0.0, length, AXLE)));
    parts.push(
        Part::new(name("part.collar"), circle(point(0.0, 0.0), 9.0))
            .with_holes(vec![axle_hole(point(0.0, 0.0))]),
    );
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
    // Lid top in the frame of the rods: z = 0 at the inside bottom.
    let lid_top = height + t;
    let mut rod_tops = Vec::new();
    for (index, key) in ["part.cam_boat", "part.cam_whale", "part.cam_fish"]
        .iter()
        .enumerate()
    {
        // A dot marks the face that looks to the front.
        parts.push(
            Part::from_area(name(key), cam(index))
                .with_engraving(vec![circle(point(0.0, 7.5), 1.2)]),
        );
        let (low, high) = cam_range(index);
        // The rod rests on the cam: its bottom at the cam's lowest
        // point, its top 6 mm over the lid.
        let bottom = AXLE_HEIGHT + low;
        let top = lid_top + 6.0;
        rod_tops.push((top - lid_top, top + high - low - lid_top));
        let mut piece = vec![
            vec![rect(
                axle_x - ROD / 2.0,
                bottom + ROD / 2.0,
                ROD,
                top - bottom - ROD / 2.0 + 4.0,
            )],
            vec![circle(point(axle_x, bottom + ROD / 2.0), ROD / 2.0)],
        ];
        // Each figure sits on an arm hidden behind the waves in front.
        let (figure_x, figure_bottom, figure): (f64, f64, Vec<Path>) = match index {
            0 => (40.0, lid_top + 16.0, boat(40.0, lid_top + 16.0)),
            1 => {
                let whale =
                    crate::art::picked(crate::art::WHALE_SHAPES, crate::art::WHALE, 28.0, true);
                let size = mml_core::geometry::bounds_of(&whale);
                let bottom = lid_top + 22.0;
                (
                    82.0,
                    bottom,
                    translate(&whale, 82.0 - size.width() / 2.0, bottom),
                )
            }
            _ => {
                let bottom = lid_top + 28.0;
                (
                    114.0,
                    bottom,
                    vec![fish(point(114.0, bottom + 7.0), 30.0, false)],
                )
            }
        };
        piece.push(vec![rect(
            figure_x.min(axle_x) - 3.0,
            top,
            (figure_x - axle_x).abs() + 6.0,
            5.0,
        )]);
        piece.push(vec![rect(
            figure_x - 3.0,
            top,
            6.0,
            figure_bottom - top + 3.0,
        )]);
        piece.push(figure);
        let piece = union(piece);
        parts.push(Part::from_area(
            name(&key.replace("cam_", "figure_")),
            piece,
        ));
    }
    // The fixed scenery: three wave bands and the sky at the back.
    for (index, level) in [22.0, 30.0, 36.0].iter().enumerate() {
        parts.push(Part::from_area(
            format!("{} {}", name("part.waves"), index + 1),
            wave_panel(width - 4.0, *level, index as f64 * 1.3),
        ));
    }
    let sky = union(vec![
        vec![rect(0.0, 0.0, width - 4.0, 70.0)],
        vec![rect(20.0, -t, 14.0, t + 0.5)],
        vec![rect(width - 38.0, -t, 14.0, t + 0.5)],
    ]);
    let mut sky_cut = vec![sky];
    sky_cut.push(vec![circle(point(width - 30.0, 52.0), 9.0)]);
    for (x, y) in [(20.0, 60.0), (45.0, 54.0), (70.0, 63.0)] {
        sky_cut.push(vec![star(point(x, y), 3.0, 1.3)]);
    }
    parts.push(Part::from_area(name("part.sky"), difference(sky_cut)));
    let highest = (0..3).map(|index| cam_range(index).1).fold(0.0, f64::max);
    Automaton {
        parts,
        rod_tops,
        cam_clearance: GUIDE_HEIGHT - (AXLE_HEIGHT + highest),
    }
}

/// Sea automaton: turn the crank, a boat bobs, a whale rises and a fish
/// jumps between three bands of waves.
pub fn sea_automaton(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "sea_automaton");
    let automaton = automaton_parts(&|key: &str| b.t(key));
    b.stock_board();
    cut_then_steps(b, automaton.parts, 5)
}

/// Trebuchet: base rail length, pivot height, and the arm on each side
/// of the pivot.
pub const TREBUCHET: (f64, f64) = (170.0, 120.0);
pub const ARM: (f64, f64) = (40.0, 130.0);
/// Spacer discs on the axle each side of the arm.
const SPACERS: usize = 7;

/// The trebuchet's parts, and figures the tests check.
pub struct Trebuchet {
    pub parts: Vec<Part>,
    /// Inside gap between the frames.
    #[allow(dead_code)]
    pub gap: f64,
    /// Width of the counterweight with its hangers.
    #[allow(dead_code)]
    pub weight_width: f64,
    /// Lowest point the counterweight reaches, swinging under the pivot.
    #[allow(dead_code)]
    pub weight_lowest: f64,
}

pub fn trebuchet_parts(name: &dyn Fn(&str) -> String) -> Trebuchet {
    let t = slot();
    let (length, pivot) = TREBUCHET;
    let gap = (2 * SPACERS + 1) as f64 * t;
    let apex = point(length / 2.0, pivot);
    let rail = 14.0;
    let meet = rail / 2.0;
    let brace_at = [15.0, length - 15.0];
    // Frame: rail, two legs to the apex, a cross bar, slots for the braces
    // from below.
    let leg =
        |foot: Point| mml_core::region::offset_paths(&[Path::new(vec![foot, apex], false)], 5.0);
    let mut frame = vec![
        vec![rect(0.0, 0.0, length, rail)],
        leg(point(25.0, rail - 2.0)),
        leg(point(length - 25.0, rail - 2.0)),
        vec![circle(apex, 12.0)],
        vec![rect(45.0, 55.0, length - 90.0, 8.0)],
    ];
    frame = vec![union(frame)];
    frame.push(vec![bearing(apex)]);
    for x in brace_at {
        frame.push(crate::puzzles3d::vertical_slot(x, -5.0, meet));
    }
    let frame = difference(frame);
    let mut parts = vec![
        Part::from_area(format!("{} 1", name("part.frame")), frame.clone()),
        Part::from_area(format!("{} 2", name("part.frame")), frame),
    ];
    // Braces seen from the front: wide feet, slots from the top where the
    // frames cross them.
    for number in 1..=2 {
        let mut brace = vec![vec![rounded_rect(-60.0, 0.0, 120.0, 24.0, 4.0)]];
        for side in [-1.0, 1.0] {
            brace.push(crate::puzzles3d::vertical_slot(
                side * (gap / 2.0 + t / 2.0),
                meet,
                30.0,
            ));
        }
        parts.push(Part::from_area(
            format!("{} {number}", name("part.brace")),
            difference(brace),
        ));
    }
    // Axle, spacers and collars.
    let axle_length = gap + 4.0 * t + 2.0;
    for _ in 0..2 {
        parts.push(Part::new(
            name("part.axle"),
            rect(0.0, 0.0, axle_length, AXLE),
        ));
    }
    for _ in 0..2 * SPACERS + 2 {
        parts.push(
            Part::new(name("part.spacer"), circle(point(0.0, 0.0), 8.0))
                .with_holes(vec![axle_hole(point(0.0, 0.0))]),
        );
    }
    // The arm: the pivot, a hole for the counterweight pin, a hole to tie
    // the sling and a hook at the tip for its loop.
    let (short, long) = ARM;
    let arm = union(vec![
        vec![rounded_rect(
            -short - 6.0,
            -5.0,
            short + long + 6.0,
            10.0,
            5.0,
        )],
        vec![circle(point(0.0, 0.0), 9.0)],
    ]);
    let hook = polygon(&[(long - 6.0, 5.1), (long - 1.0, 5.1), (long - 3.0, 1.5)]);
    parts.push(
        Part::from_area(name("part.arm"), difference(vec![arm, vec![hook]])).with_holes(vec![
            axle_hole(point(0.0, 0.0)),
            circle(point(-short, 0.0), real(1.3)),
            circle(point(long - 14.0, 0.0), real(1.0)),
        ]),
    );
    // Counterweight: a small box with a lid, hanging between two plates
    // from a pin through the arm.
    let side = 26.0;
    let weight = BoxParams {
        inner: [side, side, side],
        thickness: t,
        finger: 9.0,
        lid: true,
        kerf: 0.0,
        spacing: 4.0,
    };
    for (number, panel) in panels(&weight).into_iter().enumerate() {
        parts.push(Part::new(
            format!("{} {}", name("part.weight"), number + 1),
            panel.outline,
        ));
    }
    let outer = side + 2.0 * t;
    let hanger = rect(0.0, 0.0, 14.0, outer + 16.0);
    for number in 1..=2 {
        parts.push(
            Part::new(format!("{} {number}", name("part.hanger")), hanger.clone())
                .with_holes(vec![circle(point(7.0, outer + 10.0), real(1.3))]),
        );
    }
    // Swinging under the pivot, the box hangs below the short end.
    let weight_lowest = pivot - short - (10.0 + outer);
    Trebuchet {
        parts,
        gap,
        weight_width: outer + 2.0 * t,
        weight_lowest,
    }
}

/// Trebuchet: a counterweight siege engine that throws a paper ball.
pub fn trebuchet(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "trebuchet");
    let trebuchet = trebuchet_parts(&|key: &str| b.t(key));
    b.stock_board();
    cut_then_steps(b, trebuchet.parts, 4)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(key: &str) -> String {
        key.to_owned()
    }

    #[test]
    fn cams_lift_the_figures_and_stay_under_the_guide() {
        let automaton = automaton_parts(&names);
        assert!(automaton.cam_clearance > 2.0, "{}", automaton.cam_clearance);
        for (index, (low, high)) in automaton.rod_tops.iter().enumerate() {
            let lift = high - low;
            assert!(lift > 5.0 && lift < 16.0, "{index}: {lift}");
            // The rod never drops out of the lid.
            assert!(*low > 0.0);
        }
        // The cams clear the bottom and fit the box.
        for index in 0..3 {
            let (low, high) = cam_range(index);
            assert!(AXLE_HEIGHT - high > 2.0 && low > AXLE * 0.75);
        }
    }

    #[test]
    fn the_trebuchet_arm_and_weight_swing_clear() {
        let trebuchet = trebuchet_parts(&names);
        // The counterweight passes between the frames and above the
        // ground.
        assert!(trebuchet.weight_width < trebuchet.gap - 2.0);
        assert!(trebuchet.weight_lowest > 10.0);
        // Spacers fill the gap either side of the arm.
        let t = slot();
        assert!((trebuchet.gap - (2 * SPACERS + 1) as f64 * t).abs() < 1e-9);
        let (_, pivot) = TREBUCHET;
        let (short, long) = ARM;
        // The long end reaches the ground when cocked, the short one does
        // not hit the cross bar.
        assert!(long > pivot && short < pivot - 63.0);
    }
}
