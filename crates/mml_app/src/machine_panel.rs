use crate::app::{Anchor, Placement};
use crate::app::{Level, MyApp};
use crate::i18n::{carvera_halt, grbl_alarm, grbl_error, tr, trf};
use crate::panels::{section, warning_text};
use crate::settings::Settings;
use eframe::egui::{self, Color32, DragValue, RichText};
use mml_core::machine::MachineProfile;
use mml_device::detect::{Found, Link};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    Main,
    /// Dry run on the outline (frame), to be approved before the job.
    Frame,
    ProbeOrigin,
    ProbeGrid,
    ProbePlane,
    /// Touch of the installed tool on the tool setter.
    ToolSetter,
    /// Touch of the calibration gauge on the tool setter.
    SetterReference,
    /// One axis of a centre-finding cycle.
    ProbeCenter,
}

pub struct RunningJob {
    pub total: usize,
    pub acknowledged: usize,
    pub started: Instant,
    pub estimate: Duration,
    pub paused: bool,
    pub kind: JobKind,
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
    pub protocol: mml_device::Protocol,
    last_preview: Instant,
    pub new_macro: Option<(String, String)>,
    /// Last anomaly and whether the job was paused for it.
    pub anomaly: Option<(Anomaly, bool)>,
    pub homing_results: Vec<(Duration, bool)>,
    pub reached_pause: Option<usize>,
    pub homed: bool,
    pub probe_reports: Vec<([f64; 3], bool)>,
    pub finished: Option<(JobKind, JobOutcome)>,
    /// Duration of the last finished job.
    pub last_elapsed: Duration,
    /// A pause or tool change happened: the measured length is stale.
    pub tool_maybe_changed: bool,
    pub z_probed: bool,
    /// Continuous jog in progress: direction and whether the long jog
    /// was sent (held longer than a click).
    pub hold: Option<([f64; 3], Instant, bool)>,
    /// Direction held this frame on the jog pad or the keyboard, read
    /// once per frame by `drive_jog`.
    pub jog_pad: Option<[f64; 3]>,
    pub jog_keys: Option<[f64; 3]>,
    /// Precision change held on the keyboard: +1 coarser (Shift), -1
    /// finer (Alt).
    pub jog_shift: i32,
    /// Spindle started by hand from the jog card (M3 sent).
    pub spindle_on: bool,
    /// Why the last homing failed, until the next one starts.
    pub homing_failure: Option<mml_device::homing::Failure>,
    /// Network search for Carvera machines in progress.
    discovery: Option<std::sync::mpsc::Receiver<Vec<Found>>>,
    pub discovered: Option<Vec<Found>>,
    /// Probe USB ports too (resets Arduino boards).
    pub detect_usb: bool,
    pub homing: Option<ActiveHoming>,
    /// Lines the controller had accepted and the head position when the
    /// last job stopped early.
    pub interrupted: Option<(usize, [f64; 3])>,
}

