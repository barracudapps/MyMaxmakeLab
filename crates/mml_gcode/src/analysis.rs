use crate::parser::{MoveKind, Program};
use mml_core::geometry::Rect;
use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub struct MotionSettings {
    /// mm/min used for G0 moves (GRBL `$110`).
    pub rapid_rate: f64,
    /// mm/s² (GRBL `$120`).
    pub acceleration: f64,
}

impl Default for MotionSettings {
    fn default() -> Self {
        Self {
            rapid_rate: 5000.0,
            acceleration: 1000.0,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct JobStats {
    pub cut_bounds: Rect,
    pub cut_length: f64,
    pub travel_length: f64,
    pub duration: Duration,
    pub line_count: usize,
}

/// Duration estimate: consecutive moves of the same kind running in
/// nearly the same direction form one run that accelerates once; every
/// sharp corner or kind change costs a full stop. This keeps raster jobs
/// (thousands of colinear micro-moves) realistic.
pub fn analyze(program: &Program, settings: MotionSettings) -> JobStats {
    let mut stats = JobStats {
        cut_bounds: program.cut_bounds(),
        line_count: program.line_count,
        ..JobStats::default()
    };
    let acceleration = settings.acceleration.max(1.0);
    let mut seconds = 0.0;
    let mut run_length = 0.0;
    let mut run_speed: f64 = 0.0;
    let mut previous: Option<(MoveKind, [f64; 2])> = None;

    let close_run = |length: f64, speed: f64| -> f64 {
        if length <= 0.0 || speed <= 0.0 {
            return 0.0;
        }
        let ramp = speed * speed / acceleration;
        if length >= ramp {
            length / speed + speed / acceleration
        } else {
            2.0 * (length / acceleration).sqrt()
        }
    };

    let rotary_radius = program.rotary_diameter.map(|diameter| diameter / 2.0);
    for motion in &program.motions {
        let length = motion.path_length(rotary_radius);
        if let Some(minutes) = motion.minutes {
            // Inverse time: the duration is programmed, rotation included.
            seconds += close_run(run_length, run_speed) + minutes * 60.0;
            run_length = 0.0;
            run_speed = 0.0;
            previous = None;
            if motion.power > 0.0 {
                stats.cut_length += length;
            }
            continue;
        }
        if length <= 0.0 {
            continue;
        }
        match motion.kind {
            MoveKind::Rapid => stats.travel_length += length,
            MoveKind::Feed if motion.power > 0.0 => stats.cut_length += length,
            MoveKind::Feed => stats.travel_length += length,
        }
        let rate = match motion.kind {
            MoveKind::Rapid => settings.rapid_rate,
            MoveKind::Feed => motion
                .feed
                .max(1.0)
                .min(settings.rapid_rate.max(motion.feed)),
        } / 60.0;
        let direction = [
            (motion.to[0] - motion.from[0]) / length,
            (motion.to[1] - motion.from[1]) / length,
        ];
        let continuous = previous.is_some_and(|(kind, last)| {
            kind == motion.kind && last[0] * direction[0] + last[1] * direction[1] > 0.94
        });
        if !continuous {
            seconds += close_run(run_length, run_speed);
            run_length = 0.0;
            run_speed = 0.0;
        }
        run_length += length;
        run_speed = run_speed.max(rate);
        previous = Some((motion.kind, direction));
    }
    seconds += close_run(run_length, run_speed);
    stats.duration = Duration::from_secs_f64(seconds);
    stats
}

/// Time and cut length of each tool of a program, in order of first
/// use. Tools are named by `; Tool <name>` comments (written by this
/// application for each layer), else by their `T` number; a program
/// without either has a single "Program" entry.
pub fn tool_breakdown(
    source: &str,
    program: &Program,
    settings: MotionSettings,
) -> Vec<(String, f64, f64)> {
    let mut label_of_line = Vec::with_capacity(program.line_count);
    let mut current = String::from("Program");
    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(name) = trimmed.strip_prefix("; Tool ") {
            current = name.trim().to_owned();
        } else if let Some(index) = trimmed.to_uppercase().find('T')
            && !trimmed.starts_with(';')
            && !trimmed.starts_with('(')
            && trimmed.to_uppercase().contains("M6")
        {
            let number: String = trimmed[index + 1..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            if !number.is_empty() {
                current = format!("T{number}");
            }
        }
        label_of_line.push(current.clone());
    }
    let mut out: Vec<(String, Program)> = Vec::new();
    for motion in &program.motions {
        let label = label_of_line
            .get(motion.line)
            .cloned()
            .unwrap_or_else(|| current.clone());
        match out.iter_mut().find(|(known, _)| *known == label) {
            Some((_, part)) => part.motions.push(*motion),
            None => out.push((
                label,
                Program {
                    motions: vec![*motion],
                    ..Program::default()
                },
            )),
        }
    }
    out.into_iter()
        .map(|(label, part)| {
            let stats = analyze(&part, settings);
            (label, stats.duration.as_secs_f64(), stats.cut_length)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    #[test]
    fn colinear_moves_count_as_one_acceleration() {
        let settings = MotionSettings {
            rapid_rate: 6000.0,
            acceleration: 100.0,
        };
        let split = analyze(&parse("M3 S1\nG1 X10 F600\nX20\nX30\nX40"), settings);
        let whole = analyze(&parse("M3 S1\nG1 X40 F600"), settings);
        assert!((split.duration.as_secs_f64() - whole.duration.as_secs_f64()).abs() < 1e-9);
        assert!((whole.cut_length - 40.0).abs() < 1e-9);
        assert!((whole.duration.as_secs_f64() - 4.1).abs() < 1e-9);
    }

    #[test]
    fn inverse_time_moves_last_what_they_program() {
        let settings = MotionSettings {
            rapid_rate: 6000.0,
            acceleration: 100.0,
        };
        // 2 moves of half a minute each, one of them a pure rotation.
        let program = parse("M3 S1000\nG93 G1 X10 A90 F2\nA180 F2\nG94");
        assert!(program.uses_rotary);
        assert_eq!(program.motions[1].a, [90.0, 180.0]);
        let stats = analyze(&program, settings);
        assert!((stats.duration.as_secs_f64() - 60.0).abs() < 1e-9);
        // A quarter turn on a 40 mm stock sweeps a 31.4 mm arc.
        let wrapped = parse("; Rotary stock diameter 40 mm\nM3 S1\nG1 A90 F600");
        let stats = analyze(&wrapped, settings);
        assert!((stats.cut_length - 10.0 * std::f64::consts::PI).abs() < 0.05);
    }

    #[test]
    fn time_is_split_by_tool() {
        let settings = MotionSettings {
            rapid_rate: 6000.0,
            acceleration: 100.0,
        };
        let source = "; Tool Laser\nM4 S500\nG1 X10 F600\n; Tool End mill Ø3.175\nM3 S9000\nG1 X30 F600\nM6 T2\nG1 X40\n";
        let program = parse(source);
        let parts = tool_breakdown(source, &program, settings);
        let labels: Vec<&str> = parts.iter().map(|(label, ..)| label.as_str()).collect();
        assert_eq!(labels, ["Laser", "End mill Ø3.175", "T2"]);
        assert!((parts[1].2 - 20.0).abs() < 1e-9);
        let total: f64 = parts.iter().map(|(_, seconds, _)| seconds).sum();
        assert!(total > 0.0);
    }
}
