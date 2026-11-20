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

    for motion in &program.motions {
        let length = motion.length();
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
}
