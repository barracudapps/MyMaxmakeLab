//! More 3D puzzles in 3 mm plywood. Animals made of two flanks, the
//! full side view of the animal standing on its four legs, held apart
//! by rounded spacers that make the back; a fir tree of two crossed
//! trees and a star.

use crate::Builder;
use crate::fit::slot;
use crate::layered::{Random, fir_with};
use crate::models3d::cut_and_finish;
use crate::puzzles3d::{outline_of, smooth_closed, vertical_extent, vertical_slot};
use crate::shapes::*;
use crate::tests_projects::text_language;
use mml_core::Document;
use mml_core::geometry::Path;
use mml_core::region::{clip_lines, offset_paths, region_of};

/// Height the spacers stand into the flanks and above them.
const SPACER_DEPTH: f64 = 16.0;
const SPACER_BULGE: f64 = 5.0;
/// Wood left beyond the slots at each end of a spacer.
const SPACER_LIP: f64 = 5.0;

/// Where a spacer crosses the flanks: its x on the flank, the top of the
/// flank there and the height where the slots meet.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct Spacing {
    pub x: f64,
    pub top: f64,
    pub meet: f64,
}

/// Two flanks and their spacers.
pub struct TwinModel {
    pub flank: Part,
    pub spacers: Vec<Part>,
    #[allow(dead_code)]
    pub spacings: Vec<Spacing>,
    /// Distance between the inner faces of the flanks.
    #[allow(dead_code)]
    pub gap: f64,
}

/// A spacer across the back: a rounded top `SPACER_BULGE` above the
/// flanks, two slots from its bottom edge, one for each flank.
fn spacer(gap: f64, spacing: &Spacing) -> Vec<Path> {
    let half = gap / 2.0 + slot() + SPACER_LIP;
    let bottom = spacing.top - SPACER_DEPTH;
    let shoulder = spacing.top - 2.0;
    let mut points = vec![(-half, bottom)];
    let steps = 16;
    for step in 0..=steps {
        let angle = std::f64::consts::PI * (1.0 - step as f64 / steps as f64);
        points.push((
            half * angle.cos(),
            shoulder + (SPACER_BULGE + 2.0) * angle.sin(),
        ));
    }
    points.push((half, bottom));
    let mut shapes = vec![vec![polygon(&points)]];
    for side in [-1.0, 1.0] {
        let x = side * (gap / 2.0 + slot() / 2.0);
        shapes.push(vertical_slot(x, bottom - 5.0, spacing.meet));
    }
    difference(shapes)
}

/// Builds a twin model from a side view facing left. `spacers` are the
/// fractions of the length where the spacers cross the back.
pub fn twin_model(
    side: Vec<Path>,
    gap: f64,
    spacers: &[f64],
    names: (String, String),
) -> TwinModel {
    let profile = outline_of(side);
    let bounds = profile.bounds();
    let mut slots = vec![vec![profile.clone()]];
    let mut spacings = Vec::new();
    for fraction in spacers {
        let x = bounds.min.x + bounds.width() * fraction;
        let (_, top) = vertical_extent(&profile, x).expect("a spacer over the back");
        let meet = top - SPACER_DEPTH / 2.0;
        slots.push(vertical_slot(x, meet, top + 5.0));
        spacings.push(Spacing { x, top, meet });
    }
    let flank = Part::from_area(names.0, difference(slots));
    let spacers = spacings
        .iter()
        .enumerate()
        .map(|(index, spacing)| {
            Part::from_area(format!("{} {}", names.1, index + 1), spacer(gap, spacing))
        })
        .collect();
    TwinModel {
        flank,
        spacers,
        spacings,
        gap,
    }
}

fn twin_document(language: &str, prefix: &'static str, model: TwinModel) -> Document {
    let mut b = Builder::new(text_language(language), prefix);
    let mut parts = vec![
        model.flank.clone(),
        Part {
            name: b.t("part.flank_2"),
            ..model.flank
        },
    ];
    parts.extend(model.spacers);
    b.stock_board();
    cut_and_finish(b, parts)
}

pub fn bear_parts(names: (String, String)) -> TwinModel {
    let side = crate::art::picked(crate::art::FOREST_ANIMALS, crate::art::BEAR, 105.0, false);
    twin_model(side, 30.0, &[0.38, 0.55, 0.72], names)
}

/// Bear in two flanks and three spacers.
pub fn bear_3d(language: &str) -> Document {
    let b = Builder::new(text_language(language), "bear_3d");
    let model = bear_parts((b.t("part.flank_1"), b.t("part.spacer")));
    twin_document(language, "bear_3d", model)
}

