use crate::app::{Anchor, Placement};
use crate::app::{Level, MyApp};
use crate::i18n::{grbl_alarm, grbl_error, tr, trf};
use crate::panels::{section, warning_text};
use crate::settings::Settings;
use eframe::egui::{self, Color32, DragValue, RichText};
use mml_core::machine::MachineProfile;
use mml_device::{
    Anomaly, Command, Connection, Controller, Event, JobGuard, JobOutcome, MachineState,
    MachineStatus, realtime,
};
use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleKind {
    Sent,
    Received,
    Error,
    Info,
}

pub struct RunningJob {
    pub total: usize,
    pub acknowledged: usize,
    pub started: Instant,
    pub estimate: Duration,
    pub paused: bool,
}

pub struct DeviceState {
    controller: Option<Controller>,
    pub connecting: bool,
    pub banner: Option<String>,
    pub status: Option<MachineStatus>,
    pub console: VecDeque<(ConsoleKind, String)>,
    pub console_input: String,
    pub command_history: Vec<String>,
    pub settings: BTreeMap<u32, String>,
    pub job: Option<RunningJob>,
    pub alarm: Option<u32>,
    pub ports: Vec<(String, String)>,
    pub use_tcp: bool,
    pub serial_port: String,
    pub baud: u32,
    pub host: String,
    pub tcp_port: u16,
    last_preview: Instant,
    pub new_macro: Option<(String, String)>,
    /// Last anomaly and whether the job was paused for it.
    pub anomaly: Option<(Anomaly, bool)>,
    pub homing_results: Vec<(Duration, bool)>,
    pub reached_pause: Option<usize>,
    pub homed: bool,
}

impl DeviceState {
    pub fn new(settings: &Settings) -> Self {
        let connection = &settings.connection;
        Self {
            controller: None,
            connecting: false,
            banner: None,
            status: None,
            console: VecDeque::new(),
            console_input: String::new(),
            command_history: Vec::new(),
            settings: BTreeMap::new(),
            job: None,
            alarm: None,
            ports: mml_device::list_serial_ports(),
            use_tcp: connection.use_tcp,
            serial_port: connection.serial_port.clone(),
            baud: connection.baud,
            host: connection.host.clone(),
            tcp_port: connection.tcp_port,
            last_preview: Instant::now(),
            new_macro: None,
            anomaly: None,
            homing_results: Vec::new(),
            reached_pause: None,
            homed: false,
        }
    }

    pub fn store_connection(&self, settings: &mut Settings) {
        settings.connection.use_tcp = self.use_tcp;
        settings.connection.serial_port = self.serial_port.clone();
        settings.connection.baud = self.baud;
        settings.connection.host = self.host.clone();
        settings.connection.tcp_port = self.tcp_port;
    }

    pub fn is_connected(&self) -> bool {
        self.controller.is_some()
    }

    pub fn job_running(&self) -> bool {
        self.job.is_some()
    }

    pub fn state(&self) -> MachineState {
        self.status
            .as_ref()
            .map_or(MachineState::Unknown, |status| status.state)
    }

    pub fn setting_f64(&self, key: u32) -> Option<f64> {
        self.settings
            .get(&key)
            .and_then(|value| value.trim().parse().ok())
    }

    fn log(&mut self, kind: ConsoleKind, text: impl Into<String>) {
        self.console.push_back((kind, text.into()));
        while self.console.len() > 2000 {
            self.console.pop_front();
        }
    }

    pub fn connect(&mut self) {
        let connection = if self.use_tcp {
            Connection::Tcp {
                host: self.host.trim().to_owned(),
                port: self.tcp_port,
            }
        } else {
            Connection::Serial {
                port: self.serial_port.clone(),
                baud: self.baud,
            }
        };
        self.log(ConsoleKind::Info, trf("device.connecting", &[&connection]));
        self.controller = Some(Controller::connect(connection));
        self.connecting = true;
    }

    pub fn disconnect(&mut self) {
        self.controller = None;
        self.connecting = false;
        self.banner = None;
        self.status = None;
        self.job = None;
        self.log(ConsoleKind::Info, tr("device.disconnected"));
    }

    pub fn send(&mut self, command: Command) {
        if let Some(controller) = &self.controller {
            controller.send(command);
        }
    }

    pub fn line(&mut self, line: impl Into<String>) {
        self.send(Command::Line(line.into()));
    }

