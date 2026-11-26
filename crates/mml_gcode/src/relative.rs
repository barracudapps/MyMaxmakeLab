//! Rewrites an absolute program as moves relative to where the head
//! stands when it starts (`G91`), like the "current position" jobs of
//! MaxmakeLAB and LightBurn: the job needs no work zero and no `G92`,
//! whatever coordinates the machine uses, and nothing is left behind if
//! it stops early.
//!
//! The program states its start point in a comment
//! (`; Relative to the head from X.. Y..`), which [`crate::parse`] reads
//! back so previews and checks stay in design coordinates.

use std::fmt::Write as _;

/// Comment that marks a relative program and gives the design point the
/// head stands on at the start.
pub const START_COMMENT: &str = "; Relative to the head from ";

/// Rewrites `source` (absolute, `G90`) relative to `start`, the design
/// point the head marks. With `z_from_head` the head height is Z0;
/// otherwise the first Z move goes to its absolute height (`G90` for
/// that line only) and the following ones are relative to it.
///
/// Arcs keep their `I`/`J` words, which GRBL always reads relative to
/// the arc start. Each delta is rounded to the micrometre from the
/// position already reached, so rounding never accumulates.
pub fn to_relative(source: &str, start: [f64; 2], z_from_head: bool) -> String {
    let mut out = String::with_capacity(source.len() + 64);
    let _ = writeln!(
        out,
        "{START_COMMENT}X{} Y{}{}",
        format_value(start[0]),
        format_value(start[1]),
        if z_from_head { " Z0" } else { "" }
    );
    // Position in the program (absolute) and where the head really is.
    let mut program = [Some(start[0]), Some(start[1]), z_from_head.then_some(0.0)];
    let mut reached = [round(start[0]), round(start[1]), 0.0];
    let mut z_known = z_from_head;
    let mut source_absolute = true;
    for line in source.lines() {
        let Some(words) = words(line) else {
            out.push_str(line);
            out.push('\n');
            continue;
        };
        // Words in their order; `None` marks where an axis word goes.
        let mut kept: Vec<Option<String>> = Vec::new();
        let mut targets: [Option<f64>; 3] = [None; 3];
        let mut order: Vec<usize> = Vec::new();
        for (letter, text) in &words {
            match letter {
                'G' if is_code(text, 90) => {
                    source_absolute = true;
                    kept.push(Some("G91".into()));
                }
                'G' if is_code(text, 91) => {
                    source_absolute = false;
                    kept.push(Some("G91".into()));
                }
                'X' | 'Y' | 'Z' => {
                    let axis = (*letter as u8 - b'X') as usize;
                    let value: f64 = text.parse().unwrap_or(0.0);
                    let absolute = if source_absolute {
                        value
                    } else {
                        program[axis].unwrap_or(0.0) + value
                    };
                    targets[axis] = Some(absolute);
                    order.push(axis);
                    kept.push(None);
                }
                _ => kept.push(Some(format!("{letter}{text}"))),
            }
        }
        // The first absolute Z (the head height is not Z0): that height
        // in G90 on a line of its own, then back to relative moves.
        if let Some(z) = targets[2].filter(|_| !z_known) {
            let mut first = String::from("G90");
            for word in kept.iter().flatten() {
                if word.starts_with('G') && word != "G91" || word.starts_with('F') {
                    first.push_str(word);
                }
            }
            let _ = writeln!(first, "Z{}", format_value(z));
            out.push_str(&first);
            out.push_str("G91\n");
            program[2] = Some(z);
            reached[2] = round(z);
            z_known = true;
            targets[2] = None;
            let index = order.iter().position(|axis| *axis == 2).unwrap_or(0);
            order.remove(index);
            let mut seen = 0;
            kept.retain(|word| match word {
                None => {
                    seen += 1;
                    seen - 1 != index
                }
                Some(_) => true,
            });
            if order.is_empty() {
                continue;
            }
        }
        let mut deltas = order.iter().map(|axis| {
            let target = targets[*axis].unwrap_or(0.0);
            let delta = round(round(target) - reached[*axis]);
            reached[*axis] = round(reached[*axis] + delta);
            program[*axis] = Some(target);
            let letter = (b'X' + *axis as u8) as char;
            format!("{letter}{}", format_value(delta))
        });
        let mut line_out = String::new();
        for word in &kept {
            match word {
                Some(word) => line_out.push_str(word),
                None => line_out.push_str(&deltas.next().unwrap_or_default()),
            }
        }
        out.push_str(&line_out);
        out.push('\n');
    }
    out
}