pub fn wolf_parts(names: (String, String)) -> TwinModel {
    let side = crate::art::picked(crate::art::FOREST_ANIMALS, crate::art::WOLF, 110.0, false);
    twin_model(side, 20.0, &[0.27, 0.42, 0.58], names)
}

/// Wolf in two flanks and three spacers.
pub fn wolf_3d(language: &str) -> Document {
    let b = Builder::new(text_language(language), "wolf_3d");
    let model = wolf_parts((b.t("part.flank_1"), b.t("part.spacer")));
    twin_document(language, "wolf_3d", model)
}

pub fn rhino_parts(names: (String, String)) -> TwinModel {
    let side = crate::art::picked(crate::art::ANIMALS_B, crate::art::RHINO, 95.0, false);
    twin_model(side, 26.0, &[0.3, 0.48, 0.65], names)
}

/// Rhinoceros in two flanks and three spacers.
pub fn rhino_3d(language: &str) -> Document {
    let b = Builder::new(text_language(language), "rhino_3d");
    let model = rhino_parts((b.t("part.flank_1"), b.t("part.spacer")));
    twin_document(language, "rhino_3d", model)
}

pub fn turtle_parts(names: (String, String)) -> TwinModel {
    let side = crate::art::picked(crate::art::ANIMALS_B, crate::art::TURTLE, 72.0, false);
    twin_model(side, 40.0, &[0.32, 0.47, 0.62], names)
}

/// Tortoise in two flanks and three spacers across the shell.
pub fn turtle_3d(language: &str) -> Document {
    let b = Builder::new(text_language(language), "turtle_3d");
    let model = turtle_parts((b.t("part.flank_1"), b.t("part.spacer")));
    twin_document(language, "turtle_3d", model)
}

/// Length of the 3D rocket.
pub const ROCKET: f64 = 165.0;

/// The rocket: the full body with two fins, and the lower stage with
/// the two other fins, crossed.
pub struct Rocket {
    pub body: Part,
    pub fins: Part,
    #[allow(dead_code)]
    pub meet: f64,
    #[allow(dead_code)]
    pub cap: f64,
}

pub fn rocket_parts(names: [String; 2]) -> Rocket {
    let [body_name, fins_name] = names;
    let (outline, window) = crate::layered2::rocket(point(0.0, ROCKET), ROCKET);
    // Raise it so the fin tips stand on y = 0.
    let low = outline.bounds().min.y;
    let lift = mml_core::geometry::Transform::translate(0.0, -low);
    let outline = outline.transformed(&lift);
    let window = window.transformed(&lift);
    let base = -low;
    let meet = base + ROCKET * 0.22;
    let cap = base + ROCKET * 0.48;
    let body = difference(vec![vec![outline.clone()], vertical_slot(0.0, -5.0, meet)]);
    let fins = difference(vec![
        vec![outline.clone()],
        vec![rect(-60.0, cap, 120.0, ROCKET * 2.0)],
        vertical_slot(0.0, meet, cap + 5.0),
    ]);
    // Rivets and panel lines engraved on the body.
    let mut lines = Vec::new();
    for fraction in [0.55, 0.7] {
        let y = base + ROCKET * fraction;
        lines.push(Path::new(vec![point(-30.0, y), point(30.0, y)], false));
    }
    let area = region_of(std::slice::from_ref(&outline));
    let engraving = clip_lines(&lines, &area);
    let second = circle(window.bounds().center() - point(0.0, ROCKET * 0.13), 4.0);
    Rocket {
        body: Part::from_area(body_name, body)
            .with_holes(vec![window, second])
            .with_engraving(engraving),
        fins: Part::from_area(fins_name, fins),
        meet,
        cap,
    }
}

/// 3D rocket: the body slides down onto the lower stage, crossed, and
/// stands on its four fins.
pub fn rocket_3d(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "rocket_3d");
    let rocket = rocket_parts([b.t("part.body"), b.t("part.fins")]);
    b.stock_board();
    cut_and_finish(b, vec![rocket.body, rocket.fins])
}

/// Airplane sizes: fuselage length, wing span and root chord.
pub const PLANE: (f64, f64, f64) = (190.0, 200.0, 38.0);

/// The airplane: fuselage, wing, tail plane and landing gear.
pub struct Plane {
    pub fuselage: Part,
    pub wing: Part,
    pub tail: Part,
    pub gear: Part,
    /// The holes the wing and the tail plane go through: x from, x to.
    #[allow(dead_code)]
    pub wing_hole: (f64, f64),
    #[allow(dead_code)]
    pub tail_hole: (f64, f64),
}