    /// Jogs relative to the current position. With homing done and soft
    /// limits active GRBL refuses targets outside the travel; when the
    /// work area is known the target is also clamped here so a jog never
    /// slams into the frame.
    pub fn jog(&mut self, dx: f64, dy: f64, dz: f64, feed: f64, machine: &MachineProfile) {
        let (mut dx, mut dy) = (dx, dy);
        if let Some(status) = &self.status {
            let [x, y, _] = status.work_position;
            if machine.width > 0.0 && !machine.relative_to_current {
                dx = (x + dx).clamp(0.0, machine.width) - x;
                dy = (y + dy).clamp(0.0, machine.height) - y;
            }
        }
        if dx.abs() < 1e-6 && dy.abs() < 1e-6 && dz.abs() < 1e-6 {
            return;
        }
        self.line(format!("$J=G91 G21 X{dx:.3} Y{dy:.3} Z{dz:.3} F{feed:.0}"));
    }

    pub fn poll(&mut self, ctx: &egui::Context, notices: &mut Vec<(String, Level, Instant)>) {
        let Some(controller) = &self.controller else {
            return;
        };
        let events: Vec<Event> = controller.events().collect();
        for event in events {
            match event {
                Event::Connected { banner } => {
                    self.connecting = false;
                    self.log(ConsoleKind::Info, banner.clone());
                    self.banner = Some(banner);
                    self.alarm = None;
                }
                Event::Disconnected { reason } => {
                    self.controller = None;
                    self.connecting = false;
                    self.banner = None;
                    self.status = None;
                    if self.job.take().is_some() {
                        notices.push((
                            tr("device.lost_during_job").to_owned(),
                            Level::Error,
                            Instant::now(),
                        ));
                    }
                    let text = match reason {
                        Some(reason) => trf("device.connection_failed", &[&reason]),
                        None => tr("device.disconnected").to_owned(),
                    };
                    self.log(ConsoleKind::Error, text.clone());
                    notices.push((text, Level::Error, Instant::now()));
                    break;
                }
                Event::Status(status) => {
                    if self.connecting {
                        self.connecting = false;
                    }
                    if status.state != MachineState::Alarm {
                        self.alarm = None;
                    }
                    self.status = Some(status);
                }
                Event::Sent(line) => self.log(ConsoleKind::Sent, line),
                Event::Received(line) => self.log(ConsoleKind::Received, line),
                Event::Error { code, line } => {
                    let text = format!("error:{code} — {} ({line})", grbl_error(code));
                    self.log(ConsoleKind::Error, text.clone());
                    notices.push((text, Level::Error, Instant::now()));
                }
                Event::Alarm(code) => {
                    self.alarm = Some(code);
                    let text = format!("ALARM:{code} — {}", grbl_alarm(code));
                    self.log(ConsoleKind::Error, text.clone());
                    notices.push((text, Level::Error, Instant::now()));
                }
                Event::Setting { key, value } => {
                    self.settings.insert(key, value);
                }
                Event::JobProgress {
                    acknowledged,
                    total,
                } => {
                    if let Some(job) = &mut self.job {
                        job.acknowledged = acknowledged;
                        job.total = total;
                    }
                }
                Event::Anomaly { anomaly, paused } => {
                    let text = anomaly_text(&anomaly);
                    self.log(ConsoleKind::Error, text.clone());
                    let level = if anomaly.is_critical() {
                        Level::Error
                    } else {
                        Level::Warning
                    };
                    notices.push((text, level, Instant::now()));
                    self.anomaly = Some((anomaly, paused));
                }
                Event::HomingFinished { duration, success } => {
                    self.homed = success;
                    self.homing_results.push((duration, success));
                }
                Event::ProgramPaused { index } => {
                    self.reached_pause = Some(index);
                    if let Some(job) = &mut self.job {
                        job.paused = true;
                    }
                }
                Event::JobFinished { outcome, elapsed } => {
                    self.job = None;
                    let minutes = elapsed.as_secs() / 60;
                    let seconds = elapsed.as_secs() % 60;
                    let (text, level) = match outcome {
                        JobOutcome::Completed => (
                            trf("job.completed", &[&format!("{minutes}:{seconds:02}")]),
                            Level::Info,
                        ),
                        JobOutcome::Stopped => (tr("job.stopped").to_owned(), Level::Warning),
                        JobOutcome::Error { line, code } => (
                            trf("job.failed_line", &[&line, &grbl_error(code)]),
                            Level::Error,
                        ),
                        JobOutcome::Alarm(code) => {
                            (format!("ALARM:{code} — {}", grbl_alarm(code)), Level::Error)
                        }
                    };
                    self.log(ConsoleKind::Info, text.clone());
                    notices.push((text, level, Instant::now()));
                }
            }
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    pub fn connection_badge(&mut self, ui: &mut egui::Ui) {
        let (text, color) = if self.connecting {
            (
                tr("device.connecting_short").to_owned(),
                Color32::from_rgb(200, 150, 30),
            )
        } else if self.controller.is_some() {
            let state = self.state();
            (format!("● {}", state_label(state)), state_color(state))
        } else {
            (format!("○ {}", tr("device.offline")), Color32::GRAY)
        };
        ui.label(RichText::new(text).color(color).strong());
    }
}

fn state_label(state: MachineState) -> &'static str {
    match state {
        MachineState::Unknown => tr("state.unknown"),
        MachineState::Idle => tr("state.idle"),
        MachineState::Run => tr("state.run"),
        MachineState::Hold | MachineState::HoldComplete => tr("state.hold"),
        MachineState::Jog => tr("state.jog"),
        MachineState::Alarm => tr("state.alarm"),
        MachineState::Door => tr("state.door"),
        MachineState::Check => tr("state.check"),
        MachineState::Home => tr("state.home"),
        MachineState::Sleep => tr("state.sleep"),
    }
}

fn state_color(state: MachineState) -> Color32 {
    match state {
        MachineState::Idle => Color32::from_rgb(40, 160, 80),
        MachineState::Run | MachineState::Jog | MachineState::Home => {
            Color32::from_rgb(40, 120, 220)
        }
        MachineState::Hold | MachineState::HoldComplete | MachineState::Door => {
            Color32::from_rgb(220, 150, 20)
        }
        MachineState::Alarm => Color32::from_rgb(210, 50, 40),
        _ => Color32::GRAY,
    }
}

pub fn run_panel(app: &mut MyApp, ui: &mut egui::Ui) {
    connection(app, ui);
    if !app.device.is_connected() {
        ui.separator();
        job_card(app, ui);
        return;
    }
    ui.separator();
    machine_state(app, ui);
    ui.separator();
    job_card(app, ui);
    ui.separator();
    jog(app, ui);
    ui.separator();
    console(app, ui);
}

fn connection(app: &mut MyApp, ui: &mut egui::Ui) {
    section(ui, tr("device.connection"));
    let device = &mut app.device;
    if device.is_connected() {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(
                    device
                        .banner
                        .clone()
                        .unwrap_or_else(|| tr("device.waiting").to_owned()),
                )
                .weak(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr("device.disconnect")).clicked() {
                    device.disconnect();
                }
            });
        });
        return;
    }
    ui.horizontal(|ui| {
        ui.selectable_value(&mut device.use_tcp, false, tr("device.usb"));
        ui.selectable_value(&mut device.use_tcp, true, tr("device.wifi"));
    });
    if device.use_tcp {
        ui.horizontal(|ui| {
            ui.label(tr("device.host"));
            ui.add(egui::TextEdit::singleline(&mut device.host).desired_width(130.0));
            ui.label(tr("device.port"));
            ui.add(DragValue::new(&mut device.tcp_port).range(1..=65535));
        });
    } else {
        ui.horizontal(|ui| {
            let selected = device
                .ports
                .iter()
                .find(|(name, _)| *name == device.serial_port)
                .map(|(_, label)| label.clone())
                .unwrap_or_else(|| tr("device.choose_port").to_owned());
            egui::ComboBox::from_id_salt("port")
                .selected_text(selected)
                .width(200.0)
                .show_ui(ui, |ui| {
                    for (name, label) in device.ports.clone() {
                        ui.selectable_value(&mut device.serial_port, name, label);
                    }
                });
            if ui.button("⟳").on_hover_text(tr("device.refresh")).clicked() {
                device.ports = mml_device::list_serial_ports();
            }
            egui::ComboBox::from_id_salt("baud")
                .selected_text(device.baud.to_string())
                .width(80.0)
                .show_ui(ui, |ui| {
                    for baud in [9600, 57600, 115200, 230400, 250000, 921600] {
                        ui.selectable_value(&mut device.baud, baud, baud.to_string());
                    }
                });
        });
        if device.serial_port.is_empty() && device.ports.len() == 1 {
            device.serial_port = device.ports[0].0.clone();
        }
    }
    let ready = device.use_tcp || !device.serial_port.is_empty();
    let button = egui::Button::new(RichText::new(tr("device.connect")).strong())
        .min_size(egui::vec2(140.0, 28.0));
    if ui.add_enabled(ready, button).clicked() {
        device.connect();
    }
}

