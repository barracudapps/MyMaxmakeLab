//! Resuming an interrupted job from a line: the machine state the
//! program had built up to that line (units, coordinates, spindle or
//! laser, coolant, feed, position) is restored by a short preamble,
//! then the program continues. A CNC goes up, across, starts the
//! spindle and plunges slowly back to depth; a laser moves there off.

use crate::Program;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResumeOptions {
    /// Clearance height for a CNC (work coordinates); `None` for a
    /// laser, which needs no Z moves to get there.
    pub safe_z: Option<f64>,
    /// Feed of the plunge back to depth, mm/min.
    pub plunge_feed: f64,
    /// Seconds to let the spindle reach its speed.
    pub spin_up: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeError {
    /// The line is past the end of the program.
    OutOfRange,
    /// Relative coordinates (G91) or inverse time (G93) were active:
    /// the position cannot be rebuilt reliably.
    Unsupported,
}

#[derive(Debug, Default, Clone)]
struct Modal {
    inches: bool,
    relative: bool,
    inverse_time: bool,
    coordinates: Option<String>,
    spindle: Option<&'static str>,
    speed: f64,
    coolant: Vec<&'static str>,
    feed: f64,
    z_seen: bool,
}

fn code(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_paren = false;
    for character in line.chars() {
        match character {
            ';' if !in_paren => break,
            '(' => in_paren = true,
            ')' => in_paren = false,
            c if !in_paren => out.push(c.to_ascii_uppercase()),
            _ => {}
        }
    }
    out
}

/// Words of a line as (letter, number).
fn words(line: &str) -> Vec<(char, f64)> {
    let text = code(line);
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(letter) = chars.next() {
        if !letter.is_ascii_alphabetic() {
            continue;
        }
        let mut number = String::new();
        while let Some(&next) = chars.peek() {
            if next.is_ascii_digit() || next == '.' || next == '-' || next == '+' || next == ' ' {
                if next != ' ' {
                    number.push(next);
                }
                chars.next();
            } else {
                break;
            }
        }
        if let Ok(value) = number.parse() {
            out.push((letter, value));
        }
    }
    out
}

fn scan(lines: &[&str]) -> Modal {
    let mut modal = Modal::default();
    for line in lines {
        for (letter, value) in words(line) {
            // G90.1 and the like are other modes: whole codes only.
            let whole = (value.fract() == 0.0).then_some(value as i64);
            match (letter, whole) {
                ('G', Some(20)) => modal.inches = true,
                ('G', Some(21)) => modal.inches = false,
                ('G', Some(90)) => modal.relative = false,
                ('G', Some(91)) => modal.relative = true,
                ('G', Some(93)) => modal.inverse_time = true,
                ('G', Some(94)) => modal.inverse_time = false,
                ('G', Some(code @ 54..=59)) => modal.coordinates = Some(format!("G{code}")),
                ('M', Some(3)) => modal.spindle = Some("M3"),
                ('M', Some(4)) => modal.spindle = Some("M4"),
                ('M', Some(5)) => modal.spindle = None,
                ('M', Some(7)) => modal.coolant.push("M7"),
                ('M', Some(8)) => modal.coolant.push("M8"),
                ('M', Some(9)) => modal.coolant.clear(),
                ('S', _) => modal.speed = value,
                ('F', _) => modal.feed = value,
                ('Z', _) => modal.z_seen = true,
                _ => {}
            }
        }
    }
    modal.coolant.dedup();
    modal
}

/// The program from source line `from` (0-based), preceded by what
/// restores the state it expects. `program` is `source` parsed.
pub fn resume(
    source: &str,
    program: &Program,
    from: usize,
    options: ResumeOptions,
) -> Result<String, ResumeError> {
    let lines: Vec<&str> = source.lines().collect();
    if from >= lines.len() {
        return Err(ResumeError::OutOfRange);
    }
    let modal = scan(&lines[..from]);
    if modal.relative || modal.inverse_time {
        return Err(ResumeError::Unsupported);
    }
    // Where the machine was when the line started: the start of its
    // first move, or the end of the last move before it.
    let start = program
        .motions
        .iter()
        .find(|motion| motion.line >= from)
        .map(|motion| (motion.from, motion.a[0]))
        .or_else(|| {
            program
                .motions
                .last()
                .map(|motion| (motion.to, motion.a[1]))
        })
        .unwrap_or(([0.0; 3], 0.0));
    let ([x, y, z], a) = start;
    let rotary = program.uses_rotary;
    let mut out = vec![format!("; Resumed at line {} of the program", from + 1)];
    // Firmware commands in brackets ([STREAM_START] on the HiMill) that
    // opened the program open the resumed one too.
    out.extend(
        lines[..from]
            .iter()
            .map(|line| line.trim())
            .filter(|line| line.starts_with('['))
            .map(str::to_owned),
    );
    out.push(if modal.inches { "G20" } else { "G21" }.to_owned());
    out.push("G90 G94".to_owned());
    if let Some(coordinates) = &modal.coordinates {
        out.push(coordinates.clone());
    }
    out.push("M5".to_owned());
    if let Some(safe_z) = options.safe_z {
        out.push(format!("G0 Z{:.3}", safe_z.max(z)));
    } else if modal.z_seen {
        out.push(format!("G0 Z{z:.3}"));
    }
    out.push(if rotary {
        format!("G0 X{x:.3} Y{y:.3} A{a:.3}")
    } else {
        format!("G0 X{x:.3} Y{y:.3}")
    });
    for coolant in &modal.coolant {
        out.push((*coolant).to_owned());
    }
    match (modal.spindle, options.safe_z) {
        // Laser in dynamic mode: power comes with each move.
        (Some("M4"), None) => out.push("M4 S0".to_owned()),
        (Some(spindle), None) => out.push(format!("{spindle} S{:.0}", modal.speed)),
        (Some(spindle), Some(_)) => {
            out.push(format!("{spindle} S{:.0}", modal.speed));
            out.push(format!("G4 P{:.1}", options.spin_up));
        }
        (None, _) => {}
    }
    if options.safe_z.is_some() {
        out.push(format!("G1 Z{z:.3} F{:.0}", options.plunge_feed.max(1.0)));
    }
    if modal.feed > 0.0 {
        out.push(format!("F{:.0}", modal.feed));
    }
    out.extend(lines[from..].iter().map(|line| (*line).to_owned()));
    Ok(out.join("\n") + "\n")
}

/// Source line to resume from after a stop at `position` (work
/// coordinates), knowing the controller accepted lines up to
/// `accepted` (it plans ahead, so the head was behind that): the
/// accepted move that passes nearest to where the head stopped.
pub fn resume_line(program: &Program, accepted: usize, position: [f64; 3]) -> Option<usize> {
    let distance = |motion: &crate::Motion| {
        let [ax, ay, az] = motion.from;
        let [bx, by, bz] = motion.to;
        let (dx, dy, dz) = (bx - ax, by - ay, bz - az);
        let length = dx * dx + dy * dy + dz * dz;
        let t = if length > 0.0 {
            (((position[0] - ax) * dx + (position[1] - ay) * dy + (position[2] - az) * dz) / length)
                .clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (px, py, pz) = (ax + t * dx, ay + t * dy, az + t * dz);
        ((px - position[0]).powi(2) + (py - position[1]).powi(2) + (pz - position[2]).powi(2))
            .sqrt()
    };
    program
        .motions
        .iter()
        .filter(|motion| motion.line <= accepted)
        // The latest of the closest moves: paths cross themselves.
        .min_by(|a, b| {
            distance(a)
                .total_cmp(&distance(b))
                .then(b.line.cmp(&a.line))
        })
        .map(|motion| motion.line)
}

/// Source line of the `count`-th line the controller received (it gets
/// no comments or blank lines), for mapping an acknowledgement count.
pub fn source_line_of(source: &str, count: usize) -> usize {
    source
        .lines()
        .enumerate()
        .filter(|(_, line)| !code(line).trim().is_empty())
        .nth(count.saturating_sub(1))
        .map_or(0, |(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CNC: ResumeOptions = ResumeOptions {
        safe_z: Some(5.0),
        plunge_feed: 100.0,
        spin_up: 3.0,
    };

    const PROGRAM: &str = "\
G21 G90
G54
M3 S12000
M8
G0 Z5
G0 X10 Y10
G1 Z-2 F200
G1 X50 F800
G1 Y40
G1 X10
G0 Z5
M5
";

    #[test]
    fn a_cnc_goes_up_across_spins_and_plunges_back() {
        let program = crate::parse(PROGRAM);
        // Resume at "G1 Y40": the head was at X50 Y10 Z-2.
        let text = resume(PROGRAM, &program, 8, CNC).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        let expected = [
            "G21",
            "G90 G94",
            "G54",
            "M5",
            "G0 Z5.000",
            "G0 X50.000 Y10.000",
            "M8",
            "M3 S12000",
            "G4 P3.0",
            "G1 Z-2.000 F100",
            "F800",
            "G1 Y40",
        ];
        assert_eq!(&lines[1..13], &expected);
        assert!(text.ends_with("M5\n"));
    }

    #[test]
    fn a_laser_moves_there_with_the_beam_off() {
        let laser = "G21 G90\nM4 S0\nG0 X0 Y0\nG1 X20 S800 F3000\nG1 Y20 S800\nM5\n";
        let program = crate::parse(laser);
        let text = resume(
            laser,
            &program,
            4,
            ResumeOptions {
                safe_z: None,
                plunge_feed: 0.0,
                spin_up: 0.0,
            },
        )
        .unwrap();
        assert!(
            text.contains("M5\nG0 X20.000 Y0.000\nM4 S0\nF3000\nG1 Y20 S800"),
            "{text}"
        );
        assert!(!text.contains("G0 Z"));
    }

    #[test]
    fn relative_programs_are_refused() {
        let relative = "G91\nG1 X10 F100\nG1 X10\n";
        let program = crate::parse(relative);
        assert_eq!(
            resume(relative, &program, 2, CNC),
            Err(ResumeError::Unsupported)
        );
        assert_eq!(
            resume(relative, &program, 99, CNC),
            Err(ResumeError::OutOfRange)
        );
    }

    #[test]
    fn the_resume_line_is_where_the_head_stopped_not_where_planning_was() {
        let program = crate::parse(PROGRAM);
        // The controller had accepted up to "G1 X10" (line 9) while the
        // head was still on "G1 Y40", at X50 Y25.
        let line = resume_line(&program, 9, [50.0, 25.0, -2.0]).unwrap();
        assert_eq!(line, 8);
        // Acknowledgement counts skip blank lines and comments.
        let source = "; header\nG21\n\nG0 X1 ; move\nG0 X2\n";
        assert_eq!(source_line_of(source, 2), 3);
    }

    #[test]
    fn firmware_framing_opens_the_resumed_program() {
        let framed = format!("[STREAM_START]\n{PROGRAM}[STREAM_END]\n");
        let program = crate::parse(&framed);
        let text = resume(&framed, &program, 9, CNC).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[1], "[STREAM_START]");
        assert_eq!(lines[2], "G21");
        assert!(text.ends_with("[STREAM_END]\n"));
    }
}
