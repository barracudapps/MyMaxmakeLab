use crate::geometry::{Path, Point, Rect, contains_even_odd};

/// Orders paths to reduce travel. A closed path is only scheduled once
/// every closed path lying inside it is done, so that holes are cut
/// before the part drops out of the sheet. Closed paths start at their
/// vertex nearest to the head, open paths may be reversed.
pub fn order_paths(paths: Vec<Path>, start: Point) -> Vec<Path> {
    order(paths, start, true)
}

fn order(paths: Vec<Path>, start: Point, improve: bool) -> Vec<Path> {
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
    let mut order_ids = Vec::with_capacity(count);
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
        order_ids.push(index);
    }
    if improve {
        untangle(&mut ordered, &order_ids, &parents, start);
    }
    ordered
}

/// Or-opt: moves runs of one to three paths (possibly reversed) to
/// the place where they cost the least travel, as long as every hole
/// stays before its outline.
fn relocate(
    ordered: &mut [Path],
    ids: &mut [usize],
    parents: &[Vec<usize>],
    start: Point,
    deadline: std::time::Instant,
) {
    let count = ordered.len();
    let valid = |ids: &[usize]| {
        let mut position = vec![0usize; parents.len()];
        for (index, &id) in ids.iter().enumerate() {
            position[id] = index;
        }
        ids.iter().all(|&id| {
            parents[id]
                .iter()
                .all(|&outer| position[outer] > position[id])
        })
    };
    let mut improved = true;
    while improved && std::time::Instant::now() < deadline {
        improved = false;
        for length in 1..=3usize.min(count - 1) {
            let mut i = 0;
            while i + length <= count && std::time::Instant::now() < deadline {
                let previous = if i == 0 {
                    start
                } else {
                    exit_point(&ordered[i - 1])
                };
                let next = ordered.get(i + length).map(entry_point);
                let (first_entry, last_exit) = (
                    entry_point(&ordered[i]),
                    exit_point(&ordered[i + length - 1]),
                );
                let tail = |from: Point, to: Option<Point>| to.map_or(0.0, |to| from.distance(to));
                // Travel saved by taking the run out.
                let saved =
                    previous.distance(first_entry) + tail(last_exit, next) - tail(previous, next);
                let mut best: Option<(f64, usize, bool)> = None;
                for gap in 0..=count {
                    // Gap before `gap`: the run's own place is not a move.
                    if gap >= i && gap <= i + length {
                        continue;
                    }
                    let before = if gap == 0 {
                        start
                    } else {
                        exit_point(&ordered[gap - 1])
                    };
                    let after = ordered.get(gap).map(entry_point);
                    for reversed in [false, true] {
                        // A reversed run enters at its old exit.
                        let (entry, exit) = if reversed {
                            (last_exit, first_entry)
                        } else {
                            (first_entry, last_exit)
                        };
                        let added =
                            before.distance(entry) + tail(exit, after) - tail(before, after);
                        let gain = saved - added;
                        if gain > 1e-9 && best.is_none_or(|(best_gain, ..)| gain > best_gain) {
                            best = Some((gain, gap, reversed));
                        }
                    }
                }
                if let Some((_, gap, reversed)) = best {
                    let mut paths: Vec<Path> = ordered.to_vec();
                    let mut order: Vec<usize> = ids.to_vec();
                    let mut run: Vec<Path> = paths.drain(i..i + length).collect();
                    let mut run_ids: Vec<usize> = order.drain(i..i + length).collect();
                    if reversed {
                        run.reverse();
                        run_ids.reverse();
                        for path in run.iter_mut().filter(|path| !path.closed) {
                            path.points.reverse();
                        }
                    }
                    let at = if gap > i { gap - length } else { gap };
                    paths.splice(at..at, run);
                    order.splice(at..at, run_ids);
                    if valid(&order) {
                        ordered.clone_from_slice(&paths);
                        ids.copy_from_slice(&order);
                        improved = true;
                    }
                }
                i += 1;
            }
        }
    }
}

fn entry_point(path: &Path) -> Point {
    path.points.first().copied().unwrap_or_default()
}

/// Travel between consecutive paths, from the head position.
pub fn travel_length(paths: &[Path], start: Point) -> f64 {
    let mut head = start;
    let mut total = 0.0;
    for path in paths.iter().filter(|path| !path.points.is_empty()) {
        total += head.distance(entry_point(path));
        head = exit_point(path);
    }
    total
}

