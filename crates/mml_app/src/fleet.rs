//! Production with several machines: a queue of jobs (copies of the
//! current job, G-code files such as the sheets of a puzzle) shared by
//! machines connected side by side. Each machine has its own controller
//! and live safety monitor; a job starts on a machine only once the
//! operator confirmed that machine is loaded, and that confirmation is
//! asked again for every new piece.

use crate::app::{Level, MyApp};
use crate::i18n::{tr, trf};
use eframe::egui::{self, Color32, DragValue, RichText};
use mml_device::{
    Command, Connection, Controller, Event, JobGuard, JobOutcome, MachineState, MachineStatus,
    Protocol,
};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub struct QueueItem {
    pub name: String,
    pub gcode: Arc<String>,
    pub remaining: u32,
    pub done: u32,
}

/// Jobs waiting for a machine, in order.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ProductionQueue {
    pub items: Vec<QueueItem>,
}

impl ProductionQueue {
    pub fn add(&mut self, name: String, gcode: Arc<String>, copies: u32) {
        if copies > 0 {
            self.items.push(QueueItem {
                name,
                gcode,
                remaining: copies,
                done: 0,
            });
        }
    }

    /// Takes the next job for a machine: index of the item and its
    /// program.
    pub fn take(&mut self) -> Option<(usize, String, Arc<String>)> {
        let (index, item) = self
            .items
            .iter_mut()
            .enumerate()
            .find(|(_, item)| item.remaining > 0)?;
        item.remaining -= 1;
        Some((index, item.name.clone(), item.gcode.clone()))
    }

    /// A job came back unfinished: it goes back in the queue.
    pub fn give_back(&mut self, index: usize) {
        if let Some(item) = self.items.get_mut(index) {
            item.remaining += 1;
        }
    }

    pub fn finished(&mut self, index: usize) {
        if let Some(item) = self.items.get_mut(index) {
            item.done += 1;
        }
    }

    pub fn waiting(&self) -> u32 {
        self.items.iter().map(|item| item.remaining).sum()
    }
}

pub struct FleetMachine {
    pub name: String,
    pub use_tcp: bool,
    pub host: String,
    pub tcp_port: u16,
    pub serial_port: String,
    pub baud: u32,
    pub protocol: Protocol,
    controller: Option<Controller>,
    connected: bool,
    status: Option<MachineStatus>,
    /// Operator confirmed: material loaded, origin set, area clear.
    pub ready: bool,
    /// Queue item running and its progress.
    job: Option<(usize, String, usize, usize)>,
    message: Option<(String, bool)>,
}

impl FleetMachine {
    fn new(index: usize) -> Self {
        Self {
            name: format!("{} {}", tr("fleet.machine"), index),
            use_tcp: true,
            host: String::new(),
            tcp_port: 23,
            serial_port: String::new(),
            baud: 115_200,
            protocol: Protocol::Grbl,
            controller: None,
            connected: false,
            status: None,
            ready: false,
            job: None,
            message: None,
        }
    }

    fn state(&self) -> MachineState {
        self.status
            .as_ref()
            .map_or(MachineState::Unknown, |status| status.state)
    }

    fn send(&self, command: Command) {
        if let Some(controller) = &self.controller {
            controller.send(command);
        }
    }
}

#[derive(Default)]
pub struct FleetState {
    pub open: bool,
    pub machines: Vec<FleetMachine>,
    pub queue: ProductionQueue,
    pub copies: u32,
}

fn guard(gcode: &str, auto_pause: bool) -> JobGuard {
    let program = mml_gcode::parse(gcode);
    let bounds = program.bounds();
    JobGuard {
        envelope: (!bounds.is_empty()).then_some([
            [bounds.min.x, bounds.max.x],
            [bounds.min.y, bounds.max.y],
            [program.z_range.0, program.z_range.1],
        ]),
        uses_rotary: program.uses_rotary,
        tolerance: 1.0,
        auto_pause,
    }
}