/// Homing started from the application, supervised every frame.
pub struct ActiveHoming {
    started: Instant,
    /// Answer of the controller to `$H`: success or failure.
    done: Option<bool>,
    hold_required: bool,
    /// A homing button was seen held since the last check.
    held: bool,
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
            protocol: connection.protocol,
            last_preview: Instant::now(),
            new_macro: None,
            anomaly: None,
            homing_results: Vec::new(),
            reached_pause: None,
            homed: false,
            probe_reports: Vec::new(),
            finished: None,
            last_elapsed: Duration::ZERO,
            tool_maybe_changed: false,
            z_probed: false,
            hold: None,
            jog_pad: None,
            jog_keys: None,
            jog_shift: 0,
            spindle_on: false,
            homing_failure: None,
            discovery: None,
            discovered: None,
            detect_usb: true,
            homing: None,
            interrupted: None,
        }
    }

    pub fn store_connection(&self, settings: &mut Settings) {
        settings.connection.use_tcp = self.use_tcp;
        settings.connection.serial_port = self.serial_port.clone();
        settings.connection.baud = self.baud;
        settings.connection.host = self.host.clone();
        settings.connection.tcp_port = self.tcp_port;
        settings.connection.protocol = self.protocol;
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
        self.controller = Some(Controller::connect_with(connection, self.protocol));
        self.connecting = true;
    }

    pub fn disconnect(&mut self) {
        self.spindle_on = false;
        self.controller = None;
        self.connecting = false;
        self.banner = None;
        self.status = None;
        self.job = None;
        self.log(ConsoleKind::Info, tr("device.disconnected"));
    }

    pub fn send(&mut self, command: Command) {
        // A job, a stop or a reset leaves the spindle to the program or
        // stops it: the hand-started spindle is no longer ours to show.
        if matches!(
            command,
            Command::StartJob { .. }
                | Command::Stop
                | Command::EmergencyStop
                | Command::Realtime(realtime::SOFT_RESET)
        ) {
            self.spindle_on = false;
        }
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

    /// Sends `$H` and supervises it: with `hold_required`, the homing
    /// stops as soon as no homing button is held any more.
    pub fn start_homing(&mut self, hold_required: bool) {
        if self.job_running() || self.homing.is_some() {
            return;
        }
        self.line("$H");
        self.homing_failure = None;
        self.homing = Some(ActiveHoming {
            started: Instant::now(),
            done: None,
            hold_required,
            held: true,
        });
    }

    /// Resets the controller when the operator let go of the button, or
    /// when homing lasts longer than it usually does.
    pub fn supervise_homing(
        &mut self,
        ctx: &egui::Context,
        history: &[f64],
        notices: &mut Vec<(String, Level, Instant)>,
    ) {
        use mml_device::homing::{self, HomingRun, StopReason, Verdict};
        let Some(active) = &mut self.homing else {
            return;
        };
        let escape = ctx.input(|input| input.key_pressed(egui::Key::Escape));
        let travel = [130, 131, 132].map(|key| {
            self.settings
                .get(&key)
                .and_then(|value| value.trim().parse().ok())
                .unwrap_or(1000.0)
        });
        let seek = self
            .settings
            .get(&25)
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(500.0);
        let limit = homing::time_limit(history, travel, seek);
        let run = HomingRun {
            elapsed: active.started.elapsed(),
            done: active.done.is_some(),
            hold_required: active.hold_required,
        };
        // The press started on a homing button; from then on only letting
        // go counts. The button itself may move or vanish as soon as the
        // machine state changes (the alarm card closes when grblHAL
        // reports Home), which must not read as a release.
        let pointer = ctx.input(|input| input.pointer.primary_down() || input.any_touches());
        let held = (std::mem::take(&mut active.held) || pointer) && !escape;
        let verdict = match homing::supervise(run, held, limit) {
            Verdict::Continue if escape => Verdict::Stop(StopReason::Released),
            verdict => verdict,
        };
        match verdict {
            Verdict::Continue => ctx.request_repaint_after(Duration::from_millis(50)),
            Verdict::Finished => {
                if active.done == Some(true) {
                    self.homing_results.push((run.elapsed, true));
                }
                self.homing = None;
            }
            Verdict::Stop(reason) => {
                self.homing = None;
                self.homing_failure = Some(match reason {
                    StopReason::Released => homing::Failure::Aborted,
                    StopReason::TooLong => homing::Failure::TooLong,
                });
                self.send(Command::Realtime(realtime::SOFT_RESET));
                let text = match reason {
                    StopReason::Released => tr("homing.stopped_released").to_owned(),
                    StopReason::TooLong => trf(
                        "homing.stopped_long",
                        &[&format!("{:.0}", limit.as_secs_f64())],
                    ),
                };
                self.log(ConsoleKind::Error, text.clone());
                notices.push((text, Level::Warning, Instant::now()));
            }
        }
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
                    self.homing = None;
                    self.spindle_on = false;
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
                Event::Received(line) => {
                    if let Some(report) = mml_probe::parse_probe_report(&line) {
                        self.probe_reports.push(report);
                    }
                    self.log(ConsoleKind::Received, line);
                }
                Event::Acknowledged(line) => {
                    if let Some(homing) = &mut self.homing
                        && line.trim() == "$H"
                    {
                        homing.done = Some(true);
                    }
                }
                Event::Error { code, line } => {
                    if let Some(homing) = &mut self.homing
                        && line.trim() == "$H"
                    {
                        homing.done = Some(false);
                        self.homing_failure = Some(mml_device::homing::Failure::from_error(code));
                    }
                    let text = format!("error:{code} — {} ({line})", grbl_error(code));
                    self.log(ConsoleKind::Error, text.clone());
                    notices.push((text, Level::Error, Instant::now()));
                }
                Event::Alarm(code) => {
                    if let Some(homing) = &mut self.homing {
                        homing.done = Some(false);
                        self.homing_failure = Some(mml_device::homing::Failure::from_alarm(code));
                    }
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
                Event::HomingFinished { success, .. } => {
                    // Durations are measured from the reply to `$H`, which
                    // plain GRBL sends without any Home status report.
                    self.homed = success;
                }
                Event::ProgramPaused { index } => {
                    self.reached_pause = Some(index);
                    // A pause is where tools and heads are changed.
                    self.tool_maybe_changed = true;
                    if let Some(job) = &mut self.job {
                        job.paused = true;
                    }
                }
                Event::JobFinished { outcome, elapsed } => {
                    self.last_elapsed = elapsed;
                    if let Some(job) = self.job.take() {
                        if job.kind == JobKind::Main {
                            self.interrupted = (outcome != JobOutcome::Completed).then(|| {
                                let position =
                                    self.status.as_ref().map_or([0.0; 3], |s| s.work_position);
                                (job.acknowledged, position)
                            });
                        }
                        self.finished = Some((job.kind, outcome));
                        if job.kind != JobKind::Main {
                            continue;
                        }
                    }
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
                crate::theme::WARNING,
            )
        } else if self.controller.is_some() {
            let state = self.state();
            (format!("⏺ {}", state_label(state)), state_color(state))
        } else {
            (format!("○ {}", tr("device.offline")), Color32::GRAY)
        };
        ui.label(RichText::new(text).color(color).strong());
    }
}

