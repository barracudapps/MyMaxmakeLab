//! Gentler milling: programs rewritten to spare the cutter and the
//! machine, at the cost of a little time.
//!
//! - A straight plunge loads the tip of the cutter, which is not made to
//!   cut downwards, and drives the spindle bearings: plunges become
//!   zig-zag ramps along the next cut, at a shallow angle.
//! - Reversing direction at full feed shakes the frame and snaps small
//!   cutters: the feed drops over the last and first millimetres of
//!   sharp corners.
//!
//! Only absolute millimetre programs are rewritten; others are returned
//! unchanged.

use std::fmt::Write as _;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GentleOptions {
    /// Ramp angle in degrees; `None` keeps straight plunges.
    pub ramp_angle: Option<f64>,
    /// Corners turning more than this (degrees) are slowed down.
    pub corner_angle: f64,
    /// Feed in corners as a share of the programmed feed; 1 disables.
    pub corner_feed: f64,
    /// Length slowed down on each side of a corner, mm.
    pub corner_distance: f64,
}

impl GentleOptions {
    /// Spares the tool and the machine most.
    pub fn gentle() -> Self {
        Self {
            ramp_angle: Some(3.0),
            corner_angle: 45.0,
            corner_feed: 0.5,
            corner_distance: 1.5,
        }
    }