/// Reads the events of every machine and hands out jobs.
pub fn poll(app: &mut MyApp) {
    let auto_pause = app.settings.auto_pause;
    // Hours worked by each machine, to share the wear fairly.
    let worked = |name: &str, records: &[mml_core::stats::JobRecord]| -> f64 {
        records
            .iter()
            .filter(|record| record.machine == name)
            .map(|record| record.seconds)
            .sum()
    };
    let hours: Vec<f64> = app
        .fleet
        .machines
        .iter()
        .map(|machine| worked(&machine.name, &app.usage))
        .collect();
    let fleet = &mut app.fleet;
    let mut notices = Vec::new();
    let mut notices_done: Vec<String> = Vec::new();
    let mut finished_jobs: Vec<(String, String, Arc<String>, bool, std::time::Duration)> =
        Vec::new();
    for machine in &mut fleet.machines {
        let Some(controller) = &machine.controller else {
            continue;
        };
        let events: Vec<Event> = controller.events().collect();
        for event in events {
            match event {
                Event::Connected { .. } => machine.connected = true,
                Event::Disconnected { reason } => {
                    machine.connected = false;
                    machine.controller = None;
                    if let Some((index, ..)) = machine.job.take() {
                        fleet.queue.give_back(index);
                    }
                    machine.message = reason.map(|reason| (reason, true));
                    break;
                }
                Event::Status(status) => machine.status = Some(status),
                Event::JobProgress {
                    acknowledged,
                    total,
                } => {
                    if let Some(job) = &mut machine.job {
                        job.2 = acknowledged;
                        job.3 = total;
                    }
                }
                Event::JobFinished { outcome, elapsed } => {
                    if let Some((index, name, ..)) = machine.job.take() {
                        if let Some(item) = fleet.queue.items.get(index) {
                            finished_jobs.push((
                                machine.name.clone(),
                                name.clone(),
                                item.gcode.clone(),
                                outcome == JobOutcome::Completed,
                                elapsed,
                            ));
                        }
                        notices_done.push(format!("{}: {name}", machine.name));
                        if outcome == JobOutcome::Completed {
                            fleet.queue.finished(index);
                            machine.message = Some((trf("fleet.done", &[&name]), false));
                        } else {
                            fleet.queue.give_back(index);
                            machine.message = Some((trf("fleet.unfinished", &[&name]), true));
                        }
                    }
                    // A new piece must be loaded before the next job.
                    machine.ready = false;
                }
                Event::Anomaly { anomaly, paused } => {
                    let text = format!("{}: {anomaly:?}", machine.name);
                    machine.message = Some((text.clone(), true));
                    if paused {
                        notices.push(text);
                    }
                }
                Event::Alarm(code) => machine.message = Some((format!("ALARM:{code}"), true)),
                _ => {}
            }
        }
    }
    // Fair share: of the machines ready now, the least used starts first.
    let mut ready: Vec<usize> = (0..fleet.machines.len())
        .filter(|&index| {
            let machine = &fleet.machines[index];
            machine.connected
                && machine.ready
                && machine.job.is_none()
                && machine.state() == MachineState::Idle
        })
        .collect();
    ready.sort_by(|&a, &b| hours[a].total_cmp(&hours[b]));
    for index in ready {
        let Some((item, name, gcode)) = fleet.queue.take() else {
            break;
        };
        let machine = &mut fleet.machines[index];
        let lines: Vec<String> = gcode.lines().map(str::to_owned).collect();
        let total = lines.len();
        machine.send(Command::StartJob {
            lines,
            guard: Some(guard(&gcode, auto_pause)),
        });
        machine.job = Some((item, name, 0, total));
        machine.message = None;
    }
    for (machine, job, gcode, completed, elapsed) in finished_jobs {
        app.record_job(machine, job, &gcode, completed, elapsed);
    }
    if let Some(last) = notices_done.pop() {
        app.attention = Some((tr("os.job_done").to_owned(), last));
    }
    for notice in notices {
        app.notify(notice, Level::Error);
    }
}