pub fn state_label(state: MachineState) -> &'static str {
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
        MachineState::Idle => crate::theme::SUCCESS,
        MachineState::Run | MachineState::Jog | MachineState::Home => {
            Color32::from_rgb(40, 120, 220)
        }
        MachineState::Hold | MachineState::HoldComplete | MachineState::Door => {
            crate::theme::WARNING
        }
        MachineState::Alarm => crate::theme::DANGER,
        _ => Color32::GRAY,
    }
}

pub fn run_panel(app: &mut MyApp, ui: &mut egui::Ui) {
    use crate::theme::card;
    card(ui, |ui| connection(app, ui));
    if !app.device.is_connected() {
        card(ui, |ui| job_card(app, ui));
        return;
    }
    card(ui, |ui| machine_state(app, ui));
    card(ui, |ui| job_card(app, ui));
    card(ui, |ui| jog(app, ui));
    card(ui, |ui| crate::manual_probe::section(app, ui));
    card(ui, |ui| crate::workflow::probe_section(app, ui));
    if app.machine.kind == mml_core::machine::MachineKind::Cnc {
        card(ui, |ui| crate::collision::card(app, ui));
    }
    card(ui, |ui| crate::workflow::basics_section(app, ui));
    if app.settings.advanced {
        card(ui, |ui| console(app, ui));
        card(ui, |ui| crate::workflow::controller_settings(app, ui));
    }
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
        ui.separator();
        ui.selectable_value(&mut device.protocol, mml_device::Protocol::Grbl, "GRBL")
            .on_hover_text(tr("device.protocol_grbl"));
        ui.selectable_value(
            &mut device.protocol,
            mml_device::Protocol::SendAndWait,
            tr("device.protocol_other"),
        )
        .on_hover_text(tr("device.protocol_other_hint"));
    });
    discovery(device, ui);
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

/// Finds machines on USB and on the network and fills the connection
/// of the one picked.
fn discovery(device: &mut DeviceState, ui: &mut egui::Ui) {
    if let Some(receiver) = &device.discovery {
        match receiver.try_recv() {
            Ok(found) => {
                device.discovered = Some(found);
                device.discovery = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(tr("device.discovering")).weak());
                });
                ui.ctx().request_repaint_after(Duration::from_millis(200));
                return;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => device.discovery = None,
        }
    }
    ui.horizontal(|ui| {
        if ui
            .button(tr("device.discover"))
            .on_hover_text(tr("device.discover_hint"))
            .clicked()
        {
            device.discovered = None;
            device.discovery = Some(mml_device::detect::detect_in_background(device.detect_usb));
        }
        ui.checkbox(&mut device.detect_usb, tr("device.detect_usb"))
            .on_hover_text(tr("device.detect_usb_hint"));
    });
    let mut picked = None;
    match &device.discovered {
        Some(found) if found.is_empty() => {
            ui.label(RichText::new(tr("device.discover_none")).weak());
        }
        Some(found) => {
            for item in found {
                let place = match &item.link {
                    Link::Serial { port, baud } => format!("USB {port} @ {baud}"),
                    Link::Network { host, port } => format!("{host}:{port}"),
                };
                let text = format!("{}  ·  {place}  {}", item.firmware.name(), item.details);
                if ui
                    .button(text)
                    .on_hover_text(tr("device.discover_pick"))
                    .clicked()
                {
                    picked = Some(item.clone());
                }
            }
        }
        None => {}
    }
    if let Some(item) = picked {
        match item.link {
            Link::Serial { port, baud } => {
                device.use_tcp = false;
                device.serial_port = port;
                device.baud = baud;
            }
            Link::Network { host, port } => {
                device.use_tcp = true;
                device.host = host;
                device.tcp_port = port;
            }
        }
        device.protocol = item.firmware.protocol();
        device.discovered = None;
    }
}

