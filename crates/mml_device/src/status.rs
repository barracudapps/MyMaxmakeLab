use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MachineState {
    #[default]
    Unknown,
    Idle,
    Run,
    Hold,
    /// `Hold:0` (complete) vs `Hold:1` (decelerating).
    HoldComplete,
    Jog,
    Alarm,
    Door,
    Check,
    Home,
    Sleep,
    /// grblHAL tool change in progress.
    Tool,
    /// HiMill firmware: lines queued (seen in MaxmakeLAB's parser).
    Queue,
    /// HiMill firmware: starting up.
    Boot,
}

impl MachineState {
    pub fn is_busy(self) -> bool {
        matches!(self, Self::Run | Self::Hold | Self::Jog | Self::Home)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::Idle => "Idle",
            Self::Run => "Run",
            Self::Hold | Self::HoldComplete => "Hold",
            Self::Jog => "Jog",
            Self::Alarm => "Alarm",
            Self::Door => "Door",
            Self::Check => "Check",
            Self::Home => "Home",
            Self::Sleep => "Sleep",
            Self::Tool => "Tool",
            Self::Queue => "Queue",
            Self::Boot => "Boot",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MachineStatus {
    pub state: MachineState,
    /// State word as the controller sent it, to show what an unknown
    /// state was.
    #[serde(default)]
    pub state_text: String,
    pub machine_position: [f64; 3],
    pub work_position: [f64; 3],
    /// Work coordinate offset, `machine = work + offset`.
    pub work_offset: [f64; 3],
    pub feed: f64,
    pub spindle: f64,
    /// Feed, rapid and spindle overrides in percent.
    pub overrides: [u32; 3],
    /// Active input pins, e.g. "XZP".
    pub pins: String,
    /// Machine position of the 4th (rotary) axis when the controller
    /// reports one.
    pub rotary: Option<f64>,
    /// Tool in the spindle (`T:` of grblHAL and the Carvera), `-1` or
    /// absent when empty.
    pub tool: Option<i32>,
    /// Why a Carvera stopped (`H:`), see `halt_message`.
    pub halt: Option<u32>,
}

impl MachineStatus {
    /// Limit switches currently pressed (X, Y, Z or A pins).
    pub fn active_limits(&self) -> String {
        self.pins
            .chars()
            .filter(|pin| matches!(pin, 'X' | 'Y' | 'Z' | 'A'))
            .collect()
    }
}

/// Parses `<Idle|MPos:0.000,0.000,0.000|FS:0,0|WCO:...>`. GRBL sends the
/// work offset only now and then, so the previous status provides it.
/// Converts the comma separated report of GRBL 0.9, Smoothieware and the
/// Makera Carvera (`<Idle,MPos:1,2,3,WPos:1,2,3>`) to the GRBL 1.1 form.
fn normalise(body: &str) -> String {
    if body.contains('|') {
        return body.to_owned();
    }
    let mut fields: Vec<String> = Vec::new();
    for token in body.split(',') {
        match fields.last_mut() {
            Some(field) if !token.contains(':') && field.contains(':') => {
                field.push(',');
                field.push_str(token);
            }
            _ => fields.push(token.to_owned()),
        }
    }
    fields.join("|")
}

fn first_number(value: &str) -> Option<f64> {
    value.split(',').next()?.trim().parse().ok()
}

pub fn parse_status(line: &str, previous: &MachineStatus) -> Option<MachineStatus> {
    let body = normalise(line.trim().strip_prefix('<')?.strip_suffix('>')?);
    let mut fields = body.split('|');
    let state_text = fields.next()?;
    let state = match state_text {
        "Idle" => MachineState::Idle,
        "Run" => MachineState::Run,
        "Hold:0" => MachineState::HoldComplete,
        text if text.starts_with("Hold") => MachineState::Hold,
        "Jog" => MachineState::Jog,
        text if text.starts_with("Alarm") => MachineState::Alarm,
        text if text.starts_with("Door") => MachineState::Door,
        "Check" => MachineState::Check,
        "Home" => MachineState::Home,
        "Sleep" => MachineState::Sleep,
        "Tool" => MachineState::Tool,
        "Queue" => MachineState::Queue,
        "Boot" => MachineState::Boot,
        _ => MachineState::Unknown,
    };
    let mut status = MachineStatus {
        state,
        state_text: state_text.to_owned(),
        pins: String::new(),
        tool: None,
        halt: None,
        ..previous.clone()
    };
    let triple = |text: &str| -> Option<[f64; 3]> {
        let mut values = text.split(',').map(|value| value.parse::<f64>().ok());
        Some([
            values.next()??,
            values.next()??,
            values.next().flatten().unwrap_or(0.0),
        ])
    };
    let fourth = |text: &str| {
        text.split(',')
            .nth(3)
            .and_then(|value| value.parse::<f64>().ok())
    };
    let mut machine = None;
    let mut work = None;
    for field in fields {
        let Some((key, value)) = field.split_once(':') else {
            continue;
        };
        match key {
            "MPos" => {
                machine = triple(value);
                status.rotary = fourth(value);
            }
            "WPos" => {
                work = triple(value);
                status.rotary = fourth(value);
            }
            "WCO" => status.work_offset = triple(value).unwrap_or(status.work_offset),
            "FS" => {
                let mut values = value
                    .split(',')
                    .filter_map(|value| value.parse::<f64>().ok());
                status.feed = values.next().unwrap_or(0.0);
                status.spindle = values.next().unwrap_or(0.0);
            }
            // GRBL without variable spindle, Smoothieware and the Carvera:
            // feed first, spindle in its own `S:` field.
            "F" => status.feed = first_number(value).unwrap_or(0.0),
            "S" => status.spindle = first_number(value).unwrap_or(0.0),
            "Ov" => {
                let values: Vec<u32> = value
                    .split(',')
                    .filter_map(|value| value.parse().ok())
                    .collect();
                if values.len() == 3 {
                    status.overrides = [values[0], values[1], values[2]];
                }
            }
            "Pn" => status.pins = value.to_owned(),
            "T" => {
                status.tool = value
                    .split(',')
                    .next()
                    .and_then(|tool| tool.trim().parse().ok())
                    .filter(|tool: &i32| *tool >= 0);
            }
            "H" => {
                status.halt = value
                    .split(',')
                    .next()
                    .and_then(|code| code.trim().parse().ok())
            }
            _ => {}
        }
    }
    let offset = status.work_offset;
    match (machine, work) {
        // Older firmwares report both: the offset follows from them.
        (Some(machine), Some(work)) => {
            status.machine_position = machine;
            status.work_position = work;
            status.work_offset = [0, 1, 2].map(|axis| machine[axis] - work[axis]);
        }
        (Some(machine), None) => {
            status.machine_position = machine;
            status.work_position = [0, 1, 2].map(|axis| machine[axis] - offset[axis]);
        }
        (None, Some(work)) => {
            status.work_position = work;
            status.machine_position = [0, 1, 2].map(|axis| work[axis] + offset[axis]);
        }
        (None, None) => {}
    }
    Some(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn himill_and_grblhal_states_are_known() {
        let previous = MachineStatus::default();
        for (text, state) in [
            ("Queue", MachineState::Queue),
            ("Boot", MachineState::Boot),
            ("Tool", MachineState::Tool),
            // grblHAL adds the alarm code: "Alarm:3".
            ("Alarm:3", MachineState::Alarm),
            ("Alarm", MachineState::Alarm),
        ] {
            let status = parse_status(&format!("<{text}|MPos:0,0,0>"), &previous).unwrap();
            assert_eq!(status.state, state);
        }
        let odd = parse_status("<Weird|MPos:0,0,0>", &previous).unwrap();
        assert_eq!(odd.state, MachineState::Unknown);
        assert_eq!(odd.state_text, "Weird");
    }

    #[test]
    fn work_position_uses_the_last_known_offset() {
        let first = parse_status(
            "<Idle|MPos:10.000,20.000,0.000|FS:0,0|WCO:5.000,5.000,0.000>",
            &MachineStatus::default(),
        )
        .unwrap();
        assert_eq!(first.work_position, [5.0, 15.0, 0.0]);
        let second =
            parse_status("<Run|MPos:11.000,20.000,0.000|FS:1200,800|Pn:X>", &first).unwrap();
        assert_eq!(second.state, MachineState::Run);
        assert_eq!(second.work_position, [6.0, 15.0, 0.0]);
        assert_eq!((second.feed, second.spindle), (1200.0, 800.0));
        assert_eq!(second.pins, "X");
        assert_eq!(second.active_limits(), "X");
    }

    #[test]
    fn legacy_comma_reports_are_understood() {
        let status = parse_status(
            "<Run,MPos:10.000,20.000,-1.000,WPos:5.000,5.000,-1.000>",
            &MachineStatus::default(),
        )
        .unwrap();
        assert_eq!(status.state, MachineState::Run);
        assert_eq!(status.machine_position, [10.0, 20.0, -1.0]);
        assert_eq!(status.work_position, [5.0, 5.0, -1.0]);
    }

    #[test]
    fn fourth_axis_is_read() {
        let status = parse_status(
            "<Idle|MPos:1.000,2.000,3.000,45.000|FS:0,0>",
            &MachineStatus::default(),
        )
        .unwrap();
        assert_eq!(status.rotary, Some(45.0));
        assert_eq!(status.machine_position, [1.0, 2.0, 3.0]);
    }

    #[test]
    fn carvera_reports_tool_and_halt_reason() {
        let status = parse_status(
            "<Alarm|MPos:-10.000,-5.000,-1.000,0.000|WPos:0.000,0.000,0.000,0.000|F:0,0,100|T:3,-12.345,4|H:7>",
            &MachineStatus::default(),
        )
        .unwrap();
        assert_eq!(status.tool, Some(3));
        assert_eq!(status.halt, Some(7));
        let next = parse_status("<Idle|MPos:0,0,0|T:-1,0.000>", &status).unwrap();
        assert_eq!((next.tool, next.halt), (None, None));
        let running =
            parse_status("<Run|MPos:0,0,0|F:800,1000,100|S:12000,12000,100>", &next).unwrap();
        assert_eq!((running.feed, running.spindle), (800.0, 12000.0));
    }
}
