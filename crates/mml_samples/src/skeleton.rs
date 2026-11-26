//! A T. rex skeleton to slot together: the spine with the skull is the
//! central part, ribs cross it from below, a pelvis carries the two legs
//! seen from the side, small arms hang at the chest, and the feet go
//! into a stand.

use crate::fit::{real, slot};
use crate::puzzles3d::{smooth_closed, vertical_extent, vertical_slot};
use crate::shapes::*;
use mml_core::geometry::{Path, Point};
use mml_core::region::offset_paths;

/// Spine curve, tail tip to the back of the skull: (x, y) of its middle.
const SPINE: [(f64, f64); 9] = [
    (14.0, 60.0),
    (42.0, 73.0),
    (70.0, 90.0),
    (95.0, 98.0),
    (125.0, 96.0),
    (155.0, 92.0),
    (172.0, 100.0),
    (182.0, 114.0),
    (192.0, 122.0),
];
/// Half thickness of the spine band along the same points.
const SPINE_HALF: [f64; 9] = [1.8, 3.0, 5.0, 7.0, 6.5, 6.0, 5.0, 4.5, 5.0];
/// Where the hips and the chest are along the spine.
pub const HIP: f64 = 95.0;
const ARMS: f64 = 165.0;
/// Ribs: x along the spine, half width and drop of each.
const RIBS: [(f64, f64, f64); 8] = [
    (110.0, 18.0, 26.0),
    (117.0, 22.0, 34.0),
    (124.0, 25.0, 40.0),
    (131.0, 26.0, 44.0),
    (138.0, 26.0, 44.0),
    (145.0, 24.0, 40.0),
    (152.0, 20.0, 33.0),
    (159.0, 15.0, 24.0),
];
/// Legs stand this far either side of the spine.
const STANCE: f64 = 17.0;

/// The skeleton's parts and where they cross, for the tests.
pub struct Skeleton {
    pub spine: Part,
    pub ribs: Vec<Part>,
    pub pelvis: Part,
    pub legs: Vec<Part>,
    pub arms: Part,
    pub stand: Part,
    /// Heights where the slots of each crossing meet, spine side.
    #[allow(dead_code)]
    pub meets: Vec<(f64, f64, f64)>,
}

/// The spine band with neural spines on top, and the skull.
fn spine_and_skull() -> (Vec<Path>, Vec<Path>) {
    // Band: both edges of the curve, offset by the local half width.
    let mut top = Vec::new();
    let mut bottom = Vec::new();
    let steps = 120;
    let curve = |t: f64| -> (Point, f64) {
        let position = t * (SPINE.len() - 1) as f64;
        let index = (position.floor() as usize).min(SPINE.len() - 2);
        let local = position - index as f64;
        let (a, b) = (SPINE[index], SPINE[index + 1]);
        let half = SPINE_HALF[index] + (SPINE_HALF[index + 1] - SPINE_HALF[index]) * local;
        (
            point(a.0 + (b.0 - a.0) * local, a.1 + (b.1 - a.1) * local),
            half,
        )
    };
    for step in 0..=steps {
        let t = step as f64 / steps as f64;
        let (at, half) = curve(t);
        top.push(at + point(0.0, half));
        bottom.push(at - point(0.0, half));
    }
    let mut band = top.clone();
    band.extend(bottom.into_iter().rev());
    let mut shapes = vec![vec![Path::new(band, true)]];
    // Neural spines: small blades on the back, tallest over the hips.
    for index in 0..22 {
        let x = 20.0 + index as f64 * 7.2;
        let (at, half) = curve((x - 14.0) / (192.0 - 14.0));
        let tall = 3.0 + 6.0 * (-((x - HIP) / 45.0).powi(2)).exp();
        shapes.push(vec![polygon(&[
            (at.x - 2.0, at.y + half - 1.0),
            (at.x + 1.5, at.y + half + tall),
            (at.x + 2.6, at.y + half - 1.0),
        ])]);
    }
    // Skull, snout to the right: cranium, muzzle, lower jaw.
    let skull = smooth_closed(
        &[
            (186.0, 108.0),
            (190.0, 132.0),
            (205.0, 137.0),
            (228.0, 130.0),
            (244.0, 121.0),
            (246.0, 113.0),
            (238.0, 109.0),
            (240.0, 101.0),
            (222.0, 98.0),
            (200.0, 99.0),
        ],
        8,
    );
    shapes.push(vec![skull]);
    let outline = union(shapes);
    // Openings of the skull: the eye, the big window in front of it,
    // the nostril, and the mouth with its teeth.
    let mut holes = vec![
        circle(point(203.0, 125.0), 4.5),
        smooth_closed(
            &[
                (212.0, 118.0),
                (224.0, 125.0),
                (232.0, 120.0),
                (222.0, 113.0),
            ],
            6,
        ),
        circle(point(240.0, 118.0), 1.6),
    ];
    let mut mouth = vec![(206.0, 108.5)];
    let mut x = 208.0;
    let mut upper = true;
    while x < 238.0 {
        mouth.push((x, if upper { 107.5 } else { 105.0 }));
        upper = !upper;
        x += 2.5;
    }
    mouth.push((240.0, 106.5));
    let mut x = 238.0;
    upper = false;
    while x > 208.0 {
        mouth.push((x, if upper { 104.0 } else { 101.5 }));
        upper = !upper;
        x -= 2.5;
    }
    holes.push(polygon(&mouth));
    (outline, holes)
}