fn machine_state(app: &mut MyApp, ui: &mut egui::Ui) {
    let hold = app.settings.homing_hold;
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
                if let Some(code) = device.status.as_ref().and_then(|status| status.halt) {
                    ui.label(carvera_halt(code));
                } else if let Some(code) = device.alarm {
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
                if let Some(tool) = status.tool {
                    ui.label(RichText::new(trf("device.tool_loaded", &[&tool])).weak());
                }
                if !status.pins.is_empty() {
                    ui.label(
                        RichText::new(trf("device.pins", &[&status.pins]))
                            .color(crate::theme::WARNING),
                    );
                }
            }
        });
    ui.horizontal_wrapped(|ui| {
        homing_button(device, ui, hold, tr("device.home"));
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
    gentle_homing_advice(device, ui);
    homing_doctor(device, ui);
}

/// Precision levels of the jog, finest first: step in mm and name.
pub const JOG_LEVELS: [(f64, &str); 5] = [
    (0.01, "jog.level_micro"),
    (0.1, "jog.level_fine"),
    (1.0, "jog.level_normal"),
    (10.0, "jog.level_fast"),
    (50.0, "jog.level_rapid"),
];

/// Index of the level nearest to `step`.
pub fn jog_level(step: f64) -> usize {
    JOG_LEVELS
        .iter()
        .enumerate()
        .min_by(|a, b| {
            (a.1.0.ln() - step.max(1e-6).ln())
                .abs()
                .total_cmp(&(b.1.0.ln() - step.max(1e-6).ln()).abs())
        })
        .map_or(2, |(index, _)| index)
}

/// Step in use: the chosen level, one coarser while Shift is held, one
/// finer while Alt is held.
fn effective_step(app: &MyApp) -> f64 {
    let level = jog_level(app.settings.jog_step) as i32 + app.device.jog_shift;
    JOG_LEVELS[level.clamp(0, JOG_LEVELS.len() as i32 - 1) as usize].0
}

fn step_feed(app: &MyApp, step: f64) -> f64 {
    if app.settings.jog_proportional {
        (step * 300.0).clamp(30.0, app.settings.jog_feed)
    } else {
        app.settings.jog_feed
    }
}

/// Jog pad: a click moves one step, holding a button moves until it is
/// released (long jog cancelled with the GRBL jog-cancel command), so
/// the head can be brought quickly and then precisely into place.
fn jog(app: &mut MyApp, ui: &mut egui::Ui) {
    section(ui, tr("jog.title"));
    let busy = app.device.job_running();
    let step = effective_step(app);
    let feed = step_feed(app, step);
    let size = egui::vec2(44.0, 38.0);
    let mut pressed: Option<[f64; 3]> = None;
    let mut go_zero = false;
    ui.add_enabled_ui(!busy, |ui| {
        ui.horizontal(|ui| {
            egui::Grid::new("jog-pad")
                .spacing([4.0, 4.0])
                .show(ui, |ui| {
                    let buttons = [
                        [(-1.0, 1.0), (0.0, 1.0), (1.0, 1.0)],
                        [(-1.0, 0.0), (0.0, 0.0), (1.0, 0.0)],
                        [(-1.0, -1.0), (0.0, -1.0), (1.0, -1.0)],
                    ];
                    for row in buttons {
                        for (dx, dy) in row {
                            if dx == 0.0 && dy == 0.0 {
                                let home =
                                    egui::Button::new(RichText::new("0").size(16.0).strong())
                                        .min_size(size);
                                go_zero |=
                                    ui.add(home).on_hover_text(tr("device.goto_zero")).clicked();
                                continue;
                            }
                            let angle = (dy as f32).atan2(dx as f32).to_degrees();
                            if crate::icons::arrow_button(ui, angle, size)
                                .is_pointer_button_down_on()
                            {
                                pressed = Some([dx, dy, 0.0]);
                            }
                        }
                        ui.end_row();
                    }
                });
            ui.vertical(|ui| {
                for (label, dz) in [("Z+", 1.0), ("Z-", -1.0)] {
                    let button = egui::Button::new(RichText::new(label).size(16.0))
                        .min_size(size)
                        .sense(egui::Sense::click_and_drag());
                    if ui.add(button).is_pointer_button_down_on() {
                        pressed = Some([0.0, 0.0, dz]);
                    }
                }
            });
        });
        ui.label(tr("jog.precision"));
        let current = jog_level(app.settings.jog_step);
        let active = jog_level(step);
        ui.horizontal_wrapped(|ui| {
            for (index, (value, key)) in JOG_LEVELS.iter().enumerate() {
                let text = format!("{}\n{value} mm", tr(key));
                let mut label = RichText::new(text).small();
                if index == active && active != current {
                    // Held modifier: show the level really used.
                    label = label.color(ui.visuals().warn_fg_color);
                }
                if ui
                    .selectable_label(index == current, label)
                    .on_hover_text(tr("jog.precision_hint"))
                    .clicked()
                {
                    app.settings.jog_step = *value;
                }
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
            if app.settings.jog_proportional {
                ui.label(
                    RichText::new(trf("jog.step_feed", &[&format!("{feed:.0}")]))
                        .weak()
                        .small(),
                );
            }
        });
        if app.machine.kind == mml_core::machine::MachineKind::Cnc {
            spindle_row(app, ui);
        }
        ui.collapsing(tr("jog.keys_title"), |ui| {
            egui::Grid::new("jog-keys").num_columns(2).show(ui, |ui| {
                for (keys, action) in [
                    ("← → ↑ ↓", "jog.keys_xy"),
                    ("Page ↑ / Page ↓", "jog.keys_z"),
                    ("1 … 5", "jog.keys_levels"),
                    ("+ / −", "jog.keys_plus_minus"),
                    ("Maj / Alt", "jog.keys_modifiers"),
                ] {
                    ui.label(RichText::new(keys).monospace());
                    ui.label(tr(action));
                    ui.end_row();
                }
            });
            ui.label(RichText::new(tr("jog.keys_hold")).weak().small());
        });
    });
    if go_zero {
        app.device.line("G90 G0 X0 Y0");
    }
    if !busy {
        app.device.jog_pad = pressed;
    }
}

/// Keyboard part of the jog, called with the input of the frame while
/// the Run step is shown and nothing is being typed.
pub fn jog_keyboard(app: &mut MyApp, ctx: &egui::Context) {
    use egui::Key;
    let (direction, shift, levels, finer, coarser) = ctx.input(|input| {
        let down = |key| input.key_down(key);
        let axis = |plus, minus| f64::from(down(plus) as i8 - down(minus) as i8);
        let direction = [
            axis(Key::ArrowRight, Key::ArrowLeft),
            axis(Key::ArrowUp, Key::ArrowDown),
            axis(Key::PageUp, Key::PageDown),
        ];
        let shift = if input.modifiers.shift {
            1
        } else if input.modifiers.alt {
            -1
        } else {
            0
        };
        let plain = !input.modifiers.command && !input.modifiers.ctrl;
        let levels = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5]
            .iter()
            .position(|key| plain && input.key_pressed(*key));
        (
            direction,
            shift,
            levels,
            plain && input.key_pressed(Key::Minus),
            plain && (input.key_pressed(Key::Plus) || input.key_pressed(Key::Equals)),
        )
    });
    // Z alone: a diagonal with Z is never what was meant.
    let direction = if direction[2] != 0.0 {
        Some([0.0, 0.0, direction[2]])
    } else if direction[0] != 0.0 || direction[1] != 0.0 {
        Some(direction)
    } else {
        None
    };
    app.device.jog_keys = direction;
    app.device.jog_shift = shift;
    let mut level = jog_level(app.settings.jog_step);
    if let Some(chosen) = levels {
        level = chosen;
    }
    if finer {
        level = level.saturating_sub(1);
    }
    if coarser {
        level = (level + 1).min(JOG_LEVELS.len() - 1);
    }
    app.settings.jog_step = JOG_LEVELS[level].0;
}

/// True while the spindle turns: reported by the controller, or started
/// here and not stopped (controllers without spindle speed report).
pub fn spindle_turning(app: &MyApp) -> bool {
    let reported = app
        .device
        .status
        .as_ref()
        .is_some_and(|status| status.spindle > 0.0);
    reported || app.device.spindle_on
}

/// Spindle on / off next to the jog pad: the tool turns while it
/// approaches the material (it cuts a little instead of breaking) and
/// for manual work with the jog.
fn spindle_row(app: &mut MyApp, ui: &mut egui::Ui) {
    ui.separator();
    let turning = spindle_turning(app);
    ui.horizontal_wrapped(|ui| {
        if turning {
            let speed = app
                .device
                .status
                .as_ref()
                .map_or(app.settings.idle_rpm, |status| status.spindle);
            ui.label(
                RichText::new(trf("spindle.turning", &[&format!("{speed:.0}")]))
                    .strong()
                    .color(crate::theme::WARNING),
            );
            let stop = egui::Button::new(
                RichText::new(tr("spindle.stop"))
                    .color(Color32::WHITE)
                    .strong(),
            )
            .fill(crate::theme::DANGER);
            if ui.add(stop).clicked() {
                app.device.line("M5");
                app.device.spindle_on = false;
            }
        } else {
            ui.label(tr("spindle.label"));
            ui.add(
                DragValue::new(&mut app.settings.idle_rpm)
                    .range(100.0..=60_000.0)
                    .suffix(" rpm")
                    .speed(100.0),
            );
            let idle = app.device.state() == MachineState::Idle;
            if ui
                .add_enabled(idle, egui::Button::new(tr("spindle.start")))
                .on_hover_text(tr("spindle.start_hint"))
                .clicked()
            {
                app.device.line(format!("M3 S{:.0}", app.settings.idle_rpm));
                // Jogs sent next wait until the spindle is up to speed.
                app.device.line(format!("G4 P{SPIN_UP:.1}"));
                app.device.spindle_on = true;
            }
        }
    });
    let hint = if turning {
        tr("spindle.turning_hint")
    } else {
        tr("spindle.hint")
    };
    ui.label(RichText::new(hint).weak().small());
}

/// Seconds the spindle takes to reach its speed.
const SPIN_UP: f64 = 3.0;
/// Jog speed limits while the spindle turns, mm/min: the tool may be
/// cutting, so it never rushes into the material.
const CUTTING_JOG: f64 = 1000.0;
const CUTTING_PLUNGE: f64 = 300.0;

/// Runs the jog from what is held this frame: a short press moves one
/// step on release, a press longer than 300 ms moves until released.
/// Called every frame whatever is shown, so letting go always stops.
pub fn drive_jog(app: &mut MyApp) {
    let pad = app.device.jog_pad.take();
    let keys = app.device.jog_keys.take();
    let pressed = if app.device.job_running() || !app.device.is_connected() {
        None
    } else {
        pad.or(keys)
    };
    let step = effective_step(app);
    let mut feed = step_feed(app, step);
    if spindle_turning(app) {
        let going_down = pressed
            .or(app.device.hold.map(|(direction, _, _)| direction))
            .is_some_and(|direction| direction[2] < 0.0);
        feed = feed.min(if going_down {
            CUTTING_PLUNGE
        } else {
            CUTTING_JOG
        });
    }
    if pad.is_none() && keys.is_none() {
        app.device.jog_shift = 0;
    }
    let held = app.device.hold;
    match (pressed, held) {
        (Some(direction), None) => app.device.hold = Some((direction, Instant::now(), false)),
        (Some(direction), Some((current, since, false)))
            if direction == current && since.elapsed() > Duration::from_millis(300) =>
        {
            // Long jog towards the travel end; jog() clamps it to the work area.
            let [dx, dy, dz] = direction;
            let z_reach = app.machine.z_travel.max(10.0);
            // Held, the head moves at the speed of the level: slow when
            // fine, so it can be stopped right on the spot.
            app.device.jog(
                dx * 10_000.0,
                dy * 10_000.0,
                dz * z_reach,
                feed,
                &app.machine,
            );
            app.device.hold = Some((direction, since, true));
        }
        (Some(direction), Some((current, _, continuous))) if direction != current => {
            // Direction changed while held: stop, then start again.
            if continuous {
                app.device.send(Command::Realtime(realtime::JOG_CANCEL));
            }
            app.device.hold = Some((direction, Instant::now(), false));
        }
        (None, Some((direction, _, continuous))) => {
            app.device.hold = None;
            if continuous {
                app.device.send(Command::Realtime(realtime::JOG_CANCEL));
            } else {
                let [dx, dy, dz] = direction;
                app.device.jog(
                    dx * step,
                    dy * step,
                    dz * step.min(10.0),
                    feed,
                    &app.machine,
                );
            }
        }
        _ => {}
    }
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
pub fn start_blockers(app: &MyApp) -> Vec<String> {
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

pub fn guard_for(app: &MyApp) -> Option<JobGuard> {
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
    rotary(app, ui);
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

/// Simplified 4-axis engraving: the design is drawn flat and wrapped
/// around the cylinder.
fn rotary(app: &mut MyApp, ui: &mut egui::Ui) {
    use mml_job::Rotary;
    let mut rotary = app.job.rotary;
    ui.horizontal(|ui| {
        ui.label(tr("rotary.title"));
        let diameter = match rotary {
            Some(Rotary::AxisA { diameter } | Rotary::ReplaceY { diameter, .. }) => diameter,
            None => 60.0,
        };
        if ui
            .selectable_label(rotary.is_none(), tr("rotary.off"))
            .clicked()
        {
            rotary = None;
        }
        if ui
            .selectable_label(
                matches!(rotary, Some(Rotary::AxisA { .. })),
                tr("rotary.axis_a"),
            )
            .on_hover_text(tr("rotary.axis_a_hint"))
            .clicked()
        {
            rotary = Some(Rotary::AxisA { diameter });
        }
        if ui
            .selectable_label(
                matches!(rotary, Some(Rotary::ReplaceY { .. })),
                tr("rotary.replace_y"),
            )
            .on_hover_text(tr("rotary.replace_y_hint"))
            .clicked()
        {
            rotary = Some(Rotary::ReplaceY {
                diameter,
                y_per_rotation: 100.0,
            });
        }
    });
    match &mut rotary {
        Some(Rotary::AxisA { diameter }) => {
            ui.horizontal(|ui| {
                ui.label(tr("rotary.diameter"));
                ui.add(
                    DragValue::new(diameter)
                        .range(1.0..=500.0)
                        .suffix(" mm")
                        .speed(0.1),
                );
            });
        }
        Some(Rotary::ReplaceY {
            diameter,
            y_per_rotation,
        }) => {
            ui.horizontal(|ui| {
                ui.label(tr("rotary.diameter"));
                ui.add(
                    DragValue::new(diameter)
                        .range(1.0..=500.0)
                        .suffix(" mm")
                        .speed(0.1),
                );
                ui.label(tr("rotary.y_per_rotation"))
                    .on_hover_text(tr("rotary.y_per_rotation_hint"));
                ui.add(
                    DragValue::new(y_per_rotation)
                        .range(1.0..=5000.0)
                        .suffix(" mm")
                        .speed(0.5),
                );
            });
        }
        None => {}
    }
    if let Some(value) = rotary {
        ui.label(
            RichText::new(trf(
                "rotary.circumference",
                &[&format!("{:.1}", value.circumference())],
            ))
            .weak()
            .small(),
        );
    }
    if rotary != app.job.rotary {
        app.job.rotary = rotary;
        app.job.options_revision += 1;
    }
}

fn job_card(app: &mut MyApp, ui: &mut egui::Ui) {
    section(ui, tr("job.title"));
    if let Some((anomaly, paused)) = app.device.anomaly.clone() {
        egui::Frame::new()
            .fill(crate::theme::DANGER.gamma_multiply(0.2))
            .corner_radius(8.0)
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    RichText::new(tr("safety.title"))
                        .strong()
                        .color(crate::theme::DANGER),
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
                .fill(crate::theme::WARNING.gamma_multiply(0.2))
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
                    .fill(crate::theme::DANGER)
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
    crate::workflow::frame_review(app, ui);
    placement(app, ui);
    ui.add_space(4.0);
    let Some(built) = &app.job.built else {
        ui.label(RichText::new(tr("job.nothing_run")).weak());
        return;
    };
    let estimate = built.stats.duration;
    let bounds = built.stats.cut_bounds;
    let warnings = built.warnings.clone();
    let anchor = built.anchor;
    let pauses = built.pauses.len();
    for warning in &warnings {
        ui.label(
            RichText::new(format!("⚠ {}", warning_text(warning))).color(crate::theme::WARNING),
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
        ui.label(RichText::new(format!("⛔ {blocker}")).color(crate::theme::DANGER));
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
    let gates = crate::workflow::start_gates(app);
    for gate in &gates {
        ui.label(RichText::new(format!("🔒 {gate}")).color(crate::theme::WARNING));
    }
    let out_of_bounds = warnings.contains(&mml_job::JobWarning::OutOfBounds) && anchor.is_none();
    let can_start =
        connected && idle && current && !out_of_bounds && blockers.is_empty() && gates.is_empty();
    resume_card(
        app,
        ui,
        connected && idle && blockers.is_empty() && gates.is_empty(),
    );
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
            crate::workflow::run_auxiliary(
                app,
                program.lines().map(str::to_owned).collect(),
                JobKind::Frame,
            );
            app.job.frame_for = Some(app.job_key());
        }
        // The green fill only when Start is really possible, so a
        // disabled button never looks ready.
        let label = RichText::new(tr("job.start")).strong().size(16.0);
        let start = if can_start {
            egui::Button::new(label.color(Color32::WHITE)).fill(crate::theme::SUCCESS)
        } else {
            egui::Button::new(label)
        }
        .min_size(egui::vec2(150.0, 34.0));
        let response = ui.add_enabled(can_start, start);
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
            if app.settings.barriers {
                app.job.checklist = Some(Default::default());
            } else {
                crate::workflow::launch_job(app);
            }
        }
    });
}

/// After a job stopped early: the line to resume from (where the head
/// stopped, editable), where the machine will go, and the button.
fn resume_card(app: &mut MyApp, ui: &mut egui::Ui, can_resume: bool) {
    let Some(interrupted) = &mut app.job.interrupted else {
        return;
    };
    let mut resume = false;
    let mut forget = false;
    egui::Frame::new()
        .fill(crate::theme::WARNING.gamma_multiply(0.15))
        .corner_radius(8.0)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.label(RichText::new(tr("resume.title")).strong());
            let total = interrupted.gcode.lines().count().max(1);
            ui.horizontal(|ui| {
                ui.label(tr("resume.line"));
                let mut shown = interrupted.line + 1;
                if ui
                    .add(DragValue::new(&mut shown).range(1..=total))
                    .changed()
                {
                    interrupted.line = shown - 1;
                }
                ui.label(RichText::new(format!("/ {total}")).weak());
            });
            if let Some(text) = interrupted.gcode.lines().nth(interrupted.line) {
                ui.label(RichText::new(text.trim()).monospace().weak());
            }
            if let Some(motion) = interrupted
                .program
                .motions
                .iter()
                .find(|motion| motion.line >= interrupted.line)
            {
                let [x, y, z] = motion.from;
                ui.label(trf(
                    "resume.from",
                    &[&format!("{x:.2}"), &format!("{y:.2}"), &format!("{z:.2}")],
                ));
            }
            ui.add(egui::Label::new(RichText::new(tr("resume.hint")).weak().small()).wrap());
            ui.horizontal(|ui| {
                resume = ui
                    .add_enabled(
                        can_resume,
                        egui::Button::new(RichText::new(tr("resume.go")).strong()),
                    )
                    .clicked();
                forget = ui.button(tr("resume.forget")).clicked();
            });
        });
    if forget {
        app.job.interrupted = None;
    } else if resume {
        app.job.resume_from = app
            .job
            .interrupted
            .as_ref()
            .map(|interrupted| interrupted.line);
        if app.settings.barriers {
            app.job.checklist = Some(Default::default());
        } else {
            crate::workflow::launch_job(app);
        }
    }
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

/// Proposes slower homing speeds when the controller searches fast:
/// an obstruction then takes a much smaller hit.
fn gentle_homing_advice(device: &mut DeviceState, ui: &mut egui::Ui) {
    use mml_device::homing;
    let lines = homing::gentle_settings(device.setting_f64(25), device.setting_f64(24));
    if lines.is_empty() {
        return;
    }
    let seek = device.setting_f64(25).unwrap_or(0.0);
    ui.label(
        RichText::new(trf("homing.fast", &[&format!("{seek:.0}")])).color(crate::theme::WARNING),
    );
    let idle = device.state() == MachineState::Idle && !device.job_running();
    if ui
        .add_enabled(idle, egui::Button::new(tr("homing.make_gentle")))
        .on_hover_text(tr("homing.make_gentle_hint"))
        .clicked()
    {
        for line in lines {
            device.line(line);
        }
        device.line("$$");
    }
}

/// What to do after a failed homing, or before one when the controller
/// has homing switched off.
fn homing_doctor(device: &mut DeviceState, ui: &mut egui::Ui) {
    use mml_device::homing::{self, Failure};
    let disabled = homing::enabled(device.setting_f64(22)) == Some(false);
    let failure = match device.homing_failure {
        Some(failure) => failure,
        // Machines without switches: only worth saying when locked.
        None if disabled && device.state() == MachineState::Alarm => Failure::Disabled,
        None => return,
    };
    let idle = !device.job_running() && device.homing.is_none();
    let (title, text) = match failure {
        Failure::Disabled => ("homing.doctor.disabled", "homing.doctor.disabled_text"),
        Failure::NotFound => ("homing.doctor.not_found", "homing.doctor.not_found_text"),
        Failure::PullOff => ("homing.doctor.pull_off", "homing.doctor.pull_off_text"),
        Failure::Aborted => ("homing.doctor.aborted", "homing.doctor.aborted_text"),
        Failure::TooLong => ("homing.doctor.too_long", "homing.doctor.too_long_text"),
        Failure::Other(_) => ("homing.doctor.other", "homing.doctor.other_text"),
    };
    egui::Frame::new()
        .fill(crate::theme::WARNING.gamma_multiply(0.15))
        .corner_radius(6.0)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(tr(title))
                    .strong()
                    .color(crate::theme::WARNING),
            );
            ui.label(tr(text));
            ui.horizontal_wrapped(|ui| {
                match failure {
                    Failure::Disabled => {
                        if ui
                            .add_enabled(idle, egui::Button::new(tr("device.unlock")))
                            .on_hover_text(tr("homing.doctor.unlock_hint"))
                            .clicked()
                        {
                            device.line("$X");
                            device.homing_failure = None;
                        }
                        if ui
                            .add_enabled(idle, egui::Button::new(tr("homing.doctor.enable")))
                            .on_hover_text(tr("homing.doctor.enable_hint"))
                            .clicked()
                        {
                            device.line("$22=1");
                            device.line("$$");
                            device.homing_failure = None;
                        }
                    }
                    Failure::NotFound => {
                        let travel: Vec<String> = [(130, 'X'), (131, 'Y'), (132, 'Z')]
                            .iter()
                            .filter_map(|(key, axis)| {
                                device
                                    .setting_f64(*key)
                                    .map(|value| format!("{axis} {value:.0} mm"))
                            })
                            .collect();
                        if !travel.is_empty() {
                            ui.label(
                                RichText::new(trf("homing.doctor.travel", &[&travel.join(" · ")]))
                                    .weak(),
                            );
                        }
                    }
                    Failure::PullOff => {
                        let pull_off = device.setting_f64(27).unwrap_or(1.0);
                        if pull_off < 3.0
                            && ui
                                .add_enabled(
                                    idle,
                                    egui::Button::new(trf(
                                        "homing.doctor.more_pull_off",
                                        &[&format!("{pull_off:.1}")],
                                    )),
                                )
                                .clicked()
                        {
                            device.line("$27=3");
                            device.line("$$");
                        }
                    }
                    _ => {}
                }
                if device.homing_failure.is_some()
                    && ui.small_button(tr("homing.doctor.dismiss")).clicked()
                {
                    device.homing_failure = None;
                }
            });
        });
}

