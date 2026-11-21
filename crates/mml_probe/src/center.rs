//! Finding the centre of a hole or a pocket (probe inside) or of a boss
//! or a block (probe outside), one axis at a time: two opposite touches
//! give the centre and the size along that axis. GRBL has no variables,
//! so each axis is a separate program and the application moves to the
//! centre between them.

use crate::ProbeSettings;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CenterKind {
    /// Tool already inside the hole or pocket, below its rim.
    Inside,
    /// Tool above the middle of a boss or block: `size` is its rough
    /// width along the axis, `depth` how far below the start to touch.
    Outside { size: f64, depth: f64 },
}

/// Probing program for one axis (0 for X, 1 for Y). `start` is the
/// machine position the program begins at, where the outside cycle
/// returns (above the middle) between its two touches.
pub fn axis_program(
    axis: usize,
    kind: CenterKind,
    settings: &ProbeSettings,
    start: [f64; 3],
) -> Vec<String> {
    let letter = if axis == 0 { 'X' } else { 'Y' };
    let feed = settings.seek_feed;
    let back = settings.retract;
    let mut lines = vec!["G21 G91".to_owned()];
    match kind {
        CenterKind::Inside => {
            let travel = settings.max_travel;
            lines.extend([
                format!("G38.2 {letter}{travel:.3} F{feed:.0}"),
                format!("G0 {letter}-{back:.3}"),
                // Back across the hole to the other wall.
                format!("G38.2 {letter}-{:.3} F{feed:.0}", travel * 2.0),
                format!("G0 {letter}{back:.3}"),
            ]);
        }
        CenterKind::Outside { size, depth } => {
            let clearance = settings.clearance;
            let out = size / 2.0 + clearance;
            for side in [1.0, -1.0] {
                // Over the side, down, touch inwards, step back, up, and
                // back over the middle in machine coordinates.
                lines.extend([
                    format!("G0 {letter}{:.3}", side * out),
                    format!("G0 Z-{depth:.3}"),
                    format!("G38.2 {letter}{:.3} F{feed:.0}", -side * clearance * 2.0),
                    format!("G0 {letter}{:.3}", side * back),
                    format!("G0 Z{depth:.3}"),
                    "G90".to_owned(),
                    format!("G53 G0 X{:.3} Y{:.3}", start[0], start[1]),
                    "G91".to_owned(),
                ]);
            }
        }
    }
    lines.push("G90".into());
    lines
}

/// Centre and size along `axis` from the touches of a program, in
/// machine coordinates; the size includes the tool diameter (inside it
/// adds to the distance between touches, outside it subtracts).
pub fn axis_result(
    touches: &[[f64; 3]],
    axis: usize,
    kind: CenterKind,
    tool_diameter: f64,
) -> Option<(f64, f64)> {
    let [a, b] = touches else {
        return None;
    };
    let (low, high) = (a[axis].min(b[axis]), a[axis].max(b[axis]));
    let size = match kind {
        CenterKind::Inside => high - low + tool_diameter,
        CenterKind::Outside { .. } => high - low - tool_diameter,
    };
    (size > 0.0).then_some(((low + high) / 2.0, size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inside_probes_both_walls_and_returns() {
        let lines = axis_program(0, CenterKind::Inside, &ProbeSettings::default(), [0.0; 3]);
        let probes = lines
            .iter()
            .filter(|line| line.starts_with("G38.2 X"))
            .count();
        assert_eq!(probes, 2);
        assert_eq!(lines.last().unwrap(), "G90");
    }

    #[test]
    fn outside_lifts_before_crossing_over() {
        let lines = axis_program(
            1,
            CenterKind::Outside {
                size: 40.0,
                depth: 5.0,
            },
            &ProbeSettings::default(),
            [10.0, 20.0, -5.0],
        );
        // Every move along Y over the block happens after a lift.
        let probes: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.starts_with("G38.2 Y"))
            .map(|(index, _)| index)
            .collect();
        assert_eq!(probes.len(), 2);
        for index in probes {
            assert!(lines[index + 2].starts_with("G0 Z"), "{lines:?}");
            assert_eq!(lines[index + 4], "G53 G0 X10.000 Y20.000");
        }
        // The net Z movement is zero.
        let net: f64 = lines
            .iter()
            .filter_map(|line| line.strip_prefix("G0 Z"))
            .map(|value| value.parse::<f64>().unwrap())
            .sum();
        assert!(net.abs() < 1e-9);
    }

    #[test]
    fn centre_and_size_account_for_the_tool() {
        let touches = [[12.0, 0.0, 0.0], [32.0, 0.0, 0.0]];
        let (centre, size) = axis_result(&touches, 0, CenterKind::Inside, 2.0).unwrap();
        assert_eq!((centre, size), (22.0, 22.0));
        let outside = CenterKind::Outside {
            size: 20.0,
            depth: 3.0,
        };
        assert_eq!(axis_result(&touches, 0, outside, 2.0), Some((22.0, 18.0)));
        assert_eq!(axis_result(&touches[..1], 0, outside, 2.0), None);
    }
}