/// 2-opt on the greedy order: reversing a run of paths removes crossing
/// travels. A run holding a hole and its outline is never reversed, so
/// holes stay before outlines. Bounded in time for huge jobs.
fn untangle(ordered: &mut [Path], ids: &[usize], parents: &[Vec<usize>], start: Point) {
    let count = ordered.len();
    if count < 4 {
        return;
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(150);
    let mut ids = ids.to_vec();
    let mut improved = true;
    while improved && std::time::Instant::now() < deadline {
        improved = false;
        for i in 0..count - 1 {
            let before = if i == 0 {
                start
            } else {
                exit_point(&ordered[i - 1])
            };
            for j in i + 1..count {
                let next = (j + 1 < count).then(|| entry_point(&ordered[j + 1]));
                let current = before.distance(entry_point(&ordered[i]))
                    + next.map_or(0.0, |next| exit_point(&ordered[j]).distance(next));
                let swapped = before.distance(exit_point(&ordered[j]))
                    + next.map_or(0.0, |next| entry_point(&ordered[i]).distance(next));
                if swapped + 1e-9 >= current {
                    continue;
                }
                // Keep every hole before its outline.
                let run = &ids[i..=j];
                let breaks = run
                    .iter()
                    .any(|&id| parents[id].iter().any(|outer| run.contains(outer)));
                if breaks {
                    continue;
                }
                ordered[i..=j].reverse();
                ids[i..=j].reverse();
                for path in &mut ordered[i..=j] {
                    if !path.closed {
                        path.points.reverse();
                    }
                }
                improved = true;
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
        }
    }
    relocate(ordered, &mut ids, parents, start, deadline);
    // Closed paths start again at the vertex nearest to the new head.
    let mut head = start;
    for path in ordered.iter_mut() {
        if path.closed && !path.points.is_empty() {
            let (vertex, _, _) = best_entry(path, head);
            path.points.rotate_left(vertex);
        }
        head = exit_point(path);
    }
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

    #[test]
    fn crossing_travels_are_untangled() {
        // Dashes on both sides of the head at growing distances: the
        // greedy order zig-zags across, 2-opt sweeps one side then the
        // other.
        let mut paths = Vec::new();
        for (index, x) in [1.0, -2.0, 4.0, -8.0, 16.0, -32.0, 64.0, -128.0]
            .into_iter()
            .enumerate()
        {
            let y = index as f64 * 0.01;
            paths.push(Path::new(
                vec![Point::new(x, y), Point::new(x + 0.5, y)],
                false,
            ));
        }
        // A part with a hole stays in the right order.
        paths.push(square(Point::new(200.0, 0.0), 8.0));
        paths.push(square(Point::new(200.0, 0.0), 2.0));
        let start = Point::new(0.0, 0.0);
        let greedy = order(paths.clone(), start, false);
        let ordered = order_paths(paths.clone(), start);
        let outer = ordered
            .iter()
            .position(|path| path.bounds().width() > 10.0)
            .unwrap();
        let hole = ordered
            .iter()
            .position(|path| path.closed && path.bounds().width() < 5.0)
            .unwrap();
        assert!(hole < outer, "hole first");
        assert_eq!(ordered.len(), paths.len());
        let (before, after) = (
            travel_length(&greedy, start),
            travel_length(&ordered, start),
        );
        assert!(after < before * 0.8, "{after} vs {before}");
    }

    #[test]
    fn a_huge_job_is_ordered_quickly_and_better_than_greedy() {
        let mut seed = 7u64;
        let mut random = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as f64 / (1u64 << 31) as f64
        };
        let paths: Vec<Path> = (0..3000)
            .map(|_| {
                let (x, y) = (random() * 400.0, random() * 300.0);
                Path::new(vec![Point::new(x, y), Point::new(x + 2.0, y + 1.0)], false)
            })
            .collect();
        let start = Point::default();
        let began = std::time::Instant::now();
        let ordered = order_paths(paths.clone(), start);
        assert!(began.elapsed().as_secs_f64() < 2.0, "{:?}", began.elapsed());
        let greedy = order(paths, start, false);
        assert!(travel_length(&ordered, start) < travel_length(&greedy, start));
    }
}
