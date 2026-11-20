//! Attachment points: where a cut outline keeps small uncut bridges
//! (laser) or tabs (CNC) so the part stays in the sheet, like the
//! parts of a model kit on their sprue or a wooden 3D puzzle in its
//! board. Points go on straight, calm stretches of the outline, away
//! from corners and small details where a bridge would break the part
//! or leave a visible scar, and spread evenly so the part cannot
//! pivot.

use crate::geometry::{Path, Point};

/// Number of points chosen automatically for an outline of `length`
/// mm: one for tiny parts, then one per 80 mm, between 2 and 8.
pub fn automatic_count(length: f64) -> usize {
    if length < 25.0 {
        1
    } else {
        ((length / 80.0).round() as usize).clamp(2, 8)
    }
}

/// Centres of the attachment points as distances along the closed
/// `ring`, `width` mm wide each. `count` `None` chooses it from the
/// length. Empty when the outline is too short for them.
pub fn attachment_points(ring: &Path, count: Option<usize>, width: f64) -> Vec<f64> {
    let points = &ring.points;
    if points.len() < 3 {
        return Vec::new();
    }
    let lengths: Vec<f64> = (0..points.len())
        .map(|index| points[index].distance(points[(index + 1) % points.len()]))
        .collect();
    let total: f64 = lengths.iter().sum();
    let count = count.unwrap_or_else(|| automatic_count(total));
    if count == 0 || total < count as f64 * width * 3.0 {
        return Vec::new();
    }
    // Vertex positions along the ring and how sharply the outline turns
    // there (radians).
    let mut vertices = Vec::with_capacity(points.len());
    let mut position = 0.0;
    for index in 0..points.len() {
        let previous = points[(index + points.len() - 1) % points.len()];
        let here = points[index];
        let next = points[(index + 1) % points.len()];
        let turn = angle_between(here - previous, next - here);
        vertices.push((position, turn));
        position += lengths[index];
    }
    // Badness of a centre: turning within reach of the bridge, a sharp
    // corner nearby counting much more than a gentle curve.
    let reach = width / 2.0 + 1.5;
    let badness = |centre: f64| -> f64 {
        vertices
            .iter()
            .map(|&(at, turn)| {
                let gap = (at - centre).rem_euclid(total);
                let distance = gap.min(total - gap);
                if distance > reach * 2.0 {
                    0.0
                } else {
                    let weight = if distance <= reach { 1.0 } else { 0.3 };
                    let sharp = if turn > 0.5 { 10.0 } else { 1.0 };
                    turn * sharp * weight
                }
            })
            .sum()
    };
    let spacing = total / count as f64;
    let window = spacing * 0.3;
    let sample = 0.5f64.min(spacing / 8.0);
    let mut best: Option<(f64, Vec<f64>)> = None;
    let mut offset = 0.0;
    while offset < spacing {
        let mut chosen = Vec::with_capacity(count);
        let mut score = 0.0;
        for index in 0..count {
            let ideal = offset + spacing * index as f64;
            let mut candidate = (f64::INFINITY, ideal);
            let mut shift = -window;
            while shift <= window {
                let centre = (ideal + shift).rem_euclid(total);
                // Moving away from the even spread costs a little.
                let cost = badness(centre) + shift.abs() / spacing * 0.2;
                if cost < candidate.0 {
                    candidate = (cost, centre);
                }
                shift += sample;
            }
            score += candidate.0;
            chosen.push(candidate.1);
        }
        if best
            .as_ref()
            .is_none_or(|(best_score, _)| score < *best_score)
        {
            best = Some((score, chosen));
        }
        offset += sample.max(spacing / 40.0);
    }
    let mut chosen = best.map(|(_, chosen)| chosen).unwrap_or_default();
    chosen.sort_by(f64::total_cmp);
    chosen
}

fn angle_between(a: Point, b: Point) -> f64 {
    let (length_a, length_b) = (a.x.hypot(a.y), b.x.hypot(b.y));
    if length_a < 1e-12 || length_b < 1e-12 {
        return 0.0;
    }
    let cosine = ((a.x * b.x + a.y * b.y) / (length_a * length_b)).clamp(-1.0, 1.0);
    cosine.acos()
}