/// A rib seen from the front, crossing the spine at its top: two curved
/// bones from a block that the spine goes through.
fn rib(low: f64, high: f64, half: f64, drop: f64, meet: f64) -> Vec<Path> {
    let bone = |side: f64| {
        Path::new(
            (0..=12)
                .map(|step| {
                    let t = step as f64 / 12.0;
                    let angle = t * std::f64::consts::PI * 0.85;
                    point(
                        side * half * angle.sin(),
                        low - 3.0 - drop * (1.0 - angle.cos()) / 1.85,
                    )
                })
                .collect(),
            false,
        )
    };
    let mut shapes = vec![vec![rect(-4.5, low - 6.0, 9.0, high - low + 8.0)]];
    for side in [-1.0, 1.0] {
        shapes.push(offset_paths(&[bone(side)], 2.0));
    }
    difference(vec![union(shapes), vertical_slot(0.0, meet, high + 10.0)])
}

/// A leg seen from the side, hip at the top: thigh, shin, the long foot
/// bones and three toes, with two tabs under the foot for the stand.
fn leg(hip_x: f64, top: f64) -> Vec<Path> {
    let t = slot();
    let joints = [
        (hip_x, top),
        (hip_x + 14.0, top - 32.0),
        (hip_x - 2.0, top - 62.0),
        (hip_x + 4.0, 8.0),
    ];
    let mut shapes = Vec::new();
    for (index, pair) in joints.windows(2).enumerate() {
        let width = [6.0, 4.5, 3.5][index];
        shapes.push(offset_paths(
            &[Path::new(
                vec![point(pair[0].0, pair[0].1), point(pair[1].0, pair[1].1)],
                false,
            )],
            width,
        ));
    }
    // Foot: toes forward and a short one back, standing on y = 0.
    let ankle = point(hip_x + 4.0, 8.0);
    shapes.push(vec![polygon(&[
        (ankle.x - 12.0, 0.0),
        (ankle.x + 26.0, 0.0),
        (ankle.x + 22.0, 5.0),
        (ankle.x + 4.0, 12.0),
        (ankle.x - 6.0, 8.0),
    ])]);
    // Tabs into the stand.
    shapes.push(vec![rect(ankle.x - 8.0, -t, 8.0, t + 0.5)]);
    shapes.push(vec![rect(ankle.x + 10.0, -t, 8.0, t + 0.5)]);
    union(shapes)
}