fn machine_state(app: &mut MyApp, ui: &mut egui::Ui) {
    let device = &mut app.device;
    let state = device.state();
    egui::Frame::new()
        .fill(state_color(state).gamma_multiply(0.18))
        .corner_radius(8.0)
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(state_label(state))
                    .color(state_color(state))
                    .strong()
                    .size(20.0),
            );
            if state == MachineState::Alarm {
                if let Some(code) = device.alarm {
                    ui.label(grbl_alarm(code));
                } else {
                    ui.label(tr("state.alarm_hint"));
                }
                ui.horizontal(|ui| {
                    if ui
                        .button(tr("device.unlock"))
                        .on_hover_text(tr("device.unlock_hint"))
                        .clicked()
                    {
                        device.line("$X");
                    }
                    if ui.button(tr("device.home")).clicked() {
                        device.line("$H");
                    }
                });
            }
            if state == MachineState::Door {
                ui.label(tr("state.door_hint"));
            }
            if let Some(status) = device.status.clone() {
                ui.add_space(4.0);
                egui::Grid::new("dro")
                    .num_columns(4)
                    .spacing([14.0, 2.0])
                    .show(ui, |ui| {
                        for (axis, label) in ["X", "Y", "Z"].iter().enumerate() {
                            ui.label(RichText::new(*label).strong().size(16.0));
                            ui.label(
                                RichText::new(format!("{:>9.3}", status.work_position[axis]))
                                    .monospace()
                                    .size(18.0),
                            );
                            ui.label(
                                RichText::new(format!("{:>9.3}", status.machine_position[axis]))
                                    .monospace()
                                    .weak(),
                            );
                            if ui
                                .small_button(tr("device.zero"))
                                .on_hover_text(trf("device.zero_hint", &[label]))
                                .clicked()
                            {
                                device.line(format!("G10 L20 P1 {label}0"));
                            }
                            ui.end_row();
                        }
                    });
                ui.label(
                    RichText::new(trf(
                        "device.feed_power",
                        &[
                            &format!("{:.0}", status.feed),
                            &format!("{:.0}", status.spindle),
                        ],
                    ))
                    .weak(),
                );
                if !status.pins.is_empty() {
                    ui.label(
                        RichText::new(trf("device.pins", &[&status.pins]))
                            .color(Color32::from_rgb(220, 150, 20)),
                    );
                }
            }
        });
    ui.horizontal_wrapped(|ui| {
        if ui.button(tr("device.home")).clicked() {
            device.line("$H");
        }
        if ui.button(tr("device.zero_xy")).clicked() {
            device.line("G10 L20 P1 X0 Y0");
        }
        if ui.button(tr("device.goto_zero")).clicked() {
            device.line("G90 G0 X0 Y0");
        }
        if ui
            .button(tr("device.reset"))
            .on_hover_text(tr("device.reset_hint"))
            .clicked()
        {
            device.send(Command::Realtime(realtime::SOFT_RESET));
        }
    });
}

