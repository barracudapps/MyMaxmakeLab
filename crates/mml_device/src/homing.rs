//! Gentle, supervised homing.
//!
//! GRBL drives open-loop steppers: nothing in the controller feels a
//! hand or an object in the way, the motor pushes with its full torque
//! until it stalls or the axis reaches its switch. Three things limit
//! what an obstruction suffers:
//! - hold to run: homing only continues while the operator holds the
//!   button, releasing it resets the controller at once;
//! - a time limit taken from the usual homing duration, much shorter
//!   than the 1.5 × travel after which GRBL gives up on its own;
//! - a slow search speed, which limits the energy of an impact.
//!
//! The pushing force itself is set by the motor current (driver
//! potentiometer, or the homing current of Trinamic drivers on grblHAL),
//! which no G-code can change on a plain GRBL board.

use std::time::Duration;

/// Search speed above which homing is not considered gentle, mm/min.
pub const GENTLE_SEEK: f64 = 800.0;
/// Locate speed (slow approach on the switch) above which it is not
/// considered gentle, mm/min.
pub const GENTLE_FEED: f64 = 100.0;
/// Values written when the operator asks for gentle homing.
pub const SAFE_SEEK: f64 = 500.0;
pub const SAFE_FEED: f64 = 50.0;

/// Setting lines that make homing gentle (`$25` search, `$24` locate),
/// only for the values currently above the gentle limits.
pub fn gentle_settings(seek: Option<f64>, feed: Option<f64>) -> Vec<String> {
    let mut lines = Vec::new();
    if seek.is_some_and(|seek| seek > GENTLE_SEEK) {
        lines.push(format!("$25={SAFE_SEEK:.0}"));
    }
    if feed.is_some_and(|feed| feed > GENTLE_FEED) {
        lines.push(format!("$24={SAFE_FEED:.0}"));
    }
    lines
}

/// Longest acceptable homing. With a history of successful homings, a
/// quarter more than the slowest recent one; otherwise the worst case
/// from the travels (`$130`..`$132`) at search speed: Z first, then X
/// and Y together, 1.5 × travel each like GRBL, plus the slow locate.
pub fn time_limit(history: &[f64], travel: [f64; 3], seek: f64) -> Duration {
    if history.len() >= 3 {
        let slowest = history.iter().copied().fold(0.0, f64::max);
        return Duration::from_secs_f64(slowest * 1.25 + 1.5);
    }
    let seek = seek.max(1.0) / 60.0;
    let search = |travel: f64| 1.5 * travel / seek;
    let worst = search(travel[2]) + search(travel[0]).max(search(travel[1]));
    Duration::from_secs_f64(worst + 10.0)
}

/// A homing cycle started from the application. Plain GRBL 1.1 sends
/// no status report while homing, so the end is known from the reply to
/// `$H` (ok, error or alarm), not from the machine state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HomingRun {
    pub elapsed: Duration,
    /// The controller answered `$H`.
    pub done: bool,
    /// Started with hold to run: releasing the button stops it.
    pub hold_required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Continue,
    Finished,
    /// Reset the controller now.
    Stop(StopReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    Released,
    TooLong,
}

/// Decides, at every frame, whether a homing in progress may go on.
pub fn supervise(run: HomingRun, held: bool, limit: Duration) -> Verdict {
    if run.done {
        Verdict::Finished
    } else if run.hold_required && !held {
        Verdict::Stop(StopReason::Released)
    } else if run.elapsed > limit {
        Verdict::Stop(StopReason::TooLong)
    } else {
        Verdict::Continue
    }
}

/// Why the last homing failed, to tell the operator what to check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// `error:5`: homing is off in the controller (`$22=0`), usual on
    /// machines without limit switches.
    Disabled,
    /// `ALARM:9`: no switch within the search distance.
    NotFound,
    /// `ALARM:8`: the switch stayed pressed after the pull-off.
    PullOff,
    /// Released button, Escape or `ALARM:6`.
    Aborted,
    /// Stopped after much longer than usual.
    TooLong,
    /// Another alarm or error code.
    Other(u32),
}

impl Failure {
    /// From an alarm received while homing.
    pub fn from_alarm(code: u32) -> Self {
        match code {
            6 => Self::Aborted,
            8 => Self::PullOff,
            9 => Self::NotFound,
            code => Self::Other(code),
        }
    }

    /// From an error answering `$H`.
    pub fn from_error(code: u32) -> Self {
        match code {
            5 => Self::Disabled,
            code => Self::Other(code),
        }
    }
}

/// `$22` read from the controller: `Some(false)` when homing is off.
pub fn enabled(setting_22: Option<f64>) -> Option<bool> {
    // grblHAL uses bits of $22: bit 0 is "homing enabled".
    setting_22.map(|value| (value as i64) & 1 == 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(seconds: f64, done: bool) -> HomingRun {
        HomingRun {
            elapsed: Duration::from_secs_f64(seconds),
            done,
            hold_required: true,
        }
    }

    #[test]
    fn releasing_the_button_stops_homing_at_once() {
        let limit = Duration::from_secs(30);
        assert_eq!(supervise(run(2.0, false), true, limit), Verdict::Continue);
        assert_eq!(
            supervise(run(0.1, false), false, limit),
            Verdict::Stop(StopReason::Released)
        );
        // Once the controller answered, letting go is fine.
        assert_eq!(supervise(run(9.0, true), false, limit), Verdict::Finished);
    }

    #[test]
    fn a_homing_longer_than_usual_is_cut() {
        let limit = time_limit(&[10.0, 11.0, 10.5], [300.0; 3], 1000.0);
        assert!((limit.as_secs_f64() - (11.0 * 1.25 + 1.5)).abs() < 1e-9);
        let late = HomingRun {
            elapsed: limit + Duration::from_millis(100),
            done: false,
            hold_required: false,
        };
        assert_eq!(
            supervise(late, false, limit),
            Verdict::Stop(StopReason::TooLong)
        );
    }

    #[test]
    fn without_history_the_limit_follows_the_travels() {
        // 300 mm at 600 mm/min: 45 s per axis search, Z then X/Y.
        let limit = time_limit(&[], [300.0, 200.0, 300.0], 600.0);
        assert!((limit.as_secs_f64() - (45.0 + 45.0 + 10.0)).abs() < 1e-9);
    }

    #[test]
    fn only_fast_settings_are_lowered() {
        assert_eq!(
            gentle_settings(Some(2000.0), Some(25.0)),
            vec!["$25=500".to_owned()]
        );
        assert!(gentle_settings(Some(500.0), Some(50.0)).is_empty());
        assert!(gentle_settings(None, None).is_empty());
    }

    #[test]
    fn failures_name_what_to_check() {
        assert_eq!(Failure::from_error(5), Failure::Disabled);
        assert_eq!(Failure::from_alarm(9), Failure::NotFound);
        assert_eq!(Failure::from_alarm(8), Failure::PullOff);
        assert_eq!(Failure::from_alarm(6), Failure::Aborted);
        assert_eq!(Failure::from_alarm(1), Failure::Other(1));
        assert_eq!(enabled(Some(0.0)), Some(false));
        assert_eq!(enabled(Some(1.0)), Some(true));
        // grblHAL: homing on with extra option bits.
        assert_eq!(enabled(Some(5.0)), Some(true));
        assert_eq!(enabled(None), None);
    }
}