/// A wing seen from above, `span` wide, chord `root` at the middle and
/// `tip` at the ends, swept back by `sweep`; leading edge at y = 0.
fn wing_shape(span: f64, root: f64, tip: f64, sweep: f64) -> Path {
    let half = span / 2.0;
    polygon(&[
        (-half, -sweep),
        (0.0, 0.0),
        (half, -sweep),
        (half, -sweep - tip),
        (0.0, -root),
        (-half, -sweep - tip),
    ])
}

pub fn plane_parts(names: [String; 4]) -> Plane {
    let [fuselage_name, wing_name, tail_name, gear_name] = names;
    let (length, span, root) = PLANE;
    // Side view, nose to the right, belly at y = 0.
    let body = smooth_closed(
        &[
            (0.0, 18.0),
            (20.0, 12.0),
            (60.0, 4.0),
            (110.0, 0.0),
            (165.0, 3.0),
            (length, 13.0),
            (length - 2.0, 22.0),
            (165.0, 31.0),
            (140.0, 33.0),
            (118.0, 40.0),
            (100.0, 33.0),
            (40.0, 26.0),
            (10.0, 24.0),
        ],
        10,
    );
    let fin = polygon(&[(4.0, 22.0), (2.0, 58.0), (14.0, 58.0), (40.0, 25.0)]);
    let outline = outline_of(union(vec![vec![body], vec![fin]]));
    let wing_hole = (110.0 - root / 2.0, 110.0 + root / 2.0);
    let tail_hole = (8.0, 30.0);
    let hole =
        |from: f64, to: f64, y: f64| rect(from - 0.25, y - slot() / 2.0, to - from + 0.5, slot());
    let wing_y = 12.0;
    let tail_y = 21.0;
    // The landing gear comes up into the belly under the wing.
    let gear_x = 100.0;
    let (belly, _) = vertical_extent(&outline, gear_x).expect("a belly");
    let rise = 10.0;
    let meet = belly + rise / 2.0;
    let fuselage = difference(vec![
        vec![outline.clone()],
        vec![hole(wing_hole.0, wing_hole.1, wing_y)],
        vec![hole(tail_hole.0, tail_hole.1, tail_y)],
        vertical_slot(gear_x, belly - 5.0, meet),
    ]);
    let mut lines = vec![
        // Cockpit windows and the door.
        rect(150.0, 22.0, 9.0, 6.0),
        rect(162.0, 21.0, 8.0, 6.0),
        rect(60.0, 8.0, 7.0, 13.0),
    ];
    for index in 0..6 {
        lines.push(circle(point(75.0 + index as f64 * 12.0, 22.0), 2.0));
    }
    let wing = wing_shape(span, root, 24.0, 10.0);
    let roundels = vec![
        circle(point(-70.0, -16.0), 7.0),
        circle(point(-70.0, -16.0), 3.5),
        circle(point(70.0, -16.0), 7.0),
        circle(point(70.0, -16.0), 3.5),
    ];
    let tail = wing_shape(72.0, 22.0, 13.0, 6.0);
    // Gear seen from the front: a strut from the belly to each wheel.
    let top = belly + rise;
    let ground = belly - 24.0;
    let gear = union(vec![
        vec![rect(-10.0, belly - 4.0, 20.0, top - belly + 4.0)],
        offset_paths(
            &[Path::new(
                vec![point(-6.0, belly - 2.0), point(-26.0, ground + 6.0)],
                false,
            )],
            2.5,
        ),
        offset_paths(
            &[Path::new(
                vec![point(6.0, belly - 2.0), point(26.0, ground + 6.0)],
                false,
            )],
            2.5,
        ),
        vec![circle(point(-26.0, ground + 6.0), 6.0)],
        vec![circle(point(26.0, ground + 6.0), 6.0)],
    ]);
    let gear = difference(vec![gear, vertical_slot(0.0, meet, top + 5.0)]);
    Plane {
        fuselage: Part::from_area(fuselage_name, fuselage).with_engraving(lines),
        wing: Part::new(wing_name, wing).with_engraving(roundels),
        tail: Part::new(tail_name, tail),
        gear: Part::from_area(gear_name, gear),
        wing_hole,
        tail_hole,
    }
}

/// 3D airplane: the wing and the tail plane slide through the fuselage,
/// the landing gear slots in under the belly.
pub fn airplane_3d(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "airplane_3d");
    let plane = plane_parts([
        b.t("part.fuselage"),
        b.t("part.wing"),
        b.t("part.tail"),
        b.t("part.gear"),
    ]);
    b.stock_board();
    cut_and_finish(b, vec![plane.fuselage, plane.wing, plane.tail, plane.gear])
}