pub fn show(app: &mut MyApp, ctx: &egui::Context) {
    if !app.fleet.open {
        return;
    }
    let mut open = true;
    let mut add_current = false;
    let mut add_files = false;
    let current = app.job.built.as_ref().map(|built| built.gcode.clone());
    egui::Window::new(tr("fleet.title"))
        .open(&mut open)
        .default_width(620.0)
        .show(ctx, |ui| {
            let fleet = &mut app.fleet;
            ui.label(RichText::new(tr("fleet.intro")).weak());
            let estop = egui::Button::new(
                RichText::new(tr("fleet.estop"))
                    .strong()
                    .color(Color32::WHITE),
            )
            .fill(crate::theme::DANGER);
            if ui.add(estop).clicked() {
                for machine in &fleet.machines {
                    machine.send(Command::EmergencyStop);
                }
            }
            ui.separator();
            ui.label(RichText::new(tr("fleet.queue")).strong());
            ui.horizontal(|ui| {
                ui.add(
                    DragValue::new(&mut fleet.copies)
                        .range(1..=10_000)
                        .prefix("x "),
                );
                add_current = ui
                    .add_enabled(
                        current.is_some(),
                        egui::Button::new(tr("fleet.add_current")),
                    )
                    .clicked();
                add_files = ui.button(tr("fleet.add_files")).clicked();
                // Running jobs keep their place: only what waits is dropped.
                if ui.button(tr("fleet.clear")).clicked() {
                    for item in &mut fleet.queue.items {
                        item.remaining = 0;
                    }
                }
            });
            for item in &fleet.queue.items {
                ui.label(trf(
                    "fleet.item",
                    &[&item.name, &item.done, &item.remaining],
                ));
            }
            ui.label(RichText::new(trf("fleet.waiting", &[&fleet.queue.waiting()])).weak());
            ui.separator();
            let mut remove = None;
            for (index, machine) in fleet.machines.iter_mut().enumerate() {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut machine.name).desired_width(110.0));
                        let state = if machine.connected {
                            machine.state().label()
                        } else {
                            tr("fleet.offline")
                        };
                        ui.label(RichText::new(state).strong());
                        if machine.controller.is_none() {
                            ui.selectable_value(&mut machine.use_tcp, false, "USB");
                            ui.selectable_value(&mut machine.use_tcp, true, "Wi-Fi");
                            if machine.use_tcp {
                                ui.add(
                                    egui::TextEdit::singleline(&mut machine.host)
                                        .hint_text("192.168.1.50")
                                        .desired_width(110.0),
                                );
                                ui.add(DragValue::new(&mut machine.tcp_port).range(1..=65535));
                            } else {
                                ui.add(
                                    egui::TextEdit::singleline(&mut machine.serial_port)
                                        .hint_text("/dev/cu.usbserial")
                                        .desired_width(140.0),
                                );
                            }
                            if ui.button(tr("device.connect")).clicked() {
                                let connection = if machine.use_tcp {
                                    Connection::Tcp {
                                        host: machine.host.trim().to_owned(),
                                        port: machine.tcp_port,
                                    }
                                } else {
                                    Connection::Serial {
                                        port: machine.serial_port.clone(),
                                        baud: machine.baud,
                                    }
                                };
                                machine.controller =
                                    Some(Controller::connect_with(connection, machine.protocol));
                            }
                            if ui.small_button(tr("fleet.remove")).clicked() {
                                remove = Some(index);
                            }
                        } else if ui.button(tr("device.disconnect")).clicked()
                            && machine.job.is_none()
                        {
                            machine.send(Command::Disconnect);
                        }
                    });
                    if machine.connected {
                        ui.horizontal(|ui| {
                            ui.add_enabled_ui(machine.job.is_none(), |ui| {
                                ui.checkbox(&mut machine.ready, tr("fleet.ready"))
                                    .on_hover_text(tr("fleet.ready_hint"));
                            });
                            if let Some((_, name, done, total)) = &machine.job {
                                let share = if *total > 0 {
                                    *done as f32 / *total as f32
                                } else {
                                    0.0
                                };
                                ui.add(
                                    egui::ProgressBar::new(share)
                                        .text(name.clone())
                                        .desired_width(180.0),
                                );
                                if ui.small_button(tr("fleet.pause")).clicked() {
                                    machine.send(Command::Pause);
                                }
                                if ui.small_button(tr("fleet.resume")).clicked() {
                                    machine.send(Command::Resume);
                                }
                                if ui.small_button(tr("fleet.stop")).clicked() {
                                    machine.send(Command::Stop);
                                }
                            }
                        });
                    }
                    if let Some((text, problem)) = &machine.message {
                        let color = if *problem {
                            crate::theme::WARNING
                        } else {
                            crate::theme::SUCCESS
                        };
                        ui.label(RichText::new(text).color(color));
                    }
                });
            }
            if let Some(index) = remove {
                fleet.machines.remove(index);
            }
            if ui.button(tr("fleet.add_machine")).clicked() {
                let next = fleet.machines.len() + 1;
                fleet.machines.push(FleetMachine::new(next));
            }
        });
    if add_current && let Some(gcode) = current {
        let name = app
            .job
            .built
            .as_ref()
            .and_then(|built| built.external.clone())
            .unwrap_or_else(|| app.document_name());
        let copies = app.fleet.copies.max(1);
        app.fleet.queue.add(name, gcode, copies);
    }
    if add_files
        && let Some(paths) = rfd::FileDialog::new()
            .add_filter("G-code", &["nc", "gcode", "gc", "ngc", "tap"])
            .pick_files()
    {
        for path in paths {
            match std::fs::read_to_string(&path) {
                Ok(text) => {
                    let name = path
                        .file_name()
                        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
                    app.fleet
                        .queue
                        .add(name, Arc::new(text), app.fleet.copies.max(1));
                }
                Err(error) => app.notify(
                    trf("file.open_failed", &[&path.display(), &error]),
                    Level::Error,
                ),
            }
        }
    }
    app.fleet.open = open;
}