pub fn skeleton_parts(name: &dyn Fn(&str) -> String) -> Skeleton {
    let t = slot();
    let (outline, holes) = spine_and_skull();
    let spine_outline = outline
        .iter()
        .max_by(|a, b| a.signed_area().abs().total_cmp(&b.signed_area().abs()))
        .expect("a spine")
        .clone();
    let extent = |x: f64| vertical_extent(&spine_outline, x).expect("spine at x");
    let mut cuts = vec![outline.clone()];
    cuts.extend(holes.iter().map(|hole| vec![hole.clone()]));
    let mut meets = Vec::new();
    // Ribs: the spine's slot comes up from below, the rib's from above.
    let mut ribs = Vec::new();
    for (index, (x, half, drop)) in RIBS.iter().enumerate() {
        let (low, high) = extent(*x);
        let meet = (low + high) / 2.0;
        cuts.push(vertical_slot(*x, low - 5.0, meet));
        meets.push((*x, low, meet));
        ribs.push(Part::from_area(
            format!("{} {}", name("part.rib"), index + 1),
            rib(low, high, *half, *drop, meet),
        ));
    }
    // Pelvis across the hips: the spine goes into its top, the legs
    // into slots from its bottom.
    let (low, high) = extent(HIP);
    let meet = (low + high) / 2.0;
    cuts.push(vertical_slot(HIP, low - 5.0, meet));
    meets.push((HIP, low, meet));
    let pelvis_bottom = low - 30.0;
    let leg_top = low - 4.0;
    let leg_meet = (pelvis_bottom + leg_top) / 2.0;
    let mut pelvis = vec![vec![polygon(&[
        (-STANCE - 8.0, pelvis_bottom),
        (STANCE + 8.0, pelvis_bottom),
        (STANCE + 4.0, low - 6.0),
        (9.0, high + 2.0),
        (-9.0, high + 2.0),
        (-STANCE - 4.0, low - 6.0),
    ])]];
    pelvis.push(vertical_slot(0.0, meet, high + 10.0));
    for side in [-1.0, 1.0] {
        pelvis.push(vertical_slot(side * STANCE, pelvis_bottom - 5.0, leg_meet));
    }
    let pelvis = Part::from_area(name("part.pelvis"), difference(pelvis));
    let legs = (1..=2)
        .map(|number| {
            let leg = difference(vec![
                leg(HIP, leg_top),
                vertical_slot(HIP, leg_meet, leg_top + 10.0),
            ]);
            Part::from_area(format!("{} {number}", name("part.leg")), leg)
        })
        .collect();
    // Little arms at the chest, from below.
    let (low, high) = extent(ARMS);
    let meet = (low + high) / 2.0;
    cuts.push(vertical_slot(ARMS, low - 5.0, meet));
    meets.push((ARMS, low, meet));
    let mut arms = vec![vec![rect(-5.0, low - 8.0, 10.0, high - low + 10.0)]];
    for side in [-1.0, 1.0] {
        arms.push(offset_paths(
            &[Path::new(
                vec![
                    point(side * 3.0, low - 5.0),
                    point(side * 12.0, low - 14.0),
                    point(side * 15.0, low - 9.0),
                ],
                false,
            )],
            1.8,
        ));
    }
    arms.push(vertical_slot(0.0, meet, high + 10.0));
    let arms = Part::from_area(name("part.arms"), difference(arms));
    // Specks closed in between the neural spines would only be cut out
    // as dust.
    let spine_area: Vec<Path> = difference(cuts)
        .into_iter()
        .filter(|path| path.signed_area().abs() > 2.0)
        .collect();
    let spine = Part::from_area(name("part.spine"), spine_area);
    // The stand: a plate with the four slots the feet go into.
    let clearance = real(0.15);
    let ankle = HIP + 4.0;
    let mut stand_holes = Vec::new();
    for side in [-1.0, 1.0] {
        for from in [ankle - 8.0, ankle + 10.0] {
            stand_holes.push(rect(
                from - clearance,
                40.0 + side * STANCE - t / 2.0 - clearance,
                8.0 + 2.0 * clearance,
                t + 2.0 * clearance,
            ));
        }
    }
    let stand = Part::new(
        name("part.stand"),
        rounded_rect(HIP - 50.0, 0.0, 110.0, 80.0, 8.0),
    )
    .with_holes(stand_holes);
    Skeleton {
        spine,
        ribs,
        pelvis,
        legs,
        arms,
        stand,
        meets,
    }
}

/// The skeleton as a project.
pub fn trex_skeleton(language: &str) -> mml_core::Document {
    let mut b = crate::Builder::new(
        crate::tests_projects::text_language(language),
        "trex_skeleton",
    );
    let skeleton = skeleton_parts(&|key: &str| b.t(key));
    let mut parts = vec![
        skeleton.spine,
        skeleton.pelvis,
        skeleton.arms,
        skeleton.stand,
    ];
    parts.extend(skeleton.ribs);
    parts.extend(skeleton.legs);
    b.stock_board();
    crate::models3d::cut_then_steps(b, parts, 3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_crossing_leaves_wood_and_the_feet_reach_the_ground() {
        let skeleton = skeleton_parts(&|key: &str| key.to_owned());
        assert_eq!(skeleton.ribs.len(), RIBS.len());
        // The spine is one piece: its holes are the eye, the window in
        // front of it and the nostril (the mouth opens on the edge).
        assert_eq!(skeleton.spine.holes.len(), 3);
        for (x, low, meet) in &skeleton.meets {
            // Each slot goes halfway into a band at least 8 mm deep.
            let (_, high) = vertical_extent(&skeleton.spine.outline, x + 3.0).expect("band");
            assert!(high - low >= 8.0, "{x}: {low} {high}");
            assert!(meet - low >= 4.0);
        }
        for leg in &skeleton.legs {
            let bounds = leg.outline.bounds();
            // Tabs below the ground line, the hip high enough for the
            // pelvis.
            assert!(bounds.min.y < 0.0 && bounds.max.y > 60.0, "{bounds:?}");
        }
        assert_eq!(skeleton.stand.holes.len(), 4);
        // Everything fits a board.
        let spine = skeleton.spine.outline.bounds();
        assert!(
            spine.width() <= 236.0 && spine.height() <= 160.0,
            "{spine:?}"
        );
    }
}