fn jog(app: &mut MyApp, ui: &mut egui::Ui) {
    section(ui, tr("jog.title"));
    let busy = app.device.job_running();
    ui.add_enabled_ui(!busy, |ui| {
        let step = app.settings.jog_step;
        let feed = app.settings.jog_feed;
        let size = egui::vec2(44.0, 38.0);
        let mut target: Option<(f64, f64, f64)> = None;
        ui.horizontal(|ui| {
            egui::Grid::new("jog-pad")
                .spacing([4.0, 4.0])
                .show(ui, |ui| {
                    let buttons = [
                        [("↖", -1.0, 1.0), ("↑", 0.0, 1.0), ("↗", 1.0, 1.0)],
                        [("←", -1.0, 0.0), ("🏠", 0.0, 0.0), ("→", 1.0, 0.0)],
                        [("↙", -1.0, -1.0), ("↓", 0.0, -1.0), ("↘", 1.0, -1.0)],
                    ];
                    for row in buttons {
                        for (icon, dx, dy) in row {
                            let response = ui.add(
                                egui::Button::new(RichText::new(icon).size(18.0)).min_size(size),
                            );
                            if response.clicked() {
                                target = Some(if icon == "🏠" {
                                    (f64::NAN, 0.0, 0.0)
                                } else {
                                    (dx * step, dy * step, 0.0)
                                });
                            }
                        }
                        ui.end_row();
                    }
                });
            ui.vertical(|ui| {
                if ui
                    .add(egui::Button::new(RichText::new("Z+").size(16.0)).min_size(size))
                    .clicked()
                {
                    target = Some((0.0, 0.0, step.min(10.0)));
                }
                if ui
                    .add(egui::Button::new(RichText::new("Z−").size(16.0)).min_size(size))
                    .clicked()
                {
                    target = Some((0.0, 0.0, -step.min(10.0)));
                }
            });
        });
        if let Some((dx, dy, dz)) = target {
            if dx.is_nan() {
                app.device.line("G90 G0 X0 Y0");
            } else {
                app.device.jog(dx, dy, dz, feed, &app.machine);
            }
        }
        ui.horizontal(|ui| {
            ui.label(tr("jog.step"));
            for value in [0.1, 1.0, 10.0, 50.0] {
                ui.selectable_value(&mut app.settings.jog_step, value, format!("{value}"));
            }
        });
        ui.horizontal(|ui| {
            ui.label(tr("jog.feed"));
            ui.add(
                DragValue::new(&mut app.settings.jog_feed)
                    .range(10.0..=20_000.0)
                    .suffix(" mm/min")
                    .speed(10.0),
            );
        });
        ui.label(RichText::new(tr("jog.keyboard_hint")).weak().small());
    });
}

