use mml_core::geometry::{Point, Rect, arc_segments};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveKind {
    Rapid,
    Feed,
}

/// One straight motion; arcs are flattened into several motions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Motion {
    pub kind: MoveKind,
    pub from: [f64; 3],
    pub to: [f64; 3],
    /// mm/min, 0 for rapids.
    pub feed: f64,
    /// S value applied during the move, 0 when the tool is off.
    pub power: f64,
    /// 0-based source line, used to highlight progress while streaming.
    pub line: usize,
    /// Rotary (A) angle at the start and end of the move, degrees.
    pub a: [f64; 2],
    /// Table (C) angle at the start and end of the move, degrees.
    pub c: [f64; 2],
    /// Duration programmed in inverse time mode (G93), minutes.
    pub minutes: Option<f64>,
}

impl Motion {
    pub fn length(&self) -> f64 {
        let [dx, dy, dz] = [0, 1, 2].map(|axis| self.to[axis] - self.from[axis]);
        (dx * dx + dy * dy + dz * dz).sqrt()
    }

    /// Length travelled on the stock: for a rotary program of known
    /// radius, the rotation counts as the arc it sweeps.
    pub fn path_length(&self, rotary_radius: Option<f64>) -> f64 {
        let Some(radius) = rotary_radius.filter(|_| self.a[0] != self.a[1]) else {
            return self.length();
        };
        let pieces = ((self.a[1] - self.a[0]).abs() / 2.0).ceil().max(1.0) as usize;
        let at = |t: f64| {
            let point =
                [0, 1, 2].map(|axis| self.from[axis] + (self.to[axis] - self.from[axis]) * t);
            unwrap_rotary(point, self.a[0] + (self.a[1] - self.a[0]) * t, radius)
        };
        let mut previous = at(0.0);
        let mut total = 0.0;
        for piece in 1..=pieces {
            let next = at(piece as f64 / pieces as f64);
            total += (0..3)
                .map(|axis| (next[axis] - previous[axis]).powi(2))
                .sum::<f64>()
                .sqrt();
            previous = next;
        }
        total
    }

    pub fn is_cutting(&self) -> bool {
        self.kind == MoveKind::Feed && self.power > 0.0
    }
}

#[derive(Debug, Clone, Default)]
pub struct Program {
    pub motions: Vec<Motion>,
    pub line_count: usize,
    pub warnings: Vec<String>,
    /// The program commands a rotary axis (A, B or C words). Used by the
    /// safety monitor: a rotary axis moving during a job that never
    /// commands it is an anomaly.
    pub uses_rotary: bool,
    /// Lowest and highest Z reached, `(0, 0)` for 2D programs.
    pub z_range: (f64, f64),
    /// Diameter of the stock turned by the A axis, announced by the
    /// programs of this application (`; Rotary stock diameter 40 mm`).
    pub rotary_diameter: Option<f64>,
    /// Table height of a 5-axis trunnion program
    /// (`; Trunnion table height 10 mm`), for previews in part space.
    pub trunnion_table: Option<f64>,
}

/// Point of a rotary program in the frame of the stock: the machine
/// position (Y0 on the axis, Z0 on the stock top of `radius`) turned
/// back by the A angle.
pub fn unwrap_rotary(point: [f64; 3], a_degrees: f64, radius: f64) -> [f64; 3] {
    let (sin, cos) = a_degrees.to_radians().sin_cos();
    let (y, z) = (point[1], point[2] + radius);
    [point[0], y * cos + z * sin, -y * sin + z * cos]
}

impl Program {
    /// XY extent of the cutting moves (laser on or spindle running).
    pub fn cut_bounds(&self) -> Rect {
        let mut rect = Rect::EMPTY;
        for motion in self.motions.iter().filter(|motion| motion.is_cutting()) {
            rect.include(Point::new(motion.from[0], motion.from[1]));
            rect.include(Point::new(motion.to[0], motion.to[1]));
        }
        rect
    }

    pub fn bounds(&self) -> Rect {
        let mut rect = Rect::EMPTY;
        for motion in &self.motions {
            rect.include(Point::new(motion.to[0], motion.to[1]));
        }
        rect
    }
}

struct State {
    position: [f64; 3],
    a: f64,
    c: f64,
    inverse_time: bool,
    absolute: bool,
    scale: f64,
    motion: u8,
    feed: f64,
    s_value: f64,
    tool_on: bool,
}

const ARC_TOLERANCE: f64 = 0.02;

