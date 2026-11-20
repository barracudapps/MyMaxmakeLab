//! Random ornaments by cultural style: friezes and tilings built from
//! motifs typical of each tradition (knotwork and runes, mud-cloth
//! signs, stepped frets, meanders, waves...). A seed makes every
//! variation reproducible. Output is plain vector paths, ready for a
//! laser line, a V-bit or a pen.

use crate::geometry::{Path, Point};
use serde::{Deserialize, Serialize};
use std::f64::consts::{PI, TAU};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrnamentStyle {
    Viking,
    Celtic,
    African,
    Maya,
    Greek,
    Japanese,
    ArtDeco,
    Polynesian,
    Islamic,
}

impl OrnamentStyle {
    pub const ALL: [OrnamentStyle; 9] = [
        Self::Viking,
        Self::Celtic,
        Self::African,
        Self::Maya,
        Self::Greek,
        Self::Japanese,
        Self::ArtDeco,
        Self::Polynesian,
        Self::Islamic,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::Viking => "ornament.viking",
            Self::Celtic => "ornament.celtic",
            Self::African => "ornament.african",
            Self::Maya => "ornament.maya",
            Self::Greek => "ornament.greek",
            Self::Japanese => "ornament.japanese",
            Self::ArtDeco => "ornament.art_deco",
            Self::Polynesian => "ornament.polynesian",
            Self::Islamic => "ornament.islamic",
        }
    }

    fn motifs(self) -> &'static [Motif] {
        use Motif::*;
        match self {
            Self::Viking => &[Plait, Runes, Plait, ShieldKnots, Chevrons],
            Self::Celtic => &[Plait, Triskeles, Spirals, Plait],
            Self::African => &[
                Zigzag,
                Diamonds,
                Crosses,
                Dots,
                NestedSquares,
                Chevrons,
                Triangles,
            ],
            Self::Maya => &[SteppedFret, SteppedDiamonds, SteppedPyramids, Zigzag],
            Self::Greek => &[Meander, RunningDog, Meander, Dots],
            Self::Japanese => &[Seigaiha, Asanoha, Shippo],
            Self::ArtDeco => &[Fans, SteppedChevrons, Sunburst, NestedSquares],
            Self::Polynesian => &[SharkTeeth, Koru, Spearheads, SharkTeeth],
            Self::Islamic => &[Stars, Stars, Lattice],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrnamentLayout {
    /// Rows of different motifs, separated by lines.
    Bands,
    /// One motif repeated over the whole area.
    Tiles,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OrnamentParams {
    pub style: OrnamentStyle,
    pub layout: OrnamentLayout,
    pub width: f64,
    pub height: f64,
    /// Size of one motif, mm.
    pub cell: f64,
    pub seed: u64,
}

impl Default for OrnamentParams {
    fn default() -> Self {
        Self {
            style: OrnamentStyle::Viking,
            layout: OrnamentLayout::Bands,
            width: 120.0,
            height: 60.0,
            cell: 12.0,
            seed: 1,
        }
    }
}

/// SplitMix64: tiny, fast and identical on every platform.
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, count: usize) -> usize {
        (self.next() % count.max(1) as u64) as usize
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Motif {
    Plait,
    Runes,
    ShieldKnots,
    Chevrons,
    Triskeles,
    Spirals,
    Zigzag,
    Diamonds,
    Crosses,
    Dots,
    NestedSquares,
    Triangles,
    SteppedFret,
    SteppedDiamonds,
    SteppedPyramids,
    Meander,
    RunningDog,
    Seigaiha,
    Asanoha,
    Shippo,
    Fans,
    SteppedChevrons,
    Sunburst,
    SharkTeeth,
    Koru,
    Spearheads,
    Stars,
    Lattice,
}

/// Paths of the ornament, lower-left corner at the origin.
pub fn generate(params: &OrnamentParams) -> Vec<Path> {
    let mut random = Random(params.seed ^ 0x005E_ED0F_0A11);
    let motifs = params.style.motifs();
    let cell = params.cell.max(1.0);
    let (width, height) = (params.width.max(cell), params.height.max(cell));
    let mut paths = Vec::new();
    match params.layout {
        OrnamentLayout::Tiles => {
            let motif = motifs[random.below(motifs.len())];
            let rows = (height / cell).floor().max(1.0) as usize;
            for row in 0..rows {
                band(
                    &mut paths,
                    motif,
                    &mut random,
                    0.0,
                    row as f64 * cell,
                    width,
                    cell,
                    cell,
                );
            }
        }
        OrnamentLayout::Bands => {
            let mut y = 0.0;
            let mut previous = None;
            while y + cell * 0.5 <= height {
                let band_height = (cell * (0.7 + 0.6 * random.unit())).min(height - y);
                let mut motif = motifs[random.below(motifs.len())];
                if Some(motif) == previous && motifs.len() > 1 {
                    motif = motifs
                        [(motifs.iter().position(|m| *m == motif).unwrap() + 1) % motifs.len()];
                }
                previous = Some(motif);
                let margin = band_height * 0.08;
                band(
                    &mut paths,
                    motif,
                    &mut random,
                    0.0,
                    y + margin,
                    width,
                    band_height - 2.0 * margin,
                    cell,
                );
                y += band_height;
                if y < height - 1e-6 {
                    paths.push(line(&[(0.0, y), (width, y)]));
                }
            }
            // Frame around the frieze.
            paths.push(Path::new(
                vec![
                    Point::new(0.0, 0.0),
                    Point::new(width, 0.0),
                    Point::new(width, y),
                    Point::new(0.0, y),
                ],
                true,
            ));
        }
    }
    // Ribbon edges and spirals may overshoot the area by a hair.
    for point in paths.iter_mut().flat_map(|path| path.points.iter_mut()) {
        point.x = point.x.clamp(0.0, width);
        point.y = point.y.clamp(0.0, height);
    }
    paths.retain(|path| path.points.len() >= 2);
    paths
}

fn line(points: &[(f64, f64)]) -> Path {
    Path::new(
        points.iter().map(|&(x, y)| Point::new(x, y)).collect(),
        false,
    )
}

fn closed(points: Vec<Point>) -> Path {
    Path::new(points, true)
}

fn circle(centre: Point, radius: f64) -> Path {
    let count = ((radius * 6.0).ceil() as usize).clamp(12, 72);
    closed(
        (0..count)
            .map(|index| {
                let angle = TAU * index as f64 / count as f64;
                Point::new(
                    centre.x + radius * angle.cos(),
                    centre.y + radius * angle.sin(),
                )
            })
            .collect(),
    )
}

fn arc(centre: Point, radius: f64, from: f64, to: f64) -> Path {
    let count = (((to - from).abs() * radius * 2.0).ceil() as usize).clamp(6, 96);
    Path::new(
        (0..=count)
            .map(|index| {
                let angle = from + (to - from) * index as f64 / count as f64;
                Point::new(
                    centre.x + radius * angle.cos(),
                    centre.y + radius * angle.sin(),
                )
            })
            .collect(),
        false,
    )
}

/// Archimedean spiral of `turns` from the centre out to `radius`.
fn spiral(centre: Point, radius: f64, turns: f64, start: f64, clockwise: bool) -> Path {
    let count = (turns * 48.0).ceil() as usize;
    let sign = if clockwise { -1.0 } else { 1.0 };
    Path::new(
        (0..=count)
            .map(|index| {
                let t = index as f64 / count as f64;
                let angle = start + sign * t * turns * TAU;
                Point::new(
                    centre.x + radius * t * angle.cos(),
                    centre.y + radius * t * angle.sin(),
                )
            })
            .collect(),
        false,
    )
}

/// One band of `motif` filling `[x, x + width] x [y, y + height]`.
#[allow(clippy::too_many_arguments)]
fn band(
    paths: &mut Vec<Path>,
    motif: Motif,
    random: &mut Random,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    cell: f64,
) {
    let size = height.min(cell * 1.4).max(1.0);
    let count = (width / size).floor().max(1.0) as usize;
    // Cells stretched to fill the width exactly.
    let step = width / count as f64;
    let at = |column: usize| x + column as f64 * step;
    match motif {
        Motif::Plait => plait(paths, x, y, width, height, (step * 1.2).max(height * 0.9)),
        Motif::Runes => {
            for column in 0..count {
                let glyph = RUNES[random.below(RUNES.len())];
                let (cx, glyph_width) = (at(column) + step / 2.0, height * 0.45);
                for stroke in glyph {
                    paths.push(Path::new(
                        stroke
                            .iter()
                            .map(|&(u, v)| {
                                Point::new(
                                    cx + (u - 0.5) * glyph_width,
                                    y + height * (0.1 + 0.8 * v),
                                )
                            })
                            .collect(),
                        false,
                    ));
                }
            }
        }
        Motif::ShieldKnots => {
            for column in 0..count {
                let centre = Point::new(at(column) + step / 2.0, y + height / 2.0);
                let radius = height.min(step) * 0.42;
                paths.push(circle(centre, radius));
                // Four interlaced loops (a simple valknut-like knot).
                for quarter in 0..4 {
                    let angle = quarter as f64 * PI / 2.0 + PI / 4.0;
                    let loop_centre = Point::new(
                        centre.x + radius * 0.45 * angle.cos(),
                        centre.y + radius * 0.45 * angle.sin(),
                    );
                    paths.push(circle(loop_centre, radius * 0.38));
                }
            }
        }
        Motif::Chevrons => {
            let rows = 2 + random.below(2);
            for row in 0..rows {
                let base = y + height * (row as f64 + 0.5) / rows as f64 - height * 0.15;
                let mut points = Vec::new();
                for column in 0..=count * 2 {
                    let up = column % 2 == 1;
                    points.push((
                        x + column as f64 * step / 2.0,
                        base + if up {
                            height * 0.3 / rows as f64 * 1.6
                        } else {
                            0.0
                        },
                    ));
                }
                paths.push(line(&points));
            }
        }
        Motif::Triskeles => {
            for column in 0..count {
                let centre = Point::new(at(column) + step / 2.0, y + height / 2.0);
                let radius = height.min(step) * 0.45;
                paths.push(circle(centre, radius));
                for arm in 0..3 {
                    let angle = arm as f64 * TAU / 3.0;
                    let arm_centre = Point::new(
                        centre.x + radius * 0.42 * angle.cos(),
                        centre.y + radius * 0.42 * angle.sin(),
                    );
                    paths.push(spiral(arm_centre, radius * 0.42, 1.6, angle + PI, false));
                }
            }
        }
        Motif::Spirals => {
            for column in 0..count {
                let centre = Point::new(at(column) + step / 2.0, y + height / 2.0);
                let clockwise = column % 2 == 0;
                paths.push(spiral(centre, height.min(step) * 0.45, 2.5, 0.0, clockwise));
            }
        }
        Motif::Zigzag => {
            let lines = 1 + random.below(3);
            for index in 0..lines {
                let offset = height * index as f64 / lines as f64 * 0.6;
                let mut points = Vec::new();
                for column in 0..=count * 2 {
                    let top = column % 2 == 1;
                    points.push((
                        x + column as f64 * step / 2.0,
                        y + offset
                            + if top {
                                height * 0.4 + height * 0.2 / lines as f64
                            } else {
                                height * 0.1
                            },
                    ));
                }
                paths.push(line(&points));
            }
        }
        Motif::Diamonds => {
            let nested = 1 + random.below(3);
            for column in 0..count {
                let centre = Point::new(at(column) + step / 2.0, y + height / 2.0);
                for level in 0..nested {
                    let scale = 1.0 - level as f64 / (nested as f64 + 0.5);
                    let (half_width, half_height) = (step * 0.48 * scale, height * 0.48 * scale);
                    paths.push(closed(vec![
                        Point::new(centre.x - half_width, centre.y),
                        Point::new(centre.x, centre.y - half_height),
                        Point::new(centre.x + half_width, centre.y),
                        Point::new(centre.x, centre.y + half_height),
                    ]));
                }
            }
        }
        Motif::Crosses => {
            for column in 0..count {
                let centre = Point::new(at(column) + step / 2.0, y + height / 2.0);
                let reach = height.min(step) * 0.35;
                if column % 2 == 0 {
                    paths.push(line(&[
                        (centre.x - reach, centre.y - reach),
                        (centre.x + reach, centre.y + reach),
                    ]));
                    paths.push(line(&[
                        (centre.x - reach, centre.y + reach),
                        (centre.x + reach, centre.y - reach),
                    ]));
                } else {
                    paths.push(line(&[
                        (centre.x - reach, centre.y),
                        (centre.x + reach, centre.y),
                    ]));
                    paths.push(line(&[
                        (centre.x, centre.y - reach),
                        (centre.x, centre.y + reach),
                    ]));
                }
            }
        }
        Motif::Dots => {
            let rows = 1 + random.below(2);
            for row in 0..rows {
                let cy = y + height * (row as f64 + 0.5) / rows as f64;
                for column in 0..count * 2 {
                    let cx = x + (column as f64 + 0.5) * step / 2.0;
                    paths.push(circle(
                        Point::new(cx, cy),
                        (height / rows as f64).min(step / 2.0) * 0.18,
                    ));
                }
            }
        }
        Motif::NestedSquares => {
            let levels = 2 + random.below(2);
            for column in 0..count {
                let centre = Point::new(at(column) + step / 2.0, y + height / 2.0);
                for level in 0..levels {
                    let half =
                        height.min(step) * 0.46 * (1.0 - level as f64 / (levels as f64 + 0.3));
                    paths.push(closed(vec![
                        Point::new(centre.x - half, centre.y - half),
                        Point::new(centre.x + half, centre.y - half),
                        Point::new(centre.x + half, centre.y + half),
                        Point::new(centre.x - half, centre.y + half),
                    ]));
                }
            }
        }
        Motif::Triangles | Motif::SharkTeeth => {
            let both = motif == Motif::SharkTeeth || random.below(2) == 0;
            for column in 0..count * 2 {
                let left = x + column as f64 * step / 2.0;
                let right = left + step / 2.0;
                paths.push(closed(vec![
                    Point::new(left, y),
                    Point::new(right, y),
                    Point::new(
                        (left + right) / 2.0,
                        y + height * if both { 0.48 } else { 0.95 },
                    ),
                ]));
                if both {
                    paths.push(closed(vec![
                        Point::new(left, y + height),
                        Point::new(right, y + height),
                        Point::new((left + right) / 2.0, y + height * 0.52),
                    ]));
                }
            }
        }
        Motif::SteppedFret => {
            // Xicalcoliuhqui: a stepped wave ending in a square hook.
            for column in 0..count {
                let (left, w, h) = (at(column), step, height);
                let s = |u: f64, v: f64| (left + u * w, y + v * h);
                paths.push(line(&[
                    s(0.0, 0.0),
                    s(0.2, 0.0),
                    s(0.2, 0.2),
                    s(0.4, 0.2),
                    s(0.4, 0.4),
                    s(0.6, 0.4),
                    s(0.6, 1.0),
                    s(0.95, 1.0),
                    s(0.95, 0.6),
                    s(0.75, 0.6),
                    s(0.75, 0.8),
                    s(0.85, 0.8),
                ]));
                paths.push(line(&[s(0.6, 0.0), s(1.0, 0.0)]));
            }
        }
        Motif::SteppedDiamonds => {
            for column in 0..count {
                let centre = Point::new(at(column) + step / 2.0, y + height / 2.0);
                let steps = 3 + random.below(2);
                let (half_width, half_height) = (step * 0.48, height * 0.48);
                // Staircase from the right tip to the top tip, mirrored
                // into the four quarters.
                let mut quarter: Vec<(f64, f64)> = Vec::new();
                for index in 0..steps {
                    let (t0, t1) = (
                        index as f64 / steps as f64,
                        (index + 1) as f64 / steps as f64,
                    );
                    quarter.push((1.0 - t0, t0));
                    quarter.push((1.0 - t0, t1));
                }
                quarter.push((0.0, 1.0));
                let reversed: Vec<(f64, f64)> = quarter.iter().rev().copied().collect();
                let mut points = Vec::new();
                for (sx, sy, list) in [
                    (1.0, 1.0, &quarter),
                    (-1.0, 1.0, &reversed),
                    (-1.0, -1.0, &quarter),
                    (1.0, -1.0, &reversed),
                ] {
                    for &(u, v) in list.iter().skip(usize::from(!points.is_empty())) {
                        points.push(Point::new(
                            centre.x + sx * u * half_width,
                            centre.y + sy * v * half_height,
                        ));
                    }
                }
                points.pop();
                paths.push(closed(points));
            }
        }
        Motif::SteppedPyramids => {
            let steps = 3 + random.below(3);
            let mut points = Vec::new();
            for column in 0..count {
                let left = at(column);
                for index in 0..steps {
                    let u = index as f64 / (2 * steps) as f64;
                    let v = index as f64 / steps as f64;
                    points.push((left + u * step, y + v * height));
                    points.push((left + (u + 0.5 / steps as f64) * step, y + v * height));
                }
                for index in (0..steps).rev() {
                    let u = 1.0 - index as f64 / (2 * steps) as f64;
                    let v = index as f64 / steps as f64;
                    points.push((left + (u - 0.5 / steps as f64) * step, y + v * height));
                    points.push((left + u * step, y + v * height));
                }
            }
            paths.push(line(&points));
        }
        Motif::Meander => {
            // Greek key: one continuous line, one key per cell.
            let mut points = Vec::new();
            for column in 0..count {
                let (left, w, h) = (at(column), step, height);
                let s = |u: f64, v: f64| (left + u * w, y + v * h);
                points.extend([
                    s(0.0, 0.0),
                    s(0.0, 1.0),
                    s(0.8, 1.0),
                    s(0.8, 0.25),
                    s(0.4, 0.25),
                    s(0.4, 0.65),
                    s(0.6, 0.65),
                    s(0.6, 0.45),
                ]);
                paths.push(line(&[s(0.2, 0.0), s(1.0, 0.0)]));
                paths.push(line(&[s(0.2, 0.0), s(0.2, 0.8)]));
            }
            paths.push(line(&points));
        }
        Motif::RunningDog => {
            // Vitruvian scroll: waves curling into spirals.
            for column in 0..count {
                let centre = Point::new(at(column) + step * 0.55, y + height * 0.55);
                paths.push(spiral(
                    centre,
                    height.min(step) * 0.42,
                    1.25,
                    -PI / 2.0,
                    false,
                ));
                paths.push(line(&[
                    (at(column), y + height * 0.12),
                    (centre.x, y + height * 0.12),
                ]));
            }
        }
        Motif::Seigaiha => {
            let radius = step * 0.5;
            let rows = ((height / (radius * 0.5)).floor() as usize).max(1);
            for row in 0..rows {
                let cy = y + row as f64 * radius * 0.5;
                let shift = if row % 2 == 0 { 0.0 } else { radius };
                for column in 0..=count {
                    let cx = x + shift + column as f64 * step;
                    if cx - radius < x + width + 1e-6 {
                        for ring in 1..=3 {
                            let r = radius * ring as f64 / 3.0;
                            let mut path = arc(Point::new(cx, cy), r, 0.0, PI);
                            path.points.retain(|p| {
                                p.x >= x - 1e-6
                                    && p.x <= x + width + 1e-6
                                    && p.y <= y + height + 1e-6
                            });
                            if path.points.len() > 1 {
                                paths.push(path);
                            }
                        }
                    }
                }
            }
        }
        Motif::Asanoha => {
            // Hemp leaf: six-pointed stars of lines in a triangle grid.
            for column in 0..count {
                let centre = Point::new(at(column) + step / 2.0, y + height / 2.0);
                let radius = height.min(step) * 0.5;
                let corners: Vec<Point> = (0..6)
                    .map(|index| {
                        let angle = index as f64 * PI / 3.0 + PI / 6.0;
                        Point::new(
                            centre.x + radius * angle.cos(),
                            centre.y + radius * angle.sin(),
                        )
                    })
                    .collect();
                paths.push(closed(corners.clone()));
                for (index, corner) in corners.iter().enumerate() {
                    paths.push(Path::new(vec![centre, *corner], false));
                    let next = corners[(index + 1) % 6];
                    let middle = Point::new(
                        (centre.x + corner.x + next.x) / 3.0,
                        (centre.y + corner.y + next.y) / 3.0,
                    );
                    paths.push(Path::new(vec![*corner, middle, next], false));
                    paths.push(Path::new(vec![centre, middle], false));
                }
            }
        }
        Motif::Shippo => {
            let radius = step * 0.5;
            let rows = ((height / radius).floor() as usize).max(1);
            for row in 0..=rows {
                for column in 0..=count {
                    let centre = Point::new(
                        x + column as f64 * step,
                        y + row as f64 * radius * 2.0 * (height / (2.0 * radius * rows as f64)),
                    );
                    paths.push(clip(circle(centre, radius), x, y, width, height));
                    let between = Point::new(centre.x + radius, centre.y + radius);
                    paths.push(clip(circle(between, radius), x, y, width, height));
                }
            }
        }
        Motif::Fans => {
            for column in 0..count {
                let centre = Point::new(at(column) + step / 2.0, y);
                let radius = (step / 2.0).min(height) * 0.98;
                for ring in 1..=3 {
                    paths.push(arc(centre, radius * ring as f64 / 3.0, 0.0, PI));
                }
                for ray in 1..6 {
                    let angle = PI * ray as f64 / 6.0;
                    paths.push(line(&[
                        (
                            centre.x + radius / 3.0 * angle.cos(),
                            centre.y + radius / 3.0 * angle.sin(),
                        ),
                        (
                            centre.x + radius * angle.cos(),
                            centre.y + radius * angle.sin(),
                        ),
                    ]));
                }
            }
        }
        Motif::SteppedChevrons => {
            for column in 0..count {
                let (left, w, h) = (at(column), step, height);
                for level in 0..3 {
                    let v = 0.15 + level as f64 * 0.3;
                    paths.push(line(&[
                        (left + 0.05 * w, y + v * h),
                        (left + 0.25 * w, y + v * h),
                        (left + 0.25 * w, y + (v + 0.15) * h),
                        (left + 0.75 * w, y + (v + 0.15) * h),
                        (left + 0.75 * w, y + v * h),
                        (left + 0.95 * w, y + v * h),
                    ]));
                }
            }
        }
        Motif::Sunburst => {
            for column in 0..count {
                let centre = Point::new(at(column) + step / 2.0, y + height / 2.0);
                let radius = height.min(step) * 0.47;
                paths.push(circle(centre, radius * 0.25));
                let rays = 12 + 4 * random.below(3);
                for ray in 0..rays {
                    let angle = TAU * ray as f64 / rays as f64;
                    let reach = if ray % 2 == 0 { radius } else { radius * 0.7 };
                    paths.push(line(&[
                        (
                            centre.x + radius * 0.32 * angle.cos(),
                            centre.y + radius * 0.32 * angle.sin(),
                        ),
                        (
                            centre.x + reach * angle.cos(),
                            centre.y + reach * angle.sin(),
                        ),
                    ]));
                }
            }
        }
        Motif::Koru => {
            for column in 0..count {
                let base = Point::new(at(column) + step * 0.3, y + height * 0.05);
                let centre = Point::new(at(column) + step * 0.55, y + height * 0.6);
                let radius = height.min(step) * 0.35;
                let mut path = spiral(centre, radius, 1.5, PI, true);
                path.points.reverse();
                path.points.insert(0, base);
                paths.push(path);
            }
        }
        Motif::Spearheads => {
            for column in 0..count {
                let (left, w, h) = (at(column), step, height);
                paths.push(closed(vec![
                    Point::new(left + 0.5 * w, y + 0.95 * h),
                    Point::new(left + 0.8 * w, y + 0.45 * h),
                    Point::new(left + 0.55 * w, y + 0.3 * h),
                    Point::new(left + 0.55 * w, y + 0.05 * h),
                    Point::new(left + 0.45 * w, y + 0.05 * h),
                    Point::new(left + 0.45 * w, y + 0.3 * h),
                    Point::new(left + 0.2 * w, y + 0.45 * h),
                ]));
            }
        }
        Motif::Stars => {
            for column in 0..count {
                let centre = Point::new(at(column) + step / 2.0, y + height / 2.0);
                let radius = height.min(step) * 0.48;
                // Eight-pointed star: two squares, the inner octagon.
                let star: Vec<Point> = (0..16)
                    .map(|index| {
                        let angle = index as f64 * PI / 8.0;
                        let r = if index % 2 == 0 {
                            radius
                        } else {
                            radius * 0.72
                        };
                        Point::new(centre.x + r * angle.cos(), centre.y + r * angle.sin())
                    })
                    .collect();
                paths.push(closed(star));
                paths.push(closed(
                    (0..8)
                        .map(|index| {
                            let angle = index as f64 * PI / 4.0 + PI / 8.0;
                            Point::new(
                                centre.x + radius * 0.4 * angle.cos(),
                                centre.y + radius * 0.4 * angle.sin(),
                            )
                        })
                        .collect(),
                ));
            }
        }
        Motif::Lattice => {
            for column in 0..count {
                let (left, w, h) = (at(column), step, height);
                paths.push(line(&[(left, y), (left + w, y + h)]));
                paths.push(line(&[(left, y + h), (left + w, y)]));
                paths.push(closed(vec![
                    Point::new(left + 0.5 * w, y + 0.2 * h),
                    Point::new(left + 0.8 * w, y + 0.5 * h),
                    Point::new(left + 0.5 * w, y + 0.8 * h),
                    Point::new(left + 0.2 * w, y + 0.5 * h),
                ]));
            }
        }
    }
}

/// Keeps the points of a closed curve inside the box, as open pieces.
fn clip(path: Path, x: f64, y: f64, width: f64, height: f64) -> Path {
    let inside = |p: &Point| {
        p.x >= x - 1e-6 && p.x <= x + width + 1e-6 && p.y >= y - 1e-6 && p.y <= y + height + 1e-6
    };
    if path.points.iter().all(inside) {
        return path;
    }
    // Longest run inside, enough for these overlapping-circle motifs.
    let count = path.points.len();
    let start = (0..count)
        .find(|&index| !inside(&path.points[index]))
        .unwrap_or(0);
    let mut best: Vec<Point> = Vec::new();
    let mut run: Vec<Point> = Vec::new();
    for offset in 1..=count {
        let point = path.points[(start + offset) % count];
        if inside(&point) {
            run.push(point);
        } else {
            if run.len() > best.len() {
                best = std::mem::take(&mut run);
            }
            run.clear();
        }
    }
    if run.len() > best.len() {
        best = run;
    }
    Path::new(best, false)
}

/// Two-strand plait drawn as ribbons, the strand passing under at
/// every other crossing broken off: the look of knotwork.
fn plait(paths: &mut Vec<Path>, x: f64, y: f64, width: f64, height: f64, period: f64) {
    let periods = (width / period).round().max(1.0);
    let period = width / periods;
    let amplitude = height * 0.32;
    let ribbon = height * 0.07;
    let gap = ribbon * 2.6;
    let samples = (width / (period / 40.0)).ceil() as usize;
    for strand in 0..2 {
        let sign = if strand == 0 { 1.0 } else { -1.0 };
        let centre_line = |u: f64| -> (Point, Point) {
            let phase = TAU * (u - x) / period;
            let point = Point::new(u, y + height / 2.0 + sign * amplitude * phase.sin());
            let slope = sign * amplitude * TAU / period * phase.cos();
            let length = (1.0 + slope * slope).sqrt();
            (point, Point::new(-slope / length, 1.0 / length))
        };
        for side in [-1.0, 1.0] {
            let mut piece: Vec<Point> = Vec::new();
            for index in 0..=samples {
                let u = x + width * index as f64 / samples as f64;
                // Crossings every half period; this strand goes under at
                // alternate ones.
                let half = period / 2.0;
                let nearest = ((u - x) / half).round();
                let under = (nearest as i64 + strand) % 2 == 0;
                let near_crossing = ((u - x) - nearest * half).abs() < gap / 2.0;
                if under && near_crossing && nearest > 0.0 && (nearest * half) < width - 1e-6 {
                    if piece.len() > 1 {
                        paths.push(Path::new(std::mem::take(&mut piece), false));
                    }
                    piece.clear();
                    continue;
                }
                let (point, normal) = centre_line(u);
                piece.push(Point::new(
                    point.x + side * ribbon * normal.x,
                    point.y + side * ribbon * normal.y,
                ));
            }
            if piece.len() > 1 {
                paths.push(Path::new(piece, false));
            }
        }
    }
}

/// Elder Futhark runes, strokes in a unit box (x right, y up).
const RUNES: &[&[&[(f64, f64)]]] = &[
    &[
        &[(0.2, 0.0), (0.2, 1.0)],
        &[(0.2, 0.6), (0.8, 0.9)],
        &[(0.2, 0.35), (0.8, 0.65)],
    ],
    &[&[(0.2, 0.0), (0.2, 1.0), (0.8, 0.7), (0.8, 0.0)]],
    &[
        &[(0.2, 0.0), (0.2, 1.0)],
        &[(0.2, 0.75), (0.7, 0.5), (0.2, 0.25)],
    ],
    &[
        &[(0.2, 0.0), (0.2, 1.0)],
        &[(0.2, 1.0), (0.8, 0.75)],
        &[(0.2, 0.7), (0.8, 0.45)],
    ],
    &[&[(0.2, 0.0), (0.2, 1.0), (0.75, 0.75), (0.2, 0.5), (0.8, 0.0)]],
    &[&[(0.75, 0.9), (0.25, 0.5), (0.75, 0.1)]],
    &[&[(0.15, 0.0), (0.85, 1.0)], &[(0.15, 1.0), (0.85, 0.0)]],
    &[&[(0.2, 0.0), (0.2, 1.0), (0.7, 0.8), (0.2, 0.6)]],
    &[
        &[(0.2, 0.0), (0.2, 1.0)],
        &[(0.8, 0.0), (0.8, 1.0)],
        &[(0.2, 0.65), (0.8, 0.35)],
    ],
    &[&[(0.5, 0.0), (0.5, 1.0)], &[(0.25, 0.65), (0.75, 0.35)]],
    &[&[(0.5, 0.0), (0.5, 1.0)]],
    &[
        &[(0.5, 0.0), (0.5, 1.0)],
        &[(0.5, 1.0), (0.85, 0.8)],
        &[(0.5, 0.0), (0.15, 0.2)],
    ],
    &[
        &[(0.2, 1.0), (0.2, 0.0)],
        &[(0.2, 1.0), (0.5, 0.75), (0.8, 1.0)],
        &[(0.2, 0.0), (0.5, 0.25), (0.8, 0.0)],
    ],
    &[
        &[(0.5, 0.0), (0.5, 1.0)],
        &[(0.15, 0.7), (0.5, 0.45), (0.85, 0.7)],
    ],
    &[&[(0.7, 1.0), (0.3, 0.7), (0.7, 0.35), (0.3, 0.0)]],
    &[
        &[(0.5, 0.0), (0.5, 1.0)],
        &[(0.15, 0.7), (0.5, 1.0), (0.85, 0.7)],
    ],
    &[&[
        (0.25, 0.0),
        (0.25, 1.0),
        (0.75, 0.8),
        (0.25, 0.5),
        (0.75, 0.25),
        (0.25, 0.0),
    ]],
    &[&[(0.2, 0.0), (0.2, 1.0), (0.5, 0.7), (0.8, 1.0), (0.8, 0.0)]],
    &[
        &[(0.2, 0.0), (0.2, 1.0), (0.8, 0.5), (0.8, 0.0)],
        &[(0.8, 1.0), (0.2, 0.5)],
    ],
    &[&[(0.3, 0.0), (0.3, 1.0), (0.75, 0.75)]],
    &[&[(0.5, 0.0), (0.2, 0.3), (0.5, 0.6), (0.8, 0.3), (0.5, 0.0)]],
    &[&[
        (0.15, 0.0),
        (0.15, 1.0),
        (0.85, 0.0),
        (0.85, 1.0),
        (0.15, 0.0),
    ]],
    &[&[
        (0.2, 0.0),
        (0.7, 0.5),
        (0.35, 0.85),
        (0.5, 1.0),
        (0.65, 0.85),
        (0.3, 0.5),
        (0.8, 0.0),
    ]],
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_style_fills_the_area_and_stays_inside() {
        for style in OrnamentStyle::ALL {
            for layout in [OrnamentLayout::Bands, OrnamentLayout::Tiles] {
                let params = OrnamentParams {
                    style,
                    layout,
                    width: 100.0,
                    height: 50.0,
                    cell: 10.0,
                    seed: 7,
                };
                let paths = generate(&params);
                assert!(paths.len() > 5, "{style:?} {layout:?}");
                let mut low = Point::new(f64::MAX, f64::MAX);
                let mut high = Point::new(f64::MIN, f64::MIN);
                for point in paths.iter().flat_map(|path| &path.points) {
                    assert!(point.x.is_finite() && point.y.is_finite());
                    low = Point::new(low.x.min(point.x), low.y.min(point.y));
                    high = Point::new(high.x.max(point.x), high.y.max(point.y));
                }
                assert!(
                    low.x >= -0.6 && low.y >= -0.6,
                    "{style:?} {layout:?} {low:?}"
                );
                assert!(
                    high.x <= 100.6 && high.y <= 50.6,
                    "{style:?} {layout:?} {high:?}"
                );
                assert!(
                    high.x - low.x > 80.0,
                    "{style:?} {layout:?} spans the width"
                );
            }
        }
    }

    #[test]
    fn a_seed_gives_the_same_ornament_and_another_seed_a_new_one() {
        let params = OrnamentParams::default();
        assert_eq!(generate(&params), generate(&params));
        let other = OrnamentParams { seed: 2, ..params };
        assert_ne!(generate(&params), generate(&other));
    }
}