pub fn anomaly_text(anomaly: &Anomaly) -> String {
    match anomaly {
        Anomaly::OutsideEnvelope {
            axis,
            value,
            min,
            max,
        } => trf(
            "safety.outside",
            &[
                axis,
                &format!("{value:.2}"),
                &format!("{min:.1}"),
                &format!("{max:.1}"),
            ],
        ),
        Anomaly::UnexpectedRotary { delta } => trf("safety.rotary", &[&format!("{delta:.1}")]),
        Anomaly::MotionWhileIdle { distance } => {
            trf("safety.idle_motion", &[&format!("{distance:.2}")])
        }
        Anomaly::LimitSwitch { pins } => trf("safety.limit", &[pins]),
        Anomaly::StatusTimeout { seconds } => trf("safety.timeout", &[&format!("{seconds:.1}")]),
    }
}

fn pause_text(reason: &mml_job::PauseReason) -> String {
    match reason {
        mml_job::PauseReason::HeadChange {
            next_layer,
            to_laser: true,
        } => trf("pause.to_laser", &[next_layer]),
        mml_job::PauseReason::HeadChange {
            next_layer,
            to_laser: false,
        } => trf("pause.to_cnc", &[next_layer]),
        mml_job::PauseReason::LayerPause { next_layer } => trf("pause.layer", &[next_layer]),
        mml_job::PauseReason::NextFile { name } => trf("pause.next_file", &[name]),
    }
}

/// Reasons that forbid starting, checked against the live position:
/// the whole job envelope must fit in the machine travel.
fn start_blockers(app: &MyApp) -> Vec<String> {
    let mut blockers = Vec::new();
    let Some(built) = &app.job.built else {
        return blockers;
    };
    let Some(status) = &app.device.status else {
        return blockers;
    };
    let bounds = built.program.bounds();
    if bounds.is_empty() {
        return blockers;
    }
    let (z_low, z_high) = built.program.z_range;
    let travel = [
        app.device.setting_f64(130).or(Some(app.machine.width)),
        app.device.setting_f64(131).or(Some(app.machine.height)),
        app.device.setting_f64(132).filter(|travel| *travel > 0.0),
    ];
    let position = status.machine_position;
    let negative = position.iter().take(2).any(|value| *value < -1e-3);
    let offset = status.work_offset;
    let (min, max) = match built.anchor {
        Some(anchor) => (
            [
                position[0] + bounds.min.x - anchor.x,
                position[1] + bounds.min.y - anchor.y,
                z_low + offset[2],
            ],
            [
                position[0] + bounds.max.x - anchor.x,
                position[1] + bounds.max.y - anchor.y,
                z_high + offset[2],
            ],
        ),
        None => (
            [
                bounds.min.x + offset[0],
                bounds.min.y + offset[1],
                z_low + offset[2],
            ],
            [
                bounds.max.x + offset[0],
                bounds.max.y + offset[1],
                z_high + offset[2],
            ],
        ),
    };
    for (axis, low, high, allowed_low, allowed_high) in
        mml_device::travel_violations(min, max, travel, negative)
    {
        blockers.push(trf(
            "safety.travel",
            &[
                &axis,
                &format!("{low:.1}"),
                &format!("{high:.1}"),
                &format!("{allowed_low:.1}"),
                &format!("{allowed_high:.1}"),
            ],
        ));
    }
    blockers
}

fn guard_for(app: &MyApp) -> Option<JobGuard> {
    let built = app.job.built.as_ref()?;
    let bounds = built.program.bounds();
    let (z_low, z_high) = built.program.z_range;
    Some(JobGuard {
        envelope: (!bounds.is_empty()).then_some([
            [bounds.min.x, bounds.max.x],
            [bounds.min.y, bounds.max.y],
            [z_low, z_high],
        ]),
        uses_rotary: built.program.uses_rotary,
        tolerance: 1.0,
        auto_pause: app.settings.auto_pause,
    })
}