/// Height of the 3D fir tree.
pub const TREE: f64 = 160.0;
/// Where the two trees meet, and where the second one stops.
pub const TREE_MEET: f64 = 0.62;
pub const TREE_CAP: f64 = 0.72;

/// The two crossed trees and the star of the 3D fir tree.
pub struct Tree {
    pub full: Part,
    pub short: Part,
    pub star: Part,
}

pub fn tree_parts(names: [String; 3]) -> Tree {
    let [full_name, short_name, star_name] = names;
    let base = 12.0;
    let profile = union(vec![
        vec![fir_with(point(0.0, base - 2.0), TREE - base + 2.0, 0.55)],
        // A wide foot so it stands on four.
        vec![rect(-18.0, 0.0, 36.0, base)],
    ]);
    let meet = TREE * TREE_MEET;
    let cap = TREE * TREE_CAP;
    // Baubles as round windows, garlands engraved across the tiers.
    let mut random = Random::new(17);
    let decorate = |random: &mut Random, outline: &[Path], top: f64| {
        let area = region_of(outline);
        // Holes only where the wood goes round them, clear of the slot.
        let solid = offset_paths(outline, -1.8);
        let mut holes = Vec::new();
        for (fraction, x) in [
            (0.2, -18.0),
            (0.27, 16.0),
            (0.36, -10.0),
            (0.44, 12.0),
            (0.52, -8.0),
            (0.6, 7.0),
            (0.68, -5.0),
        ] {
            let y = TREE * fraction;
            let hole = circle(point(x, y), random.between(2.2, 3.0));
            let clear = hole
                .points
                .iter()
                .all(|p| inside(&solid, *p) && p.x.abs() > slot());
            if y < top - 8.0 && clear {
                holes.push(hole);
            }
        }
        let mut garlands = Vec::new();
        for (from, to) in [(0.2, 0.32), (0.38, 0.48), (0.55, 0.62)] {
            if TREE * to < top - 4.0 {
                garlands.push(Path::new(
                    (0..=20)
                        .map(|step| {
                            let t = step as f64 / 20.0;
                            let x = -40.0 + 80.0 * t;
                            let y = TREE * (from + (to - from) * t)
                                - 3.0 * (t * std::f64::consts::TAU).sin();
                            point(x, y)
                        })
                        .collect(),
                    false,
                ));
            }
        }
        (holes, clip_lines(&garlands, &area))
    };
    // The full tree takes the slot from below, the short one from above.
    let full_area = difference(vec![profile.clone(), vertical_slot(0.0, -5.0, meet)]);
    let short_area = difference(vec![
        profile.clone(),
        vec![rect(-60.0, cap, 120.0, TREE)],
        vertical_slot(0.0, meet, cap + 5.0),
    ]);
    let (holes, garlands) = decorate(&mut random, &profile, TREE);
    let full = Part::from_area(full_name, full_area)
        .with_holes(holes)
        .with_engraving(garlands);
    let (holes, garlands) = decorate(&mut random, &profile, cap);
    let short = Part::from_area(short_name, short_area)
        .with_holes(holes)
        .with_engraving(garlands);
    // The star sits on the tip: its slot takes the last 9 mm of it.
    let centre = point(0.0, TREE + 1.0);
    let star = difference(vec![
        vec![Path::new(
            (0..10)
                .map(|index| {
                    let radius = if index % 2 == 0 { 17.0 } else { 7.5 };
                    polar(centre, radius, 90.0 + index as f64 * 36.0)
                })
                .collect(),
            true,
        )],
        vertical_slot(0.0, TREE - 12.0, TREE),
    ]);
    Tree {
        full,
        short,
        star: Part::from_area(star_name, star),
    }
}

/// Whether `p` lies inside `area` (even-odd over its paths).
fn inside(area: &[Path], p: mml_core::geometry::Point) -> bool {
    let mut inside = false;
    for path in area {
        let count = path.points.len();
        for index in 0..count {
            let (a, b) = (path.points[index], path.points[(index + 1) % count]);
            if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (b.x - a.x) * (p.y - a.y) / (b.y - a.y) {
                inside = !inside;
            }
        }
    }
    inside
}