/// Words of a G-code line as `(letter, number text)`, `None` for lines
/// to copy unchanged (comments, `$` and `[...]` commands, blank lines).
fn words(line: &str) -> Option<Vec<(char, String)>> {
    let trimmed = line.trim();
    if trimmed.is_empty()
        || trimmed.starts_with(';')
        || trimmed.starts_with('(')
        || trimmed.starts_with('$')
        || trimmed.starts_with('[')
        || trimmed.starts_with('%')
    {
        return None;
    }
    let code = trimmed.split(';').next().unwrap_or("");
    let mut words = Vec::new();
    let mut chars = code.chars().peekable();
    while let Some(character) = chars.next() {
        if character.is_ascii_alphabetic() {
            let mut number = String::new();
            while let Some(&next) = chars.peek() {
                if next.is_ascii_digit() || matches!(next, '.' | '-' | '+') {
                    number.push(next);
                    chars.next();
                } else if next == ' ' && number.is_empty() {
                    chars.next();
                } else {
                    break;
                }
            }
            words.push((character.to_ascii_uppercase(), number));
        } else if character == '(' {
            for inner in chars.by_ref() {
                if inner == ')' {
                    break;
                }
            }
        }
    }
    (!words.is_empty()).then_some(words)
}

fn is_code(text: &str, code: i64) -> bool {
    text.parse::<f64>()
        .is_ok_and(|value| (value * 10.0).round() as i64 == code * 10)
}

fn round(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

fn format_value(value: f64) -> String {
    let value = if value.abs() < 0.0005 { 0.0 } else { value };
    let text = format!("{value:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_become_deltas_from_the_head() {
        let source = "G90 G21 G17 G94\nM4 S0\nG0X12Y8\nG1X22F600S500\nY18\nG0X10Y5\nM5\n";
        let relative = to_relative(source, [10.0, 5.0], false);
        let lines: Vec<&str> = relative.lines().collect();
        assert_eq!(lines[0], "; Relative to the head from X10 Y5");
        assert_eq!(lines[1], "G91G21G17G94");
        assert_eq!(lines[2], "M4S0");
        assert_eq!(lines[3], "G0X2Y3");
        assert_eq!(lines[4], "G1X10F600S500");
        assert_eq!(lines[5], "Y10");
        assert_eq!(lines[6], "G0X-12Y-13");
        // Parsed back, the program is where the design is.
        let program = crate::parse(&relative);
        let last = program.motions.last().unwrap();
        assert!((last.to[0] - 10.0).abs() < 1e-9 && (last.to[1] - 5.0).abs() < 1e-9);
        assert!((program.motions[1].to[0] - 22.0).abs() < 1e-9);
    }

    #[test]
    fn rounding_never_drifts() {
        let mut source = String::from("G90\n");
        for step in 1..=3000 {
            let _ = writeln!(source, "G1X{:.4}", f64::from(step) * 0.0333);
        }
        source.push_str("G0X0\n");
        let relative = to_relative(&source, [0.0, 0.0], false);
        let total: f64 = relative
            .lines()
            .filter_map(|line| line.split('X').nth(1))
            .filter_map(|value| value.parse::<f64>().ok())
            .sum();
        assert!(total.abs() < 1e-6, "{total}");
    }

    #[test]
    fn the_first_z_is_absolute_unless_the_head_is_z0() {
        let source = "G90\nG0Z5\nG0X1Y1\nG1Z-0.5F150\nG0Z5\n";
        let relative = to_relative(source, [0.0, 0.0], false);
        let lines: Vec<&str> = relative.lines().collect();
        assert_eq!(lines[2], "G90G0Z5");
        assert_eq!(lines[3], "G91");
        assert_eq!(lines[5], "G1Z-5.5F150");
        assert_eq!(lines[6], "G0Z5.5");
        let from_head = to_relative(source, [0.0, 0.0], true);
        assert!(from_head.lines().any(|line| line == "G0Z5"));
        assert!(!from_head.contains("G90"));
        let program = crate::parse(&from_head);
        assert!((program.z_range.0 + 0.5).abs() < 1e-9);
    }

    #[test]
    fn comments_and_controller_commands_are_kept() {
        let source = "; Layer Cut\n[STREAM_START]\n$H\n\nG1X1(inline)\n";
        let relative = to_relative(source, [0.0, 0.0], false);
        assert!(relative.contains("; Layer Cut\n[STREAM_START]\n$H\n\n"));
        assert!(relative.ends_with("G1X1\n"), "{relative}");
    }
}