fn placement(app: &mut MyApp, ui: &mut egui::Ui) {
    let external = app
        .job
        .built
        .as_ref()
        .is_some_and(|built| built.external.is_some());
    if external {
        return;
    }
    ui.label(RichText::new(tr("place.title")).strong());
    let mut placement = app.job.placement;
    ui.horizontal(|ui| {
        ui.radio_value(&mut placement, Placement::MachineOrigin, tr("place.origin"));
        let on_head = matches!(placement, Placement::Head(_));
        if ui.radio(on_head, tr("place.head")).clicked() && !on_head {
            placement = Placement::Head(Anchor::Center);
        }
    });
    if let Placement::Head(anchor) = placement {
        ui.horizontal_wrapped(|ui| {
            let options = [
                (Anchor::Center, "place.center"),
                (Anchor::BottomLeft, "place.bottom_left"),
                (Anchor::BottomRight, "place.bottom_right"),
                (Anchor::TopLeft, "place.top_left"),
                (Anchor::TopRight, "place.top_right"),
            ];
            for (candidate, key) in options {
                if ui.selectable_label(anchor == candidate, tr(key)).clicked() {
                    placement = Placement::Head(candidate);
                }
            }
            let picked = matches!(anchor, Anchor::Picked(_));
            let label = if app.job.picking_anchor {
                tr("place.picking")
            } else {
                tr("place.pick")
            };
            if ui
                .selectable_label(picked || app.job.picking_anchor, label)
                .on_hover_text(tr("place.pick_hint"))
                .clicked()
            {
                app.job.picking_anchor = !app.job.picking_anchor;
            }
        });
        ui.label(RichText::new(tr("place.head_hint")).weak().small());
    }
    app.set_placement(placement);
    let count = app.selection.len();
    let mut only = app.job.selection_only;
    let response = ui.add_enabled(
        count > 0 || only,
        egui::Checkbox::new(&mut only, trf("place.selection_only", &[&count])),
    );
    if response
        .on_hover_text(tr("place.selection_only_hint"))
        .changed()
    {
        app.job.selection_only = only;
        app.job.options_revision += 1;
    }
}