/// The parts of the closed `ring` left once the bridges centred at
/// `centres` (distances along it) are removed: open paths to cut.
pub fn split_at_bridges(ring: &Path, centres: &[f64], width: f64) -> Vec<Path> {
    let total = ring.length();
    if centres.is_empty() || total <= 0.0 {
        return vec![ring.clone()];
    }
    let mut pieces = Vec::new();
    for (index, &centre) in centres.iter().enumerate() {
        let start = centre + width / 2.0;
        let next = centres[(index + 1) % centres.len()];
        let mut end = next - width / 2.0;
        if end <= start {
            end += total;
        }
        let piece = sub_path(ring, start, end, total);
        if piece.points.len() >= 2 {
            pieces.push(piece);
        }
    }
    pieces
}

/// Open path along the closed `ring` from distance `start` to `end`
/// (which may go past the end and wrap).
fn sub_path(ring: &Path, start: f64, end: f64, total: f64) -> Path {
    let points = &ring.points;
    let count = points.len();
    let at = |distance: f64| -> Point {
        let mut distance = distance.rem_euclid(total);
        for index in 0..count {
            let (a, b) = (points[index], points[(index + 1) % count]);
            let length = a.distance(b);
            if distance <= length {
                return a.lerp(b, if length > 0.0 { distance / length } else { 0.0 });
            }
            distance -= length;
        }
        points[0]
    };
    let mut out = vec![at(start)];
    // Vertices strictly between start and end, over two laps.
    let mut position = 0.0;
    for lap in 0..2 {
        for index in 0..count {
            let vertex_at = position + lap as f64 * total;
            if vertex_at > start && vertex_at < end {
                out.push(points[index]);
            }
            position += points[index].distance(points[(index + 1) % count]);
        }
        position = 0.0;
    }
    out.push(at(end));
    Path::new(out, false)
}

/// Outer outlines of one shape: closed paths not inside another closed
/// path of the same shape (holes are scrap and need no bridge).
pub fn outer_rings(paths: &[Path]) -> Vec<usize> {
    let closed: Vec<usize> = (0..paths.len())
        .filter(|&index| paths[index].closed && paths[index].points.len() > 2)
        .collect();
    closed
        .iter()
        .copied()
        .filter(|&index| {
            let probe = paths[index].points[0];
            !closed
                .iter()
                .any(|&other| other != index && contains(&paths[other], probe))
        })
        .collect()
}

fn contains(path: &Path, point: Point) -> bool {
    let mut inside = false;
    let count = path.points.len();
    for index in 0..count {
        let a = path.points[index];
        let b = path.points[(index + 1) % count];
        if (a.y > point.y) != (b.y > point.y)
            && point.x < a.x + (point.y - a.y) / (b.y - a.y) * (b.x - a.x)
        {
            inside = !inside;
        }
    }
    inside
}

