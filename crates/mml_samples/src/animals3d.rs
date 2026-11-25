//! More 3D puzzles in 3 mm plywood. Animals made of two flanks, the
//! full side view of the animal standing on its four legs, held apart
//! by rounded spacers that make the back; a fir tree of two crossed
//! trees and a star.

use crate::Builder;
use crate::layered::{Random, fir_with};
use crate::models3d::cut_and_finish;
use crate::puzzles3d::{THICKNESS, outline_of, vertical_extent, vertical_slot};
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
    let half = gap / 2.0 + THICKNESS + SPACER_LIP;
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
        let x = side * (gap / 2.0 + THICKNESS / 2.0);
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
    let b = Builder::new(text_language(language), prefix);
    let mut parts = vec![
        model.flank.clone(),
        Part {
            name: b.t("part.flank_2"),
            ..model.flank
        },
    ];
    parts.extend(model.spacers);
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
                .all(|p| inside(&solid, *p) && p.x.abs() > THICKNESS);
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
    let b = Builder::new(text_language(language), "fir_tree_3d");
    let tree = tree_parts([b.t("part.full"), b.t("part.short"), b.t("part.star")]);
    cut_and_finish(b, vec![tree.full, tree.short, tree.star])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> (String, String) {
        ("flank".into(), "spacer".into())
    }

    #[test]
    fn spacer_slots_meet_the_flank_slots_halfway() {
        for model in [bear_parts(names()), wolf_parts(names())] {
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
    fn the_trees_cross_at_the_same_height() {
        let tree = tree_parts(["a".into(), "b".into(), "c".into()]);
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