fn job_card(app: &mut MyApp, ui: &mut egui::Ui) {
    section(ui, tr("job.title"));
    if let Some((anomaly, paused)) = app.device.anomaly.clone() {
        egui::Frame::new()
            .fill(Color32::from_rgb(200, 40, 30).gamma_multiply(0.2))
            .corner_radius(8.0)
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    RichText::new(tr("safety.title"))
                        .strong()
                        .color(Color32::from_rgb(220, 60, 50)),
                );
                ui.label(anomaly_text(&anomaly));
                if paused {
                    ui.label(tr("safety.paused"));
                }
                ui.horizontal(|ui| {
                    if paused && ui.button(tr("job.resume")).clicked() {
                        app.device.send(Command::Resume);
                        if let Some(job) = &mut app.device.job {
                            job.paused = false;
                        }
                        app.device.anomaly = None;
                    }
                    if app.device.job_running() && ui.button(tr("job.stop")).clicked() {
                        app.device.send(Command::Stop);
                        app.device.anomaly = None;
                    }
                    if ui.button(tr("safety.dismiss")).clicked() {
                        app.device.anomaly = None;
                    }
                });
            });
    }
    if let Some(job) = &app.device.job {
        let fraction = if job.total > 0 {
            job.acknowledged as f32 / job.total as f32
        } else {
            0.0
        };
        ui.add(
            egui::ProgressBar::new(fraction)
                .show_percentage()
                .desired_height(22.0),
        );
        let elapsed = job.started.elapsed();
        let remaining = job.estimate.saturating_sub(elapsed);
        ui.label(trf(
            "job.elapsed_remaining",
            &[&format_short(elapsed), &format_short(remaining)],
        ));
        let paused = job.paused;
        if let Some(reason) = app.job.waiting_pause.clone() {
            egui::Frame::new()
                .fill(Color32::from_rgb(220, 150, 20).gamma_multiply(0.2))
                .corner_radius(8.0)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(pause_text(&reason)).strong());
                    ui.label(tr("pause.how_to_resume"));
                });
        }
        ui.horizontal(|ui| {
            if paused {
                if ui
                    .add(
                        egui::Button::new(RichText::new(tr("job.resume")).strong())
                            .min_size(egui::vec2(110.0, 32.0)),
                    )
                    .clicked()
                {
                    app.device.send(Command::Resume);
                    app.job.waiting_pause = None;
                    if let Some(job) = &mut app.device.job {
                        job.paused = false;
                    }
                }
            } else if ui
                .add(
                    egui::Button::new(RichText::new(tr("job.pause")).strong())
                        .min_size(egui::vec2(110.0, 32.0)),
                )
                .clicked()
            {
                app.device.send(Command::Pause);
                if let Some(job) = &mut app.device.job {
                    job.paused = true;
                }
            }
            let stop =
                egui::Button::new(RichText::new(tr("job.stop")).strong().color(Color32::WHITE))
                    .fill(Color32::from_rgb(200, 45, 35))
                    .min_size(egui::vec2(110.0, 32.0));
            if ui.add(stop).on_hover_text(tr("job.stop_hint")).clicked() {
                app.device.send(Command::Stop);
                app.job.waiting_pause = None;
            }
        });
        overrides(app, ui);
        let line = (job_line(app))
            .filter(|_| app.device.last_preview.elapsed() > Duration::from_millis(500));
        if let Some(line) = line {
            app.job.preview_line = Some(Some(line));
            app.device.last_preview = Instant::now();
        }
        return;
    }
    placement(app, ui);
    ui.add_space(4.0);
    let Some(built) = &app.job.built else {
        ui.label(RichText::new(tr("job.nothing_run")).weak());
        return;
    };
    let estimate = built.stats.duration;
    let bounds = built.stats.cut_bounds;
    let warnings = built.warnings.clone();
    let gcode = built.gcode.clone();
    let anchor = built.anchor;
    let pauses = built.pauses.len();
    for warning in &warnings {
        ui.label(
            RichText::new(format!("⚠ {}", warning_text(warning)))
                .color(Color32::from_rgb(210, 140, 20)),
        );
    }
    ui.label(trf("job.estimate", &[&format_short(estimate)]));
    if pauses > 0 {
        ui.label(RichText::new(trf("job.pauses", &[&pauses])).weak());
    }
    let connected = app.device.is_connected();
    let idle = app.device.state() == MachineState::Idle;
    let current = app.job_is_current();
    let blockers = if connected {
        start_blockers(app)
    } else {
        Vec::new()
    };
    for blocker in &blockers {
        ui.label(RichText::new(format!("⛔ {blocker}")).color(Color32::from_rgb(210, 50, 40)));
    }
    if connected && !app.device.homed && anchor.is_none() {
        ui.label(RichText::new(tr("safety.not_homed")).weak().small());
    }
    ui.checkbox(&mut app.settings.auto_pause, tr("safety.auto_pause"))
        .on_hover_text(tr("safety.auto_pause_hint"));
    ui.horizontal(|ui| {
        ui.label(tr("job.frame_power"));
        ui.add(egui::Slider::new(&mut app.settings.frame_power, 0.0..=5.0).suffix(" %"));
    });
    ui.horizontal(|ui| {
        let frame = ui
            .add_enabled(
                connected && idle && !bounds.is_empty() && blockers.is_empty(),
                egui::Button::new(tr("job.frame")).min_size(egui::vec2(100.0, 32.0)),
            )
            .on_hover_text(tr("job.frame_hint"));
        if frame.clicked() {
            let program = mml_job::frame_job(
                bounds,
                anchor,
                &app.machine,
                app.settings.frame_power,
                3000.0,
            );
            app.device.send(Command::StartJob {
                lines: program.lines().map(str::to_owned).collect(),
                guard: None,
            });
            app.device.job = Some(RunningJob {
                total: 0,
                acknowledged: 0,
                started: Instant::now(),
                estimate: Duration::ZERO,
                paused: false,
            });
        }
        let start = egui::Button::new(
            RichText::new(tr("job.start"))
                .strong()
                .color(Color32::WHITE)
                .size(16.0),
        )
        .fill(Color32::from_rgb(40, 140, 80))
        .min_size(egui::vec2(150.0, 32.0));
        let out_of_bounds =
            warnings.contains(&mml_job::JobWarning::OutOfBounds) && anchor.is_none();
        let response = ui.add_enabled(
            connected && idle && current && !out_of_bounds && blockers.is_empty(),
            start,
        );
        let response = if !connected {
            response.on_disabled_hover_text(tr("job.need_connection"))
        } else if !idle {
            response.on_disabled_hover_text(tr("job.need_idle"))
        } else if out_of_bounds {
            response.on_disabled_hover_text(tr("warn.out_of_bounds"))
        } else {
            response
        };
        if response.clicked() {
            let guard = guard_for(app);
            app.device.send(Command::StartJob {
                lines: gcode.lines().map(str::to_owned).collect(),
                guard,
            });
            app.device.job = Some(RunningJob {
                total: gcode.lines().count(),
                acknowledged: 0,
                started: Instant::now(),
                estimate,
                paused: false,
            });
            app.device.alarm = None;
            app.device.anomaly = None;
            app.job.waiting_pause = None;
            app.notify(tr("job.started"), Level::Info);
        }
    });
}

fn job_line(app: &MyApp) -> Option<usize> {
    let job = app.device.job.as_ref()?;
    let built = app.job.built.as_ref()?;
    let total = job.total.max(1);
    // Comment-free lines are streamed; map progress back to source lines.
    Some(job.acknowledged * built.program.line_count / total)
}

