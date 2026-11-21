//! Where a jog may go, in machine coordinates. GRBL puts its machine
//! space on one side of the home position: below it for an axis that
//! homes towards its maximum (the default, space from -travel to 0),
//! above it for an axis that homes towards its minimum (`$23` bit set).
//! Some firmwares set the home position to another value; working from
//! the position really reported after homing covers them all.

/// Keeps this far from the travel ends, mm: the homing pull-off.
const MARGIN: f64 = 0.5;

/// Allowed range of each axis (X, Y, Z) in machine coordinates, from
/// the machine position read right after homing, the travel of each
/// axis ($130 to $132) and the homing direction mask ($23).
pub fn limits(home: [f64; 3], travel: [f64; 3], direction_mask: u32) -> [(f64, f64); 3] {
    std::array::from_fn(|axis| {
        let towards_minimum = direction_mask & (1 << axis) != 0;
        let travel = travel[axis].max(0.0);
        if towards_minimum {
            (home[axis] + MARGIN, home[axis] + travel - MARGIN)
        } else {
            (home[axis] - travel + MARGIN, home[axis] - MARGIN)
        }
    })
}

/// Shortens a relative jog so it stays within `limits`; never turns it
/// around or makes it longer. Without limits (no homing yet) the move is
/// only capped to one travel length per axis, so a held key cannot
/// send the head further than the machine is long.
pub fn clamp(
    position: [f64; 3],
    delta: [f64; 3],
    limits: Option<[(f64, f64); 3]>,
    travel: [f64; 3],
) -> [f64; 3] {
    std::array::from_fn(|axis| {
        let wanted = delta[axis];
        let capped = if travel[axis] > 0.0 {
            wanted.clamp(-travel[axis], travel[axis])
        } else {
            wanted
        };
        let Some(limits) = limits else {
            return capped;
        };
        let (low, high) = limits[axis];
        if high < low {
            return capped;
        }
        // Up to the limit in the direction of the move; nothing when the
        // head is already at or past it. Moving back inside from beyond
        // a limit is allowed.
        if capped > 0.0 {
            if position[axis] >= high {
                0.0
            } else {
                (position[axis] + capped).min(high) - position[axis]
            }
        } else if capped < 0.0 {
            if position[axis] <= low {
                0.0
            } else {
                (position[axis] + capped).max(low) - position[axis]
            }
        } else {
            0.0
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRAVEL: [f64; 3] = [280.0, 200.0, 80.0];

    #[test]
    fn himill_space_after_homing_left_and_table_forward() {
        // X homes to its minimum (left), Y and Z to their maximum: GRBL
        // reports X at -279, Y and Z near 0.
        let home = [-279.0, -1.0, -1.0];
        let limits = limits(home, TRAVEL, 0b001);
        assert_eq!(limits[0], (-278.5, 0.5));
        assert_eq!(limits[1], (-200.5, -1.5));
        // Bringing the head forward (-Y) by 10 mm moves 10 mm forward.
        let moved = clamp(home, [0.0, -10.0, 0.0], Some(limits), TRAVEL);
        assert!((moved[1] + 10.0).abs() < 1e-9, "{moved:?}");
        // 10 mm to the right is 10 mm, never the whole width.
        let moved = clamp(home, [10.0, 0.0, 0.0], Some(limits), TRAVEL);
        assert!((moved[0] - 10.0).abs() < 1e-9, "{moved:?}");
    }

    #[test]
    fn jogs_stop_at_the_ends_and_are_never_reversed() {
        let limits = Some([(-279.5, -0.5), (-199.5, -0.5), (-79.5, -0.5)]);
        let at = [-5.0, -100.0, -2.0];
        let moved = clamp(at, [10_000.0, 0.0, 0.0], limits, TRAVEL);
        assert!((moved[0] - 4.5).abs() < 1e-9);
        // Already at the end: nothing, not a move the other way.
        let moved = clamp([-0.5, -100.0, -2.0], [5.0, 0.0, 0.0], limits, TRAVEL);
        assert_eq!(moved[0], 0.0);
        // Outside (after unlocking far out): moving back in is allowed.
        let moved = clamp([2.0, -100.0, -2.0], [-1.0, 0.0, 0.0], limits, TRAVEL);
        assert_eq!(moved[0], -1.0);
    }

    #[test]
    fn without_homing_only_the_length_is_capped() {
        let moved = clamp([0.0; 3], [10_000.0, -10.0, 0.0], None, TRAVEL);
        assert_eq!(moved, [280.0, -10.0, 0.0]);
    }
}