    /// Fair share between time and wear.
    pub fn balanced() -> Self {
        Self {
            ramp_angle: Some(8.0),
            corner_angle: 75.0,
            corner_feed: 0.75,
            corner_distance: 1.0,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct GentleReport {
    pub ramps: usize,
    pub corners: usize,
    /// Why the program was left as it was.
    pub skipped: Option<String>,
}

#[derive(Clone, Copy)]
struct Move {
    from: [f64; 3],
    to: [f64; 3],
    feed: f64,
}

/// Coordinate words of a line and whether it is a feed move.
fn words(code: &str) -> Vec<(char, f64)> {
    let mut out = Vec::new();
    let mut chars = code.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_alphabetic() {
            let mut number = String::new();
            while let Some(&next) = chars.peek() {
                if next.is_ascii_digit() || next == '.' || next == '-' || next == '+' {
                    number.push(next);
                    chars.next();
                } else if next == ' ' && number.is_empty() {
                    chars.next();
                } else {
                    break;
                }
            }
            if let Ok(value) = number.parse() {
                out.push((c.to_ascii_uppercase(), value));
            }
        }
    }
    out
}

fn number(value: f64) -> String {
    let text = format!("{value:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".into()
    } else {
        text.to_owned()
    }
}

fn emit(out: &mut String, active: &mut Option<f64>, point: [f64; 3], feed: f64) {
    *active = Some(feed);
    let _ = writeln!(
        out,
        "G1 X{} Y{} Z{} F{}",
        number(point[0]),
        number(point[1]),
        number(point[2]),
        number(feed)
    );
}

pub fn gentle(source: &str, options: &GentleOptions) -> (String, GentleReport) {
    let mut report = GentleReport::default();
    let lines: Vec<&str> = source.lines().collect();
    let codes: Vec<String> = lines
        .iter()
        .map(|line| {
            let code = line.split(';').next().unwrap_or("");
            match code.find('(') {
                Some(index) => code[..index].to_owned(),
                None => code.to_owned(),
            }
        })
        .collect();
    let upper = source.to_uppercase();
    if upper.contains("G91") || upper.contains("G20") || upper.contains("G93") {
        report.skipped = Some("relative, inch or inverse time program".into());
        return (source.to_owned(), report);
    }
    // First pass: the feed move of every line (None for other lines).
    let mut position = [0.0f64; 3];
    let mut motion = 0u8;
    let mut feed = 0.0f64;
    let mut spindle = false;
    let mut moves: Vec<Option<Move>> = Vec::with_capacity(lines.len());
    for code in &codes {
        let words = words(code);
        let mut target = position;
        let mut has_axis = false;
        let mut other_axis = false;
        let mut arc = false;
        for &(letter, value) in &words {
            match letter {
                'G' => match value.round() as i64 {
                    0 => motion = 0,
                    1 => motion = 1,
                    2 | 3 => {
                        motion = 2;
                        arc = true;
                    }
                    _ => {}
                },
                'M' => match value.round() as i64 {
                    3 | 4 => spindle = true,
                    5 => spindle = false,
                    _ => {}
                },
                'F' => feed = value,
                'X' => {
                    target[0] = value;
                    has_axis = true;
                }
                'Y' => {
                    target[1] = value;
                    has_axis = true;
                }
                'Z' => {
                    target[2] = value;
                    has_axis = true;
                }
                'A' | 'B' | 'C' | 'I' | 'J' | 'R' => other_axis = true,
                _ => {}
            }
        }
        let cutting = has_axis && motion == 1 && !arc && !other_axis && spindle && feed > 0.0;
        moves.push(cutting.then_some(Move {
            from: position,
            to: target,
            feed,
        }));
        if has_axis && !arc {
            position = target;
        } else if arc {
            position = target;
        }
    }
    let next_cut = |index: usize| -> Option<Move> {
        moves[index + 1..]
            .iter()
            .zip(&codes[index + 1..])
            .find_map(|(item, code)| match item {
                Some(found) => Some(Some(*found)),
                None if code.trim().is_empty() => None,
                None => Some(None),
            })
            .flatten()
    };
    let horizontal = |item: &Move| {
        let (dx, dy) = (item.to[0] - item.from[0], item.to[1] - item.from[1]);
        ((dx * dx + dy * dy).sqrt(), dx, dy)
    };
    let turn = |a: &Move, b: &Move| -> Option<f64> {
        let (la, ax, ay) = horizontal(a);
        let (lb, bx, by) = horizontal(b);
        if la < 1e-6 || lb < 1e-6 || (a.to[2] - b.from[2]).abs() > 1e-9 {
            return None;
        }
        Some(
            ((ax * bx + ay * by) / (la * lb))
                .clamp(-1.0, 1.0)
                .acos()
                .to_degrees(),
        )
    };

    let mut out = String::with_capacity(source.len() + source.len() / 4);
    // Slowed start carried to the next move after a corner.
    let mut slow_start = false;
    // Feed active in the output: an original line without F must not
    // inherit a feed written here.
    let mut active_feed: Option<f64> = None;
    for index in 0..lines.len() {
        let Some(item) = moves[index] else {
            if let Some(value) = words(&codes[index])
                .iter()
                .find(|(letter, _)| *letter == 'F')
                .map(|(_, value)| *value)
            {
                active_feed = Some(value);
            }
            out.push_str(lines[index]);
            out.push('\n');
            continue;
        };
        let (length, dx, dy) = horizontal(&item);
        let plunge = length < 1e-6 && item.to[2] < item.from[2] - 1e-6;
        if plunge && let Some(angle) = options.ramp_angle {
            if let Some(next) = next_cut(index) {
                let (next_length, nx, ny) = horizontal(&next);
                let depth = item.from[2] - item.to[2];
                let ramp = depth / angle.to_radians().tan();
                if next_length > 0.5 && ramp > 0.0 {
                    let leg = next_length.min(ramp).max(0.5);
                    // Even number of legs so the ramp ends where it began.
                    let legs = ((ramp / leg / 2.0).ceil().max(1.0) as usize) * 2;
                    let direction = [nx / next_length, ny / next_length];
                    for step in 1..=legs {
                        let out_leg = step % 2 == 1;
                        let point = [
                            item.from[0] + if out_leg { direction[0] * leg } else { 0.0 },
                            item.from[1] + if out_leg { direction[1] * leg } else { 0.0 },
                            item.from[2] - depth * step as f64 / legs as f64,
                        ];
                        emit(&mut out, &mut active_feed, point, item.feed);
                    }
                    report.ramps += 1;
                    continue;
                }
            }
        }
        // Corner at the end of this move?
        let corner = options.corner_feed < 1.0
            && next_cut(index)
                .filter(|next| next.from == item.to)
                .and_then(|next| turn(&item, &next))
                .is_some_and(|angle| angle > options.corner_angle);
        let slow = item.feed * options.corner_feed.clamp(0.1, 1.0);
        let distance = options.corner_distance.min(length / 2.0);
        if (corner || slow_start) && length > 1e-6 && distance > 1e-6 {
            let along = |share: f64| {
                [
                    item.from[0] + dx * share,
                    item.from[1] + dy * share,
                    item.from[2] + (item.to[2] - item.from[2]) * share,
                ]
            };
            let start_share = if slow_start { distance / length } else { 0.0 };
            let end_share = if corner { 1.0 - distance / length } else { 1.0 };
            if slow_start {
                emit(&mut out, &mut active_feed, along(start_share), slow);
            }
            if end_share > start_share + 1e-9 {
                emit(&mut out, &mut active_feed, along(end_share), item.feed);
            }
            if corner {
                emit(&mut out, &mut active_feed, item.to, slow);
                report.corners += 1;
            } else if end_share >= 1.0 - 1e-9 && start_share > 0.0 {
                // Already ended at full feed above.
            }
            slow_start = corner;
            continue;
        }
        slow_start = false;
        out.push_str(lines[index]);
        let has_feed = words(&codes[index])
            .iter()
            .any(|(letter, _)| *letter == 'F');
        if !has_feed && active_feed.is_some_and(|feed| (feed - item.feed).abs() > 1e-9) {
            let _ = write!(out, " F{}", number(item.feed));
        }
        active_feed = Some(item.feed);
        out.push('\n');
    }
    (out, report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    #[test]
    fn plunges_become_ramps_ending_where_they_began() {
        let source = "G90 G21\nM3 S10000\nG0 X0 Y0 Z2\nG1 Z-2 F200\nG1 X20 F800\nG0 Z5\nM5\n";
        let (out, report) = gentle(source, &GentleOptions::gentle());
        assert_eq!(report.ramps, 1);
        let program = parse(&out);
        // No vertical cutting move left, and the ramp ends at X0 Z-2.
        for motion in program.motions.iter().filter(|m| m.is_cutting()) {
            let vertical = (motion.to[0] - motion.from[0]).abs() < 1e-9
                && (motion.to[1] - motion.from[1]).abs() < 1e-9
                && motion.to[2] < motion.from[2];
            assert!(!vertical, "plunge left at {:?}", motion.to);
        }
        let ramp_end = program
            .motions
            .iter()
            .find(|m| (m.to[2] + 2.0).abs() < 1e-9)
            .unwrap();
        assert!(ramp_end.to[0].abs() < 1e-9, "{:?}", ramp_end.to);
        // Each leg descends at the chosen angle at most.
        for motion in program
            .motions
            .iter()
            .filter(|m| m.is_cutting() && m.to[2] < m.from[2])
        {
            let run = (motion.to[0] - motion.from[0]).hypot(motion.to[1] - motion.from[1]);
            let drop = motion.from[2] - motion.to[2];
            assert!(drop / run <= 3.0f64.to_radians().tan() + 1e-6);
        }
    }

    #[test]
    fn sharp_corners_are_slowed_and_the_path_kept() {
        let source = "G90 G21\nM3 S10000\nG0 X0 Y0 Z-1\nG1 X20 F1000\nG1 X20 Y20\nG1 X40 Y20\nG1 X60 Y20\nM5\n";
        let (out, report) = gentle(source, &GentleOptions::gentle());
        assert_eq!(report.corners, 2);
        let (before, after) = (parse(source), parse(&out));
        let ends = |program: &crate::parser::Program| program.motions.last().unwrap().to;
        assert_eq!(ends(&before), ends(&after));
        let length = |program: &crate::parser::Program| {
            program.motions.iter().map(|m| m.length()).sum::<f64>()
        };
        assert!((length(&before) - length(&after)).abs() < 1e-9);
        assert!(after.motions.iter().any(|m| (m.feed - 500.0).abs() < 1e-9));
        // The straight move after the corners runs at full feed again.
        assert!((after.motions.last().unwrap().feed - 1000.0).abs() < 1e-9);
        // Laser or inch programs are left alone.
        let (same, skipped) = gentle("G20\nM3\nG1 X1 F10\n", &GentleOptions::gentle());
        assert_eq!(same, "G20\nM3\nG1 X1 F10\n");
        assert!(skipped.skipped.is_some());
    }
}