/// Homing button. With hold to run, homing starts when it is pressed
/// and stops (controller reset) as soon as it is released.
pub fn homing_button(device: &mut DeviceState, ui: &mut egui::Ui, hold: bool, label: &str) {
    let active = device.homing.is_some();
    let text = if active && hold {
        tr("homing.keep_holding")
    } else {
        label
    };
    let mut button = egui::Button::new(text);
    if active {
        button = button.fill(crate::theme::WARNING.gamma_multiply(0.35));
    }
    let response = ui.add_enabled(!device.job_running(), button);
    let response = if hold {
        response.on_hover_text(tr("homing.hold_hint"))
    } else {
        response
    };
    if hold {
        let pressed = response.hovered() && ui.input(|input| input.pointer.primary_pressed());
        if pressed && !active {
            device.start_homing(true);
        }
        if response.is_pointer_button_down_on()
            && let Some(homing) = &mut device.homing
        {
            homing.held = true;
        }
    } else if response.clicked() {
        device.start_homing(false);
    }
}

pub fn format_short(duration: Duration) -> String {
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
                            ConsoleKind::Error => crate::theme::DANGER,
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
                    if line.trim() == "$H" {
                        app.device.start_homing(false);
                    } else {
                        app.device.line(line);
                    }
                    response.request_focus();
                }
            });
            ui.label(RichText::new(tr("macros.title")).strong());
            ui.horizontal_wrapped(|ui| {
                let macros = app.settings.macros.clone();
                for (index, entry) in macros.iter().enumerate() {
                    if entry.gcode.trim() == "$H" {
                        let hold = app.settings.homing_hold;
                        homing_button(&mut app.device, ui, hold, &entry.name);
                        continue;
                    }
                    let response = ui
                        .add_enabled(!running, egui::Button::new(&entry.name))
                        .on_hover_text(&entry.gcode);
                    if response.clicked() {
                        for line in entry.gcode.lines() {
                            if line.trim() == "$H" {
                                app.device.start_homing(false);
                            } else {
                                app.device.line(line.to_owned());
                            }
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