fn overrides(app: &mut MyApp, ui: &mut egui::Ui) {
    let current = app
        .device
        .status
        .as_ref()
        .map_or([100, 100, 100], |status| status.overrides);
    ui.horizontal(|ui| {
        ui.label(trf("job.feed_override", &[&current[0]]));
        if ui.small_button("−10").clicked() {
            app.device.send(Command::Realtime(realtime::FEED_MINUS_10));
        }
        if ui.small_button("100").clicked() {
            app.device.send(Command::Realtime(realtime::FEED_100));
        }
        if ui.small_button("+10").clicked() {
            app.device.send(Command::Realtime(realtime::FEED_PLUS_10));
        }
    });
    ui.horizontal(|ui| {
        ui.label(trf("job.power_override", &[&current[2]]));
        if ui.small_button("−10").clicked() {
            app.device
                .send(Command::Realtime(realtime::SPINDLE_MINUS_10));
        }
        if ui.small_button("100").clicked() {
            app.device.send(Command::Realtime(realtime::SPINDLE_100));
        }
        if ui.small_button("+10").clicked() {
            app.device
                .send(Command::Realtime(realtime::SPINDLE_PLUS_10));
        }
    });
}

fn format_short(duration: Duration) -> String {
    let seconds = duration.as_secs();
    format!(
        "{}:{:02}:{:02}",
        seconds / 3600,
        (seconds % 3600) / 60,
        seconds % 60
    )
}

fn console(app: &mut MyApp, ui: &mut egui::Ui) {
    egui::CollapsingHeader::new(RichText::new(tr("console.title")).strong())
        .default_open(false)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .stick_to_bottom(true)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for (kind, text) in &app.device.console {
                        let color = match kind {
                            ConsoleKind::Sent => ui.visuals().strong_text_color(),
                            ConsoleKind::Received => ui.visuals().weak_text_color(),
                            ConsoleKind::Error => Color32::from_rgb(210, 60, 50),
                            ConsoleKind::Info => Color32::from_rgb(60, 140, 210),
                        };
                        let prefix = if *kind == ConsoleKind::Sent {
                            "› "
                        } else {
                            ""
                        };
                        ui.label(
                            RichText::new(format!("{prefix}{text}"))
                                .monospace()
                                .color(color),
                        );
                    }
                });
            let running = app.device.job_running();
            ui.horizontal(|ui| {
                let response = ui.add_enabled(
                    !running,
                    egui::TextEdit::singleline(&mut app.device.console_input)
                        .hint_text(tr("console.hint"))
                        .desired_width(220.0)
                        .font(egui::TextStyle::Monospace),
                );
                let submit =
                    response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                if (ui
                    .add_enabled(!running, egui::Button::new(tr("console.send")))
                    .clicked()
                    || submit)
                    && !app.device.console_input.trim().is_empty()
                {
                    let line = std::mem::take(&mut app.device.console_input);
                    app.device.command_history.push(line.clone());
                    app.device.line(line);
                    response.request_focus();
                }
            });
            ui.label(RichText::new(tr("macros.title")).strong());
            ui.horizontal_wrapped(|ui| {
                let macros = app.settings.macros.clone();
                for (index, entry) in macros.iter().enumerate() {
                    let response = ui
                        .add_enabled(!running, egui::Button::new(&entry.name))
                        .on_hover_text(&entry.gcode);
                    if response.clicked() {
                        for line in entry.gcode.lines() {
                            app.device.line(line.to_owned());
                        }
                    }
                    response.context_menu(|ui| {
                        if ui.button(tr("macros.remove")).clicked() {
                            app.settings.macros.remove(index);
                        }
                    });
                }
                if ui
                    .small_button("+")
                    .on_hover_text(tr("macros.add"))
                    .clicked()
                {
                    app.device.new_macro = Some((String::new(), String::new()));
                }
            });
            let mut save = false;
            let mut cancel = false;
            if let Some((name, gcode)) = &mut app.device.new_macro {
                ui.add(egui::TextEdit::singleline(name).hint_text(tr("macros.name")));
                ui.add(
                    egui::TextEdit::multiline(gcode)
                        .code_editor()
                        .desired_rows(2)
                        .hint_text("G0 X0 Y0"),
                );
                ui.horizontal(|ui| {
                    save = ui.button(tr("materials.save")).clicked();
                    cancel = ui.button(tr("dialog.cancel")).clicked();
                });
            }
            if save
                && let Some((name, gcode)) = app.device.new_macro.take()
                && !name.trim().is_empty()
            {
                app.settings
                    .macros
                    .push(crate::settings::Macro { name, gcode });
            }
            if cancel {
                app.device.new_macro = None;
            }
        });
}