/// Parses a G-code program. Unknown words are ignored with a warning so
/// that third-party files still preview.
pub fn parse(source: &str) -> Program {
    let mut program = Program::default();
    let mut state = State {
        position: [0.0; 3],
        a: 0.0,
        c: 0.0,
        inverse_time: false,
        absolute: true,
        scale: 1.0,
        motion: 0,
        feed: 0.0,
        s_value: 0.0,
        tool_on: false,
    };
    for (line_index, raw_line) in source.lines().enumerate() {
        program.line_count += 1;
        if let Some(rest) = raw_line.trim().strip_prefix("; Rotary stock diameter ") {
            program.rotary_diameter = rest
                .split_whitespace()
                .next()
                .and_then(|value| value.parse().ok());
        }
        if let Some(rest) = raw_line.trim().strip_prefix("; Trunnion table height ") {
            program.trunnion_table = rest
                .split_whitespace()
                .next()
                .and_then(|value| value.parse().ok());
        }
        let words = match tokenize(raw_line) {
            Ok(words) => words,
            Err(message) => {
                push_warning(&mut program, line_index, &message);
                continue;
            }
        };
        if words.is_empty() {
            continue;
        }
        let mut target: [Option<f64>; 3] = [None; 3];
        let mut target_a = None;
        let mut target_c = None;
        let mut offsets = [None::<f64>; 2];
        let mut radius = None;
        let mut skip_motion = false;
        for (letter, value) in &words {
            match letter {
                'G' => match (*value * 10.0).round() as i64 {
                    0 | 10 | 20 | 30 => {
                        state.motion = (*value).round() as u8;
                    }
                    900 => state.absolute = true,
                    930 => state.inverse_time = true,
                    940 => state.inverse_time = false,
                    910 => state.absolute = false,
                    200 => state.scale = 25.4,
                    210 => state.scale = 1.0,
                    40 | 280 | 300 | 530 | 920 | 381 | 382 | 383 | 384 => skip_motion = true,
                    _ => {}
                },
                'M' => match value.round() as i64 {
                    3 | 4 => state.tool_on = true,
                    5 => state.tool_on = false,
                    _ => {}
                },
                'X' => target[0] = Some(*value),
                'Y' => target[1] = Some(*value),
                'Z' => target[2] = Some(*value),
                'I' => offsets[0] = Some(*value),
                'J' => offsets[1] = Some(*value),
                'R' => radius = Some(*value),
                'F' => state.feed = *value * state.scale,
                'S' => state.s_value = *value,
                'A' => {
                    program.uses_rotary = true;
                    target_a = Some(*value);
                }
                'C' => {
                    program.uses_rotary = true;
                    target_c = Some(*value);
                }
                'B' => program.uses_rotary = true,
                'N' | 'P' | 'T' | 'K' | 'L' => {}
                other => push_warning(
                    &mut program,
                    line_index,
                    &format!("unsupported word {other}"),
                ),
            }
        }
        if skip_motion
            || (target.iter().all(Option::is_none) && target_a.is_none() && target_c.is_none())
        {
            continue;
        }
        let end_a = match target_a {
            Some(value) if state.absolute => value,
            Some(value) => state.a + value,
            None => state.a,
        };
        let a = [state.a, end_a];
        let end_c = match target_c {
            Some(value) if state.absolute => value,
            Some(value) => state.c + value,
            None => state.c,
        };
        let c = [state.c, end_c];
        let minutes = (state.inverse_time && state.motion != 0 && state.feed > 0.0)
            // G93: F is the inverse of the move duration in minutes.
            .then(|| state.scale / state.feed);
        let mut end = state.position;
        for axis in 0..3 {
            if let Some(value) = target[axis] {
                let value = value * state.scale;
                end[axis] = if state.absolute {
                    value
                } else {
                    state.position[axis] + value
                };
            }
        }
        let power = if state.tool_on { state.s_value } else { 0.0 };
        match state.motion {
            0 | 1 => {
                let kind = if state.motion == 0 {
                    MoveKind::Rapid
                } else {
                    MoveKind::Feed
                };
                program.motions.push(Motion {
                    kind,
                    from: state.position,
                    to: end,
                    feed: if kind == MoveKind::Rapid {
                        0.0
                    } else {
                        state.feed
                    },
                    power: if kind == MoveKind::Rapid { 0.0 } else { power },
                    line: line_index,
                    a,
                    c,
                    minutes,
                });
            }
            2 | 3 => {
                let clockwise = state.motion == 2;
                let start = Point::new(state.position[0], state.position[1]);
                let stop = Point::new(end[0], end[1]);
                let center = match (radius, offsets) {
                    (Some(radius), _) => {
                        arc_center_from_radius(start, stop, radius * state.scale, clockwise)
                    }
                    (None, [i, j]) => {
                        Some(start + Point::new(i.unwrap_or(0.0), j.unwrap_or(0.0)) * state.scale)
                    }
                };
                let Some(center) = center else {
                    push_warning(&mut program, line_index, "invalid arc");
                    continue;
                };
                let points = flatten_arc(start, stop, center, clockwise);
                let count = points.len() as f64;
                let mut from = state.position;
                let share = minutes.map(|minutes| minutes / count);
                for (index, point) in points.into_iter().enumerate() {
                    let z = state.position[2]
                        + (end[2] - state.position[2]) * (index as f64 + 1.0) / count;
                    let to = [point.x, point.y, z];
                    program.motions.push(Motion {
                        kind: MoveKind::Feed,
                        from,
                        to,
                        feed: state.feed,
                        power,
                        line: line_index,
                        a: [end_a; 2],
                        c: [end_c; 2],
                        minutes: share,
                    });
                    from = to;
                }
            }
            _ => {}
        }
        state.position = end;
        state.a = end_a;
        state.c = end_c;
    }
    program.z_range = program
        .motions
        .iter()
        .fold((0.0f64, 0.0f64), |(low, high), motion| {
            (low.min(motion.to[2]), high.max(motion.to[2]))
        });
    program
}

