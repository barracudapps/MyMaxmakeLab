//! Repairs and lightens G-code files: comments, repeated modal words,
//! zero-length moves, useless decimals and colinear micro-segments,
//! without changing the path the machine follows.

use crate::parser::parse;
use std::fmt::Write;

#[derive(Debug, Clone, Copy)]
pub struct OptimizeOptions {
    pub strip_comments: bool,
    pub drop_redundant_words: bool,
    pub remove_zero_moves: bool,
    pub merge_colinear: bool,
    /// Largest deviation allowed when merging, mm.
    pub tolerance: f64,
    pub decimals: usize,
    /// Feed inserted before the first cutting move when none was set.
    pub default_feed: Option<f64>,
}

impl Default for OptimizeOptions {
    fn default() -> Self {
        Self {
            strip_comments: true,
            drop_redundant_words: true,
            remove_zero_moves: true,
            merge_colinear: true,
            tolerance: 0.005,
            decimals: 3,
            default_feed: Some(600.0),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct OptimizeReport {
    pub lines_before: usize,
    pub lines_after: usize,
    pub bytes_before: usize,
    pub bytes_after: usize,
    pub comments_removed: usize,
    pub words_removed: usize,
    pub zero_moves_removed: usize,
    pub segments_merged: usize,
    pub feed_inserted: bool,
    pub relative_mode: bool,
    pub inches: bool,
    pub warnings: Vec<String>,
}

#[derive(Clone, Copy)]
struct Segment {
    start: [f64; 3],
    end: [f64; 3],
}

struct Optimizer {
    options: OptimizeOptions,
    report: OptimizeReport,
    out: String,
    absolute: bool,
    /// G93: every move carries its own F, nothing may be merged.
    inverse_time: bool,
    motion: Option<u8>,
    emitted_motion: Option<u8>,
    feed: Option<f64>,
    power: Option<f64>,
    position: [f64; 3],
    pending: Option<Segment>,
    feed_needed: bool,
}

fn tokenize(code: &str) -> Option<Vec<(char, f64)>> {
    let mut words = Vec::new();
    let mut chars = code.chars().peekable();
    while let Some(character) = chars.next() {
        if character.is_whitespace() {
            continue;
        }
        if !character.is_ascii_alphabetic() {
            return None;
        }
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
        words.push((character.to_ascii_uppercase(), number.parse().ok()?));
    }
    Some(words)
}

fn split_comment(line: &str) -> (String, bool) {
    let mut code = String::with_capacity(line.len());
    let mut had_comment = false;
    let mut in_paren = false;
    for character in line.chars() {
        match character {
            ';' if !in_paren => {
                had_comment = true;
                break;
            }
            '(' => {
                in_paren = true;
                had_comment = true;
            }
            ')' => in_paren = false,
            c if !in_paren => code.push(c),
            _ => {}
        }
    }
    (code.trim().to_owned(), had_comment)
}

impl Optimizer {
    fn number(&self, value: f64) -> String {
        let text = format!("{value:.precision$}", precision = self.options.decimals);
        let text = if text.contains('.') {
            text.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            text
        };
        if text == "-0" { "0".into() } else { text }
    }

    fn flush(&mut self) {
        let Some(segment) = self.pending.take() else {
            return;
        };
        let mut line = String::new();
        if self.emitted_motion != Some(1) || !self.options.drop_redundant_words {
            line.push_str("G1");
            self.emitted_motion = Some(1);
        }
        for (axis, letter) in ['X', 'Y', 'Z'].into_iter().enumerate() {
            if (segment.end[axis] - segment.start[axis]).abs() > 1e-9
                || !self.options.drop_redundant_words
            {
                let _ = write!(line, "{letter}{}", self.number(segment.end[axis]));
            }
        }
        self.out.push_str(&line);
        self.out.push('\n');
    }

    fn emit(&mut self, words: &[(char, f64)]) {
        let mut line = String::new();
        for (letter, value) in words {
            let text =
                if matches!(letter, 'G' | 'M' | 'T' | 'N' | 'P' | 'L') && value.fract() == 0.0 {
                    format!("{}", *value as i64)
                } else {
                    self.number(*value)
                };
            let _ = write!(line, "{letter}{text}");
        }
        if !line.is_empty() {
            self.out.push_str(&line);
            self.out.push('\n');
        }
    }

    fn line(&mut self, raw: &str) {
        let (code, had_comment) = split_comment(raw);
        if had_comment && self.options.strip_comments {
            self.report.comments_removed += 1;
        }
        if code.is_empty() {
            if !self.options.strip_comments && !raw.trim().is_empty() {
                self.flush();
                self.out.push_str(raw.trim());
                self.out.push('\n');
            }
            return;
        }
        let Some(mut words) = tokenize(&code) else {
            // $ commands, % markers, [STREAM] tags: kept verbatim.
            self.flush();
            self.out.push_str(&code);
            self.out.push('\n');
            return;
        };
        let mut motion_word = None;
        let mut has_tool_command = false;
        for (letter, value) in &words {
            match (letter, (value * 10.0).round() as i64) {
                ('G', 0 | 10 | 20 | 30) => motion_word = Some(value.round() as u8),
                ('G', 900) => self.absolute = true,
                ('G', 910) => {
                    self.absolute = false;
                    self.report.relative_mode = true;
                }
                ('G', 200) => self.report.inches = true,
                ('G', 930) => self.inverse_time = true,
                ('G', 940) => self.inverse_time = false,
                ('M', _) => has_tool_command = true,
                _ => {}
            }
        }
        if let Some(code) = motion_word {
            self.motion = Some(code);
        }
        let has_axis = words
            .iter()
            .any(|(letter, _)| matches!(letter, 'X' | 'Y' | 'Z'));
        let motion = self.motion.unwrap_or(0);
        let simple = self.absolute
            && !self.inverse_time
            && !self.report.inches
            && has_axis
            && matches!(motion, 0 | 1);

        if simple {
            let mut target = self.position;
            for (letter, value) in &words {
                match letter {
                    'X' => target[0] = *value,
                    'Y' => target[1] = *value,
                    'Z' => target[2] = *value,
                    _ => {}
                }
            }
            let new_feed = words
                .iter()
                .find(|(letter, _)| *letter == 'F')
                .map(|(_, value)| *value);
            let new_power = words
                .iter()
                .find(|(letter, _)| *letter == 'S')
                .map(|(_, value)| *value);
            if motion == 1 && new_feed.is_none() && self.feed.is_none() {
                self.feed_needed = true;
            }
            let feed_changes = new_feed.is_some_and(|feed| Some(feed) != self.feed);
            let power_changes = new_power.is_some_and(|power| Some(power) != self.power);
            let only_motion = words
                .iter()
                .all(|(letter, _)| matches!(letter, 'G' | 'X' | 'Y' | 'Z' | 'F' | 'S'));
            let zero = (0..3).all(|axis| (target[axis] - self.position[axis]).abs() < 1e-9);
            if zero
                && self.options.remove_zero_moves
                && only_motion
                && !feed_changes
                && !power_changes
            {
                self.report.zero_moves_removed += 1;
                return;
            }
            let mergeable = self.options.merge_colinear
                && motion == 1
                && only_motion
                && !feed_changes
                && !power_changes;
            if mergeable {
                let start = self.position;
                self.position = target;
                if let Some(pending) = &mut self.pending
                    && colinear(pending.start, pending.end, target, self.options.tolerance)
                {
                    pending.end = target;
                    self.report.segments_merged += 1;
                    return;
                }
                self.flush();
                self.pending = Some(Segment { start, end: target });
                return;
            }
            self.flush();
            if self.options.drop_redundant_words {
                let before = words.len();
                let position = self.position;
                let emitted = self.emitted_motion;
                let feed = self.feed;
                let power = self.power;
                words.retain(|(letter, value)| match letter {
                    'G' if motion_word.is_some() && (value.round() as u8) == motion => {
                        Some(motion) != emitted
                    }
                    'X' => (value - position[0]).abs() > 1e-9,
                    'Y' => (value - position[1]).abs() > 1e-9,
                    'Z' => (value - position[2]).abs() > 1e-9,
                    'F' => Some(*value) != feed,
                    'S' => has_tool_command || Some(*value) != power,
                    _ => true,
                });
                if !words
                    .iter()
                    .any(|(letter, value)| *letter == 'G' && (value.round() as u8) == motion)
                    && Some(motion) != emitted
                {
                    words.insert(0, ('G', motion as f64));
                }
                self.report.words_removed += before.saturating_sub(words.len());
            }
            self.position = target;
            self.emitted_motion = Some(motion);
            if let Some(feed) = new_feed {
                self.feed = Some(feed);
            }
            if let Some(power) = new_power {
                self.power = Some(power);
            }
            self.emit(&words);
            return;
        }

        self.flush();
        for (letter, value) in &words {
            match letter {
                'F' => self.feed = Some(*value),
                'S' => self.power = Some(*value),
                _ => {}
            }
        }
        if has_axis {
            // Arcs, relative or inch moves: kept, position tracked when
            // absolute so later moves stay correct.
            if self.absolute && !self.report.inches {
                for (letter, value) in &words {
                    match letter {
                        'X' => self.position[0] = *value,
                        'Y' => self.position[1] = *value,
                        'Z' => self.position[2] = *value,
                        _ => {}
                    }
                }
            }
            if motion_word.is_some() || has_axis {
                self.emitted_motion = Some(motion);
            }
        }
        self.emit(&words);
    }
}

fn colinear(start: [f64; 3], middle: [f64; 3], end: [f64; 3], tolerance: f64) -> bool {
    let along = [0, 1, 2].map(|axis| end[axis] - start[axis]);
    let length = (along[0] * along[0] + along[1] * along[1] + along[2] * along[2]).sqrt();
    if length < 1e-9 {
        return false;
    }
    let offset = [0, 1, 2].map(|axis| middle[axis] - start[axis]);
    let projection = (offset[0] * along[0] + offset[1] * along[1] + offset[2] * along[2]) / length;
    if projection <= 0.0 || projection >= length {
        return false;
    }
    let cross = [
        offset[1] * along[2] - offset[2] * along[1],
        offset[2] * along[0] - offset[0] * along[2],
        offset[0] * along[1] - offset[1] * along[0],
    ];
    (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt() / length <= tolerance
}

pub fn optimize(source: &str, options: OptimizeOptions) -> (String, OptimizeReport) {
    let mut optimizer = Optimizer {
        options,
        report: OptimizeReport {
            lines_before: source.lines().count(),
            bytes_before: source.len(),
            ..OptimizeReport::default()
        },
        out: String::with_capacity(source.len()),
        absolute: true,
        inverse_time: false,
        motion: None,
        emitted_motion: None,
        feed: None,
        power: None,
        position: [0.0; 3],
        pending: None,
        feed_needed: false,
    };
    for line in source.lines() {
        optimizer.line(line);
    }
    optimizer.flush();
    let mut out = optimizer.out;
    let mut report = optimizer.report;
    if optimizer.feed_needed
        && let Some(feed) = options.default_feed
    {
        out.insert_str(0, &format!("F{feed:.0}\n"));
        report.feed_inserted = true;
    }
    report.warnings = parse(source).warnings;
    report.lines_after = out.lines().count();
    report.bytes_after = out.len();
    (out, report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{MotionSettings, analyze};

    #[test]
    fn lighter_program_follows_the_same_path() {
        let source = "; header\nG90 G21\nG0 X0 Y0 (start)\nM3 S500\nG1 X1 Y0 F600 S500\nG1 X2 Y0 F600 S500\nG1 X3 Y0\nG1 X3 Y0\nG1 X3 Y4\nG0 X0 Y0\nM5\n";
        let (out, report) = optimize(source, OptimizeOptions::default());
        assert_eq!(report.comments_removed, 2);
        assert_eq!(
            report.zero_moves_removed, 2,
            "G0 to the start position and the repeated X3 Y0"
        );
        assert_eq!(
            report.segments_merged, 1,
            "X2 and X3 join; X1 sets the feed and stays"
        );
        assert!(report.bytes_after < report.bytes_before);
        let (before, after) = (parse(source), parse(&out));
        let settings = MotionSettings::default();
        assert!(
            (analyze(&before, settings).cut_length - analyze(&after, settings).cut_length).abs()
                < 1e-9
        );
        assert_eq!(before.cut_bounds(), after.cut_bounds());
        assert_eq!(after.motions.last().unwrap().to, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn inverse_time_moves_keep_their_feed_and_rotation() {
        let source = "M3 S9000\nG93\nG1 X1 A10 F120\nG1 X2 A20 F120\nG1 X3 F120\nG94\nG1 X4 F600\n";
        let (out, _) = optimize(source, OptimizeOptions::default());
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines.contains(&"G1X2A20F120"), "{out}");
        assert!(
            lines
                .iter()
                .any(|line| *line == "G1X3F120" || *line == "X3F120"),
            "{out}"
        );
        assert_eq!(out.matches("F120").count(), 3);
    }

    #[test]
    fn missing_feed_is_reported_and_inserted() {
        let (out, report) = optimize("G1 X10\n", OptimizeOptions::default());
        assert!(report.feed_inserted);
        assert!(out.starts_with("F600\n"));
    }

    #[test]
    fn relative_programs_are_not_merged() {
        let source = "G91\nG1 X1 F100\nG1 X1\n";
        let (out, report) = optimize(source, OptimizeOptions::default());
        assert!(report.relative_mode);
        assert_eq!(report.segments_merged, 0);
        assert_eq!(parse(&out).motions.last().unwrap().to[0], 2.0);
    }
}
