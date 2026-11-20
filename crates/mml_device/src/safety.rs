//! Live safety checks run by the controller thread on every status
//! report. GRBL drives open-loop steppers: the reported position is the
//! position GRBL commanded, not a measurement. These checks therefore
//! catch everything that makes the machine go where the job did not ask
//! (wrong settings, stray commands, rotary misconfiguration, a program
//! larger than the material) plus every switch the controller reads; a
//! motor that skips steps without any switch can only be caught by the
//! homing duration check.

use crate::status::{MachineState, MachineStatus};
use serde::{Deserialize, Serialize};

/// What a running job is allowed to do, in work coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobGuard {
    /// `[min, max]` for X, Y and Z.
    pub envelope: Option<[[f64; 2]; 3]>,
    pub uses_rotary: bool,
    /// Margin added to the envelope, in mm.
    pub tolerance: f64,
    /// Feed hold as soon as a critical anomaly is seen.
    pub auto_pause: bool,
}

impl Default for JobGuard {
    fn default() -> Self {
        Self {
            envelope: None,
            uses_rotary: false,
            tolerance: 1.0,
            auto_pause: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Anomaly {
    /// The head left the area the program covers.
    OutsideEnvelope {
        axis: char,
        value: f64,
        min: f64,
        max: f64,
    },
    /// The rotary axis turned although the job never commands it.
    UnexpectedRotary { delta: f64 },
    /// The position changed while idle with no command in flight.
    MotionWhileIdle { distance: f64 },
    /// A limit switch is pressed outside homing.
    LimitSwitch { pins: String },
    /// No status report for this many seconds during a job.
    StatusTimeout { seconds: f64 },
}

impl Anomaly {
    pub fn is_critical(&self) -> bool {
        !matches!(self, Self::MotionWhileIdle { .. })
    }

    fn kind(&self) -> u8 {
        match self {
            Self::OutsideEnvelope { .. } => 0,
            Self::UnexpectedRotary { .. } => 1,
            Self::MotionWhileIdle { .. } => 2,
            Self::LimitSwitch { .. } => 3,
            Self::StatusTimeout { .. } => 4,
        }
    }
}

#[derive(Debug, Default)]
pub struct SafetyMonitor {
    guard: Option<JobGuard>,
    rotary_reference: Option<f64>,
    idle_reference: Option<[f64; 4]>,
    reported: Vec<u8>,
    /// An `M6` is executing: the head visits the tool rack, outside the
    /// job area, on purpose.
    tool_change: bool,
}

/// `M6` (any spelling: `M6 T3`, `T3M06`, `m6t8888`), not `M60` or `M61`.
pub fn is_tool_change(line: &str) -> bool {
    let code = line
        .split([';', '('])
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    let bytes = code.as_bytes();
    bytes.iter().enumerate().any(|(index, &byte)| {
        if byte != b'M' {
            return false;
        }
        let digits: String = code[index + 1..]
            .trim_start()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        !digits.is_empty() && digits.parse::<u32>() == Ok(6)
    })
}

fn position_of(status: &MachineStatus) -> [f64; 4] {
    let [x, y, z] = status.machine_position;
    [x, y, z, status.rotary.unwrap_or(0.0)]
}

impl SafetyMonitor {
    pub fn start_job(&mut self, guard: JobGuard, status: Option<&MachineStatus>) {
        self.rotary_reference = status.and_then(|status| status.rotary);
        self.guard = Some(guard);
        self.reported.clear();
    }

    pub fn end_job(&mut self) {
        self.guard = None;
        self.rotary_reference = None;
    }

    pub fn set_tool_change(&mut self, active: bool) {
        self.tool_change = active;
    }

    pub fn guard(&self) -> Option<&JobGuard> {
        self.guard.as_ref()
    }

    /// Reports each kind of anomaly once per job so the user is not
    /// flooded at 5 Hz.
    pub fn report(&mut self, anomaly: Anomaly) -> Option<Anomaly> {
        let kind = anomaly.kind();
        if self.reported.contains(&kind) {
            return None;
        }
        self.reported.push(kind);
        Some(anomaly)
    }

    /// `commands_pending` is true while lines or jogs are in flight.
    pub fn observe(&mut self, status: &MachineStatus, commands_pending: bool) -> Vec<Anomaly> {
        let mut found = Vec::new();
        let limits = status.active_limits();
        if !limits.is_empty()
            && matches!(
                status.state,
                MachineState::Run | MachineState::Jog | MachineState::Idle
            )
        {
            found.push(Anomaly::LimitSwitch { pins: limits });
        }

        if let Some(guard) = &self.guard {
            if status.state == MachineState::Run
                && !self.tool_change
                && let Some(envelope) = guard.envelope
            {
                for (axis, name) in ['X', 'Y', 'Z'].into_iter().enumerate() {
                    let value = status.work_position[axis];
                    let [min, max] = envelope[axis];
                    if value < min - guard.tolerance || value > max + guard.tolerance {
                        found.push(Anomaly::OutsideEnvelope {
                            axis: name,
                            value,
                            min,
                            max,
                        });
                        break;
                    }
                }
            }
            if !guard.uses_rotary
                && let (Some(reference), Some(current)) = (self.rotary_reference, status.rotary)
                && (current - reference).abs() > 0.05
            {
                found.push(Anomaly::UnexpectedRotary {
                    delta: current - reference,
                });
            }
        }

        let position = position_of(status);
        if status.state == MachineState::Idle && !commands_pending && self.guard.is_none() {
            if let Some(reference) = self.idle_reference {
                let distance = (0..4)
                    .map(|axis| (position[axis] - reference[axis]).powi(2))
                    .sum::<f64>()
                    .sqrt();
                if distance > 0.01 {
                    found.push(Anomaly::MotionWhileIdle { distance });
                    self.idle_reference = Some(position);
                }
            } else {
                self.idle_reference = Some(position);
            }
        } else {
            self.idle_reference = None;
        }
        found
    }
}

/// Axes on which a job would leave the machine travel, as
/// `(axis, needed min, needed max, allowed min, allowed max)`.
///
/// `machine_min` / `machine_max` is the job envelope in machine
/// coordinates. GRBL machines homed towards the maximum use negative
/// coordinates (`[-travel, 0]`), the others `[0, travel]`.
pub fn travel_violations(
    machine_min: [f64; 3],
    machine_max: [f64; 3],
    travel: [Option<f64>; 3],
    negative_space: bool,
) -> Vec<(char, f64, f64, f64, f64)> {
    let mut violations = Vec::new();
    for (axis, name) in ['X', 'Y', 'Z'].into_iter().enumerate() {
        let Some(travel) = travel[axis] else { continue };
        let (low, high) = if negative_space {
            (-travel, 0.0)
        } else {
            (0.0, travel)
        };
        let epsilon = 1e-3;
        if machine_min[axis] < low - epsilon || machine_max[axis] > high + epsilon {
            violations.push((name, machine_min[axis], machine_max[axis], low, high));
        }
    }
    violations
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(state: MachineState, work: [f64; 3], rotary: Option<f64>) -> MachineStatus {
        MachineStatus {
            state,
            work_position: work,
            machine_position: work,
            rotary,
            ..MachineStatus::default()
        }
    }

    #[test]
    fn leaving_the_job_envelope_is_critical() {
        let mut monitor = SafetyMonitor::default();
        let guard = JobGuard {
            envelope: Some([[0.0, 50.0], [0.0, 50.0], [0.0, 0.0]]),
            ..JobGuard::default()
        };
        monitor.start_job(guard, None);
        assert!(
            monitor
                .observe(&status(MachineState::Run, [50.5, 10.0, 0.0], None), true)
                .is_empty()
        );
        let found = monitor.observe(&status(MachineState::Run, [10.0, 60.0, 0.0], None), true);
        assert!(matches!(
            found[0],
            Anomaly::OutsideEnvelope { axis: 'Y', .. }
        ));
        assert!(found[0].is_critical());
    }

    #[test]
    fn the_tool_rack_is_allowed_during_a_tool_change_only() {
        assert!(is_tool_change("M6 T3"));
        assert!(is_tool_change("t3m06 ; next"));
        assert!(is_tool_change("M6T8888"));
        assert!(!is_tool_change("M60"));
        assert!(!is_tool_change("G0 X6 ; M6 later"));
        let mut monitor = SafetyMonitor::default();
        let guard = JobGuard {
            envelope: Some([[0.0, 50.0], [0.0, 50.0], [-5.0, 5.0]]),
            ..JobGuard::default()
        };
        monitor.start_job(guard, None);
        let rack = status(MachineState::Run, [-80.0, 120.0, 5.0], None);
        monitor.set_tool_change(true);
        assert!(monitor.observe(&rack, true).is_empty());
        monitor.set_tool_change(false);
        assert!(!monitor.observe(&rack, true).is_empty());
    }

    #[test]
    fn rotary_moving_without_being_commanded_is_detected() {
        let mut monitor = SafetyMonitor::default();
        let start = status(MachineState::Idle, [0.0; 3], Some(0.0));
        monitor.start_job(JobGuard::default(), Some(&start));
        let found = monitor.observe(&status(MachineState::Run, [0.0; 3], Some(12.0)), true);
        assert_eq!(found, vec![Anomaly::UnexpectedRotary { delta: 12.0 }]);
        let mut allowed = SafetyMonitor::default();
        allowed.start_job(
            JobGuard {
                uses_rotary: true,
                ..JobGuard::default()
            },
            Some(&start),
        );
        assert!(
            allowed
                .observe(&status(MachineState::Run, [0.0; 3], Some(12.0)), true)
                .is_empty()
        );
    }

    #[test]
    fn motion_while_idle_without_command_is_reported_once_per_move() {
        let mut monitor = SafetyMonitor::default();
        assert!(
            monitor
                .observe(&status(MachineState::Idle, [0.0; 3], None), false)
                .is_empty()
        );
        assert!(
            monitor
                .observe(&status(MachineState::Idle, [0.0; 3], None), false)
                .is_empty()
        );
        let found = monitor.observe(&status(MachineState::Idle, [1.0, 0.0, 0.0], None), false);
        assert!(matches!(found[0], Anomaly::MotionWhileIdle { .. }));
        assert!(
            monitor
                .observe(&status(MachineState::Idle, [1.0, 0.0, 0.0], None), false)
                .is_empty()
        );
    }

    #[test]
    fn job_larger_than_the_travel_is_refused() {
        let violations = travel_violations(
            [-390.0, -100.0, 0.0],
            [-10.0, -20.0, 0.0],
            [Some(300.0), Some(300.0), None],
            true,
        );
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].0, 'X');
        assert!(
            travel_violations(
                [10.0, 10.0, 0.0],
                [200.0, 200.0, 0.0],
                [Some(300.0), Some(300.0), None],
                false
            )
            .is_empty()
        );
    }
}