fn push_warning(program: &mut Program, line_index: usize, message: &str) {
    if program.warnings.len() < 100 {
        program
            .warnings
            .push(format!("line {}: {message}", line_index + 1));
    }
}

fn tokenize(line: &str) -> Result<Vec<(char, f64)>, String> {
    let mut words = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            ';' | '%' => break,
            '(' => {
                for inner in chars.by_ref() {
                    if inner == ')' {
                        break;
                    }
                }
            }
            '$' | '[' => return Ok(Vec::new()),
            c if c.is_whitespace() => {}
            c if c.is_ascii_alphabetic() => {
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
                let value = number
                    .parse::<f64>()
                    .map_err(|_| format!("invalid number after {c}"))?;
                words.push((c.to_ascii_uppercase(), value));
            }
            other => return Err(format!("unexpected character '{other}'")),
        }
    }
    Ok(words)
}

fn arc_center_from_radius(
    start: Point,
    stop: Point,
    radius: f64,
    clockwise: bool,
) -> Option<Point> {
    let chord = stop - start;
    let half = chord.length() / 2.0;
    if half < 1e-9 || radius.abs() < half - 1e-6 {
        return None;
    }
    let height = (radius * radius - half * half).max(0.0).sqrt();
    let middle = start.lerp(stop, 0.5);
    let normal = chord.normalized().perpendicular();
    // GRBL: a negative radius selects the arc longer than 180 degrees.
    let left = clockwise == (radius < 0.0);
    Some(if left {
        middle + normal * height
    } else {
        middle - normal * height
    })
}

fn flatten_arc(start: Point, stop: Point, center: Point, clockwise: bool) -> Vec<Point> {
    let radius = start.distance(center);
    let start_angle = (start.y - center.y).atan2(start.x - center.x);
    let mut stop_angle = (stop.y - center.y).atan2(stop.x - center.x);
    let full = std::f64::consts::TAU;
    if clockwise {
        if stop_angle >= start_angle - 1e-9 {
            stop_angle -= full;
        }
    } else if stop_angle <= start_angle + 1e-9 {
        stop_angle += full;
    }
    let sweep = stop_angle - start_angle;
    let count = arc_segments(radius, sweep, ARC_TOLERANCE);
    let mut points: Vec<Point> = (1..count)
        .map(|index| {
            let angle = start_angle + sweep * index as f64 / count as f64;
            center + Point::new(angle.cos(), angle.sin()) * radius
        })
        .collect();
    points.push(stop);
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_moves_and_tool_state() {
        let program = parse("G91\nG0 X10 Y5\nM4 S500\nG1 X5 F1200\nM5\nG1 X5");
        assert_eq!(program.motions.len(), 3);
        assert_eq!(program.motions[1].to, [15.0, 5.0, 0.0]);
        assert!(program.motions[1].is_cutting());
        assert!(!program.motions[2].is_cutting());
    }

    #[test]
    fn arc_with_ij_ends_on_target_and_stays_on_circle() {
        let program = parse("G0 X10 Y0\nG3 X-10 Y0 I-10 J0 F600");
        let last = program.motions.last().unwrap();
        assert!((last.to[0] + 10.0).abs() < 1e-9 && last.to[1].abs() < 1e-9);
        assert!(program.motions.iter().skip(1).all(|motion| {
            ((motion.to[0].powi(2) + motion.to[1].powi(2)).sqrt() - 10.0).abs() < 1e-6
        }));
        assert!(
            program
                .motions
                .iter()
                .skip(1)
                .all(|motion| motion.to[1] >= -1e-9)
        );
    }

    #[test]
    fn maxmakelab_style_comments_and_streams_are_skipped() {
        let program = parse("; header\n[STREAM_START]\n(comment) G0 X1\n$H");
        assert_eq!(program.motions.len(), 1);
        assert!(program.warnings.is_empty());
    }

    #[test]
    fn rotary_words_and_z_range_are_reported() {
        let program = parse("G0 Z5\nG1 Z-1.5 F100\nG1 X1 A90");
        assert!(program.uses_rotary);
        assert_eq!(program.z_range, (-1.5, 5.0));
        assert!(!parse("G1 X1 F100").uses_rotary);
    }
}