/// 3D fir tree: two trees crossed, one with its slot from below and the
/// other, shorter, from above, and a star on the tip.
pub fn fir_tree_3d(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "fir_tree_3d");
    let tree = tree_parts([b.t("part.full"), b.t("part.short"), b.t("part.star")]);
    b.stock_board();
    cut_and_finish(b, vec![tree.full, tree.short, tree.star])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::puzzles3d::THICKNESS;

    fn names() -> (String, String) {
        ("flank".into(), "spacer".into())
    }

    #[test]
    fn spacer_slots_meet_the_flank_slots_halfway() {
        for model in [
            bear_parts(names()),
            wolf_parts(names()),
            rhino_parts(names()),
            turtle_parts(names()),
        ] {
            assert_eq!(model.spacers.len(), model.spacings.len());
            for (spacing, part) in model.spacings.iter().zip(&model.spacers) {
                let bottom = part.outline.bounds().min.y;
                // The spacer reaches as far below the back as the flank
                // slot goes, twice over.
                assert!((spacing.top - bottom - SPACER_DEPTH).abs() < 0.01);
                assert!((spacing.top - spacing.meet - SPACER_DEPTH / 2.0).abs() < 1e-9);
                // The slot in the flank leaves wood below it.
                let (low, _) = vertical_extent(&model.flank.outline, spacing.x - 3.0)
                    .expect("flank under the spacer");
                assert!(spacing.meet - low > 15.0, "{spacing:?}");
            }
            // The spacer is one piece (slots open from its bottom edge).
            assert!(model.spacers.iter().all(|part| part.holes.is_empty()));
            assert!(model.gap > 10.0);
        }
    }

    #[test]
    fn plane_wings_pass_through_their_holes() {
        let plane = plane_parts(["a".into(), "b".into(), "c".into(), "d".into()]);
        let (_, _, root) = PLANE;
        // Each chord of the wing fits the hole it slides through.
        let wing = plane.wing.outline.bounds();
        assert!(wing.height() <= plane.wing_hole.1 - plane.wing_hole.0 + 1e-9);
        assert!((wing.height() - root).abs() < 1e-9);
        let tail = plane.tail.outline.bounds();
        assert!(tail.height() <= plane.tail_hole.1 - plane.tail_hole.0 + 1e-9);
        // Holes are inside the fuselage, wood above and below them.
        assert_eq!(plane.fuselage.holes.len(), 2);
        for hole in &plane.fuselage.holes {
            let bounds = hole.bounds();
            for x in [bounds.min.x + 0.5, bounds.max.x - 0.5] {
                let (low, high) =
                    vertical_extent(&plane.fuselage.outline, x).expect("fuselage over the hole");
                assert!(bounds.min.y - low >= 2.5, "{x}: {low} {bounds:?}");
                assert!(high - bounds.max.y >= 2.5, "{x}: {high} {bounds:?}");
            }
        }
        // The gear and the wing fit a board.
        assert!(wing.width() <= 250.0);
        assert!(plane.gear.holes.is_empty());
    }

    #[test]
    fn rocket_stages_cross_and_stand_on_their_fins() {
        let rocket = rocket_parts(["a".into(), "b".into()]);
        let body = rocket.body.outline.bounds();
        let fins = rocket.fins.outline.bounds();
        assert!(body.min.y.abs() < 1e-6 && fins.min.y.abs() < 1e-6);
        assert!((fins.max.y - rocket.cap).abs() < 0.5);
        assert!(rocket.meet < rocket.cap);
        // Wood either side of the stage slot.
        let (low, high) = vertical_extent(&rocket.fins.outline, THICKNESS / 2.0 + 4.0)
            .expect("wood beside the slot");
        assert!(low < rocket.meet && high > rocket.cap - 1.0);
    }

    #[test]
    fn the_trees_cross_at_the_same_height() {
        let tree = tree_parts(["a".into(), "b".into(), "c".into()]);
        // Each tree is one piece: its only holes are the baubles.
        for part in [&tree.full, &tree.short] {
            for hole in &part.holes {
                assert!(hole.bounds().width() < 7.0, "{:?}", hole.bounds());
            }
        }
        let full = tree.full.outline.bounds();
        let short = tree.short.outline.bounds();
        assert!((full.max.y - TREE).abs() < 0.5);
        assert!((short.max.y - TREE * TREE_CAP).abs() < 0.5);
        assert!(
            full.min.y.abs() < 1e-6 && short.min.y.abs() < 1e-6,
            "{full:?} {short:?}"
        );
        // The short tree's prongs either side of its slot stay at least
        // 3 mm wide.
        let (low, high) = vertical_extent(&tree.short.outline, THICKNESS / 2.0 + 3.0)
            .expect("wood beside the slot");
        assert!(high > TREE * TREE_CAP - 1.0 && low < 1.0);
    }
}