/// Places parts of `sizes` (width, height) in rows no wider than
/// `max_width`, tallest first, `spacing` apart. Returns the lower-left
/// corner of each part and the size of the whole arrangement.
pub fn shelf_pack(sizes: &[(f64, f64)], max_width: f64, spacing: f64) -> (Vec<Point>, (f64, f64)) {
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by(|&a, &b| sizes[b].1.total_cmp(&sizes[a].1));
    let mut corners = vec![Point::default(); sizes.len()];
    let (mut x, mut y, mut row_height, mut width) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for index in order {
        let (part_width, part_height) = sizes[index];
        if x > 0.0 && x + part_width > max_width {
            y += row_height + spacing;
            x = 0.0;
            row_height = 0.0;
        }
        corners[index] = Point::new(x, y);
        x += part_width + spacing;
        width = width.max(x - spacing);
        row_height = row_height.max(part_height);
    }
    (corners, (width, y + row_height))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rectangle(width: f64, height: f64) -> Path {
        Path::new(
            vec![
                Point::new(0.0, 0.0),
                Point::new(width, 0.0),
                Point::new(width, height),
                Point::new(0.0, height),
            ],
            true,
        )
    }

    #[test]
    fn points_avoid_corners_and_spread_around_the_part() {
        let ring = rectangle(100.0, 40.0);
        let centres = attachment_points(&ring, None, 1.0);
        assert_eq!(centres.len(), 4, "280 mm outline");
        let corners = [0.0, 100.0, 140.0, 240.0, 280.0];
        for centre in &centres {
            let nearest = corners
                .iter()
                .map(|corner| (corner - centre).abs())
                .fold(f64::MAX, f64::min);
            assert!(nearest > 2.0, "bridge {centre} next to a corner");
        }
        let gaps: Vec<f64> = centres
            .windows(2)
            .map(|pair| pair[1] - pair[0])
            .chain(std::iter::once(280.0 - centres[3] + centres[0]))
            .collect();
        assert!(gaps.iter().all(|gap| *gap > 40.0), "{gaps:?}");
    }

    #[test]
    fn a_saw_tooth_edge_gets_its_bridges_on_the_calm_side() {
        // Bottom edge flat, top edge with many teeth.
        let mut points = vec![Point::new(0.0, 0.0), Point::new(60.0, 0.0)];
        for tooth in (0..12).rev() {
            let x = tooth as f64 * 5.0;
            points.push(Point::new(x + 5.0, 20.0));
            points.push(Point::new(x + 2.5, 24.0));
        }
        points.push(Point::new(0.0, 20.0));
        let ring = Path::new(points, true);
        let centres = attachment_points(&ring, Some(2), 0.6);
        // The teeth run from 80 mm (top right) to about 193 mm along the
        // ring; straight edges are everywhere else.
        assert_eq!(centres.len(), 2);
        assert!(
            centres.iter().all(|centre| !(80.0..193.5).contains(centre)),
            "{centres:?}"
        );
    }

    #[test]
    fn bridges_leave_gaps_of_the_requested_width() {
        let ring = rectangle(40.0, 20.0);
        let centres = vec![20.0, 70.0];
        let pieces = split_at_bridges(&ring, &centres, 1.0);
        assert_eq!(pieces.len(), 2);
        let cut: f64 = pieces.iter().map(Path::length).sum();
        assert!((cut - (120.0 - 2.0)).abs() < 1e-9, "{cut}");
        assert!(pieces[0].points[0].distance(Point::new(20.5, 0.0)) < 1e-9);
    }

    #[test]
    fn holes_are_not_outer_rings() {
        let outer = rectangle(40.0, 20.0);
        let hole = Path::new(
            vec![
                Point::new(10.0, 5.0),
                Point::new(15.0, 5.0),
                Point::new(15.0, 10.0),
                Point::new(10.0, 10.0),
            ],
            true,
        );
        assert_eq!(outer_rings(&[outer, hole]), vec![0]);
    }

    #[test]
    fn parts_are_packed_in_rows_without_overlap() {
        let sizes = [(30.0, 10.0), (20.0, 25.0), (40.0, 5.0), (10.0, 10.0)];
        let (corners, (width, height)) = shelf_pack(&sizes, 60.0, 2.0);
        assert!(width <= 60.0 + 1e-9);
        for a in 0..sizes.len() {
            for b in a + 1..sizes.len() {
                let apart = corners[a].x + sizes[a].0 + 2.0 - 1e-9 <= corners[b].x
                    || corners[b].x + sizes[b].0 + 2.0 - 1e-9 <= corners[a].x
                    || corners[a].y + sizes[a].1 + 2.0 - 1e-9 <= corners[b].y
                    || corners[b].y + sizes[b].1 + 2.0 - 1e-9 <= corners[a].y;
                assert!(apart, "{a} and {b} overlap");
            }
        }
        assert!(height < 25.0 + 10.0 + 5.0 + 3.0 * 2.0 + 1e-9);
    }
}
