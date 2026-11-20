use crate::geometry::{Path, Point, Rect, contains_even_odd};

/// Orders paths to reduce travel. A closed path is only scheduled once
/// every closed path lying inside it is done, so that holes are cut
/// before the part drops out of the sheet. Closed paths start at their
/// vertex nearest to the head, open paths may be reversed.
pub fn order_paths(paths: Vec<Path>, start: Point) -> Vec<Path> {
    let count = paths.len();
    if count <= 1 {
        return paths;
    }
    let bounds: Vec<Rect> = paths.iter().map(Path::bounds).collect();
    let mut inner_remaining = vec![0usize; count];
    let mut parents: Vec<Vec<usize>> = vec![Vec::new(); count];
    for (inner, inner_path) in paths.iter().enumerate() {
        let Some(&probe) = inner_path.points.first() else {
            continue;
        };
        for (outer, outer_path) in paths.iter().enumerate() {
            if inner == outer || !outer_path.closed {
                continue;
            }
            let (a, b) = (bounds[inner], bounds[outer]);
            let strictly_larger = b.width() * b.height() > a.width() * a.height();
            if strictly_larger
                && b.contains(a.min)
                && b.contains(a.max)
                && contains_even_odd(std::slice::from_ref(outer_path), probe)
            {
                inner_remaining[outer] += 1;
                parents[inner].push(outer);
            }
        }
    }

    let mut slots: Vec<Option<Path>> = paths.into_iter().map(Some).collect();
    let mut ordered = Vec::with_capacity(count);
    let mut head = start;
    while ordered.len() < count {
        let mut best: Option<(usize, usize, bool, f64)> = None;
        for (index, slot) in slots.iter().enumerate() {
            let Some(path) = slot else { continue };
            if inner_remaining[index] > 0 {
                continue;
            }
            let (vertex, reverse, distance) = best_entry(path, head);
            if best.is_none_or(|(_, _, _, best_distance)| distance < best_distance) {
                best = Some((index, vertex, reverse, distance));
            }
        }
        let (index, vertex, reverse, _) = best.expect("containment graph is acyclic");
        let mut path = slots[index].take().unwrap();
        if path.closed {
            path.points.rotate_left(vertex);
        } else if reverse {
            path.points.reverse();
        }
        head = exit_point(&path);
        for &parent in &parents[index] {
            inner_remaining[parent] -= 1;
        }
        ordered.push(path);
    }
    ordered
}

fn best_entry(path: &Path, head: Point) -> (usize, bool, f64) {
    if path.points.is_empty() {
        return (0, false, f64::INFINITY);
    }
    if path.closed {
        let (vertex, distance) = path
            .points
            .iter()
            .enumerate()
            .map(|(index, point)| (index, point.distance(head)))
            .fold((0, f64::INFINITY), |best, item| {
                if item.1 < best.1 { item } else { best }
            });
        (vertex, false, distance)
    } else {
        let to_start = path.points[0].distance(head);
        let to_end = path.points.last().unwrap().distance(head);
        if to_end < to_start {
            (0, true, to_end)
        } else {
            (0, false, to_start)
        }
    }
}

fn exit_point(path: &Path) -> Point {
    if path.closed {
        path.points[0]
    } else {
        *path.points.last().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(center: Point, half: f64) -> Path {
        Path::new(
            vec![
                center + Point::new(-half, -half),
                center + Point::new(half, -half),
                center + Point::new(half, half),
                center + Point::new(-half, half),
            ],
            true,
        )
    }

    #[test]
    fn holes_are_cut_before_their_outline() {
        let outer = square(Point::new(0.0, 0.0), 10.0);
        let hole = square(Point::new(5.0, 5.0), 1.0);
        let ordered = order_paths(vec![outer.clone(), hole], Point::new(-10.0, -10.0));
        assert_eq!(ordered[1].bounds(), outer.bounds());
    }

    #[test]
    fn open_paths_are_reversed_when_shorter() {
        let line = Path::new(vec![Point::new(10.0, 0.0), Point::new(0.0, 0.0)], false);
        let ordered = order_paths(vec![line.clone(), line], Point::new(0.0, 0.0));
        assert_eq!(ordered[0].points[0], Point::new(0.0, 0.0));
        assert_eq!(ordered[1].points[0], Point::new(10.0, 0.0));
    }
}