/// A job streams on one of the production machines.
pub fn running(app: &MyApp) -> bool {
    app.fleet
        .machines
        .iter()
        .any(|machine| machine.job.is_some())
}

/// Keeps machines served while the window is closed.
pub fn busy(app: &MyApp) -> bool {
    app.fleet
        .machines
        .iter()
        .any(|machine| machine.controller.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_queue_hands_out_every_copy_once_and_takes_back_failures() {
        let mut queue = ProductionQueue::default();
        queue.add("sheet A".into(), Arc::new("G0 X1".into()), 2);
        queue.add("sheet B".into(), Arc::new("G0 X2".into()), 1);
        assert_eq!(queue.waiting(), 3);
        let (first, name, _) = queue.take().unwrap();
        assert_eq!(name, "sheet A");
        let (second, _, _) = queue.take().unwrap();
        let (third, name, _) = queue.take().unwrap();
        assert_eq!(name, "sheet B");
        assert!(queue.take().is_none());
        queue.finished(first);
        // The second copy failed on its machine: it comes back.
        queue.give_back(second);
        assert_eq!(queue.waiting(), 1);
        let (again, name, _) = queue.take().unwrap();
        assert_eq!((again, name.as_str()), (0, "sheet A"));
        queue.finished(again);
        queue.finished(third);
        assert_eq!(queue.items[0].done, 2);
        assert_eq!(queue.items[1].done, 1);
        assert_eq!(queue.waiting(), 0);
    }
}
