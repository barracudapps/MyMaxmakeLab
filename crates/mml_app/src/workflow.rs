//! Operator workflow around a job: mandatory checks (barriers), dry run
//! approval, probing, basic machine functions, controller settings and
//! the first-launch tutorial.

use crate::app::{Level, Mode, MyApp, Placement};
use crate::i18n::{tr, trf};
use crate::machine_panel::{JobKind, RunningJob, guard_for};
use eframe::egui::{self, Color32, DragValue, RichText};
use mml_core::document::{Shape, ShapeKind};
use mml_core::geometry::{Path, Point, Rect, Transform};
use mml_core::machine::MachineKind;
use mml_device::{Command, JobOutcome, MachineState};
use mml_probe::{GridSpec, HeightMap};
use std::time::{Duration, Instant};

/// Safety checklist shown before every job while barriers are on.
#[derive(Debug, Default, Clone)]
pub struct Checklist {
    pub ticks: [bool; 4],
}

pub struct ProbeState {
    pub map: Option<HeightMap>,
    /// Flat mode: the top face is a plane (possibly tilted) measured on
    /// a few points; otherwise a grid of adjustable size.
    pub flat: bool,
    pub plane_points: usize,
    pub plane: Option<(mml_probe::Plane, f64)>,
    pub pending_plane: Option<(Rect, Vec<Point>, [f64; 3])>,
    pub pending: Option<(GridSpec, [f64; 3])>,
    pub columns: usize,
    pub rows: usize,
    pub follow: bool,
    pub slope_power: bool,
    pub depth: f64,
}

impl Default for ProbeState {
    fn default() -> Self {
        Self {
            map: None,
            flat: true,
            plane_points: 4,
            plane: None,
            pending_plane: None,
            pending: None,
            columns: 6,
            rows: 6,
            follow: false,
            slope_power: true,
            depth: 15.0,
        }
    }
}

fn job_uses_cnc(app: &MyApp) -> bool {
    match &app.job.built {
        Some(built) if built.external.is_some() => {
            app.machine.kind == MachineKind::Cnc && !app.machine.laser_mode
        }
        _ => app
            .document
            .layers
            .iter()
            .any(|layer| layer.output && layer.visible && !layer.operation.is_laser()),
    }
}

fn job_uses_laser(app: &MyApp) -> bool {
    match &app.job.built {
        Some(built) if built.external.is_some() => {
            app.machine.kind == MachineKind::Laser || app.machine.laser_mode
        }
        _ => app
            .document
            .layers
            .iter()
            .any(|layer| layer.output && layer.visible && layer.operation.is_laser()),
    }
}

/// Conditions the operator must satisfy before Start, unless barriers
/// are switched off in advanced mode.
pub fn start_gates(app: &MyApp) -> Vec<String> {
    let mut gates = Vec::new();
    if app
        .device
        .status
        .as_ref()
        .is_some_and(|status| status.pins.contains('D'))
    {
        gates.push(tr("gate.door_open").to_owned());
    }
    if !app.settings.barriers {
        return gates;
    }
    if job_uses_cnc(app) && app.settings.require_probe_cnc && !app.device.z_probed {
        gates.push(tr("gate.probe_first").to_owned());
    }
    if job_uses_cnc(app) && app.machine.kind == MachineKind::Cnc {
        gates.extend(crate::collision::gates(app));
    }
    if job_uses_laser(app)
        && app.settings.require_frame_laser
        && app.job.frame_approved != Some(app.job_key())
    {
        gates.push(tr("gate.frame_first").to_owned());
    }
    gates
}

/// Starts frame, probing or other helper programs.
pub fn run_auxiliary(app: &mut MyApp, lines: Vec<String>, kind: JobKind) {
    let total = lines.len();
    app.device.send(Command::StartJob { lines, guard: None });
    app.device.job = Some(RunningJob {
        total,
        acknowledged: 0,
        started: Instant::now(),
        estimate: Duration::ZERO,
        paused: false,
        kind,
    });
}

pub fn launch_job(app: &mut MyApp) {
    let Some(built) = &app.job.built else { return };
    let mut gcode = built.gcode.to_string();
    let follow = app.probe.follow && built.anchor.is_none();
    if follow && let Some(map) = &app.probe.map {
        let options = mml_probe::FollowOptions {
            max_segment: 1.0,
            slope_power: app.probe.slope_power,
            max_power_gain: 1.5,
            max_s: app.machine.max_s,
        };
        gcode = mml_probe::follow_surface(&gcode, &built.program, map, options);
    }
    let estimate = built.stats.duration;
    let guard = guard_for(app).map(|mut guard| {
        if follow && let (Some(envelope), Some(map)) = (&mut guard.envelope, &app.probe.map) {
            let heights = map.heights.iter().flatten();
            let high = heights.clone().copied().fold(0.0f64, f64::max);
            let low = heights.copied().fold(0.0f64, f64::min);
            envelope[2] = [envelope[2][0] + low, envelope[2][1] + high];
        }
        guard
    });
    let lines: Vec<String> = gcode.lines().map(str::to_owned).collect();
    let total = lines.len();
    app.device.send(Command::StartJob { lines, guard });
    app.device.job = Some(RunningJob {
        total,
        acknowledged: 0,
        started: Instant::now(),
        estimate,
        paused: false,
        kind: JobKind::Main,
    });
    app.device.alarm = None;
    app.device.anomaly = None;
    app.job.waiting_pause = None;
    app.notify(tr("job.started"), Level::Info);
}

/// Handles the end of helper programs (frame review, probe results).
pub fn process_finished(app: &mut MyApp) {
    let Some((kind, outcome)) = app.device.finished.take() else {
        return;
    };
    let completed = outcome == JobOutcome::Completed;
    if matches!(kind, JobKind::ToolSetter | JobKind::SetterReference) {
        crate::collision::setter_finished(app, kind, completed);
        return;
    }
    if kind == JobKind::Main
        && let Some(built) = &app.job.built
    {
        let gcode = built.gcode.clone();
        let name = built
            .external
            .clone()
            .unwrap_or_else(|| app.document_name());
        let machine = app.machine.name.clone();
        let elapsed = app.device.last_elapsed;
        app.record_job(machine, name, &gcode, completed, elapsed);
        let title = if completed {
            tr("os.job_done")
        } else {
            tr("os.job_stopped")
        };
        app.attention = Some((title.to_owned(), app.document_name()));
    }
    match kind {
        JobKind::Frame if completed => app.job.frame_review = true,
        JobKind::ProbeOrigin => {
            let touched = app.device.probe_reports.iter().any(|(_, touched)| *touched);
            app.device.probe_reports.clear();
            if completed && touched {
                app.device.z_probed = true;
                app.notify(tr("probe.origin_done"), Level::Info);
            } else {
                app.notify(tr("probe.origin_failed"), Level::Error);
            }
        }
        JobKind::ProbePlane => {
            let reports = std::mem::take(&mut app.device.probe_reports);
            if let Some((area, points, offset)) = app.probe.pending_plane.take() {
                let measured: Vec<[f64; 3]> = points
                    .iter()
                    .zip(reports.iter().filter(|(_, touched)| *touched))
                    .map(|(point, (position, _))| [point.x, point.y, position[2] - offset[2]])
                    .collect();
                match mml_probe::Plane::fit(&measured) {
                    Some(plane) if completed => {
                        let residual = plane.residual(&measured);
                        app.probe.map = Some(plane.to_map(area));
                        app.probe.plane = Some((plane, residual));
                        let text = trf(
                            "probe.plane_done",
                            &[
                                &format!("{:.2}", plane.tilt_degrees()),
                                &format!("{residual:.2}"),
                            ],
                        );
                        app.notify(text, Level::Info);
                    }
                    _ => app.notify(tr("probe.origin_failed"), Level::Error),
                }
            }
        }
        JobKind::ProbeGrid => {
            let reports = std::mem::take(&mut app.device.probe_reports);
            if let Some((spec, offset)) = app.probe.pending.take() {
                let mut map = HeightMap::new(spec.area, spec.columns, spec.rows);
                for (point, (position, touched)) in spec.points().into_iter().zip(reports) {
                    map.record(point, touched.then_some(position[2] - offset[2]));
                }
                let count = map.touched();
                app.notify(
                    trf(
                        "probe.map_done",
                        &[&count, &(spec.columns.max(2) * spec.rows.max(2))],
                    ),
                    Level::Info,
                );
                app.probe.map = Some(map);
            }
        }
        _ => {}
    }
}

pub fn frame_review(app: &mut MyApp, ui: &mut egui::Ui) {
    if !app.job.frame_review {
        return;
    }
    egui::Frame::new()
        .fill(Color32::from_rgb(40, 120, 220).gamma_multiply(0.18))
        .corner_radius(8.0)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(tr("frame.question")).strong());
            ui.horizontal_wrapped(|ui| {
                if ui.button(tr("frame.approve")).clicked() {
                    app.job.frame_approved = app.job.frame_for;
                    app.job.frame_review = false;
                }
                if ui
                    .button(tr("frame.adjust"))
                    .on_hover_text(tr("frame.adjust_hint"))
                    .clicked()
                {
                    app.job.frame_review = false;
                }
                if ui.button(tr("frame.rerun")).clicked() {
                    app.job.frame_review = false;
                    if let Some(built) = &app.job.built {
                        let program = mml_job::frame_job(
                            built.stats.cut_bounds,
                            built.anchor,
                            &app.machine,
                            app.settings.frame_power,
                            3000.0,
                        );
                        run_auxiliary(
                            app,
                            program.lines().map(str::to_owned).collect(),
                            JobKind::Frame,
                        );
                        app.job.frame_for = Some(app.job_key());
                    }
                }
            });
        });
}

pub fn checklist_modal(app: &mut MyApp, ctx: &egui::Context) {
    if app.job.checklist.is_none() {
        return;
    }
    let laser = job_uses_laser(app);
    let Some(checklist) = &mut app.job.checklist else {
        return;
    };
    let keys: [&'static str; 4] = if laser {
        [
            "check.laser_lid",
            "check.laser_eyes",
            "check.laser_material",
            "check.laser_fire",
        ]
    } else {
        [
            "check.cnc_tool",
            "check.cnc_clamp",
            "check.cnc_eyes",
            "check.cnc_clear",
        ]
    };
    let mut start = false;
    let mut cancel = false;
    egui::Modal::new(egui::Id::unique("checklist")).show(ctx, |ui| {
        ui.label(RichText::new(tr("check.title")).strong().size(16.0));
        for (tick, key) in checklist.ticks.iter_mut().zip(keys) {
            ui.checkbox(tick, tr(key));
        }
        ui.horizontal(|ui| {
            let ready = checklist.ticks.iter().all(|tick| *tick);
            start = ui
                .add_enabled(
                    ready,
                    egui::Button::new(RichText::new(tr("job.start")).strong()),
                )
                .clicked();
            cancel = ui.button(tr("dialog.cancel")).clicked();
        });
    });
    if start {
        app.job.checklist = None;
        launch_job(app);
    } else if cancel {
        app.job.checklist = None;
    }
}

pub fn probe_section(app: &mut MyApp, ui: &mut egui::Ui) {
    egui::CollapsingHeader::new(RichText::new(tr("probe.title")).strong())
        .default_open(app.machine.kind == MachineKind::Cnc)
        .show(ui, |ui| {
            let idle = app.device.state() == MachineState::Idle && !app.device.job_running();
            ui.checkbox(&mut app.settings.has_probe, tr("probe.has_probe"))
                .on_hover_text(tr("probe.has_probe_hint"));
            if !app.settings.has_probe {
                ui.label(RichText::new(tr("probe.without_probe")).weak());
            }
            if app.settings.has_probe {
                egui::Grid::new("probe-settings")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label(tr("probe.plate"));
                        ui.add(
                            DragValue::new(&mut app.settings.probe.plate_thickness)
                                .range(0.0..=50.0)
                                .suffix(" mm")
                                .speed(0.05),
                        );
                        ui.end_row();
                        ui.label(tr("probe.tool"));
                        ui.add(
                            DragValue::new(&mut app.settings.probe.tool_diameter)
                                .range(0.1..=20.0)
                                .suffix(" mm")
                                .speed(0.01),
                        );
                        ui.end_row();
                    });
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(idle, egui::Button::new(tr("probe.z")))
                        .on_hover_text(tr("probe.z_hint"))
                        .clicked()
                    {
                        app.device.probe_reports.clear();
                        run_auxiliary(
                            app,
                            mml_probe::z_origin_program(&app.settings.probe),
                            JobKind::ProbeOrigin,
                        );
                    }
                    if ui
                        .add_enabled(idle, egui::Button::new(tr("probe.corner")))
                        .on_hover_text(tr("probe.corner_hint"))
                        .clicked()
                    {
                        app.device.probe_reports.clear();
                        run_auxiliary(
                            app,
                            mml_probe::corner_origin_program(&app.settings.probe),
                            JobKind::ProbeOrigin,
                        );
                    }
                });
                if app.device.z_probed {
                    ui.label(RichText::new(tr("probe.origin_ok")).color(crate::theme::SUCCESS));
                }
                ui.add_space(4.0);
                ui.label(RichText::new(tr("probe.map_title")).strong());
                ui.horizontal(|ui| {
                    ui.radio_value(&mut app.probe.flat, true, tr("probe.flat"))
                        .on_hover_text(tr("probe.flat_hint"));
                    ui.radio_value(&mut app.probe.flat, false, tr("probe.uneven"))
                        .on_hover_text(tr("probe.uneven_hint"));
                });
                if app.probe.flat {
                    ui.horizontal(|ui| {
                        ui.label(tr("probe.points"));
                        for count in [3, 4, 5, 9] {
                            ui.selectable_value(
                                &mut app.probe.plane_points,
                                count,
                                count.to_string(),
                            );
                        }
                    });
                    let area = app
                        .job
                        .built
                        .as_ref()
                        .map(|built| expand(built.stats.cut_bounds, 2.0))
                        .filter(|area| area.width() > 4.0);
                    let measure = egui::Button::new(tr("probe.measure_plane"));
                    if ui.add_enabled(idle && area.is_some(), measure).clicked()
                        && let Some(area) = area
                    {
                        start_plane(app, area);
                    }
                    if let Some((plane, residual)) = app.probe.plane {
                        ui.label(trf(
                            "probe.plane_summary",
                            &[
                                &format!("{:.2}", plane.tilt_degrees()),
                                &format!("{residual:.2}"),
                            ],
                        ));
                        if residual > 0.2 {
                            ui.label(
                                RichText::new(tr("probe.not_flat")).color(crate::theme::WARNING),
                            );
                        }
                    }
                }
                ui.horizontal(|ui| {
                    ui.label(tr("probe.grid"));
                    ui.add(DragValue::new(&mut app.probe.columns).range(2..=40));
                    ui.label("×");
                    ui.add(DragValue::new(&mut app.probe.rows).range(2..=40));
                    ui.label(tr("probe.depth"));
                    ui.add(
                        DragValue::new(&mut app.probe.depth)
                            .range(1.0..=60.0)
                            .suffix(" mm"),
                    );
                });
                let area = app
                    .job
                    .built
                    .as_ref()
                    .map(|built| built.stats.cut_bounds)
                    .filter(|area| !area.is_empty());
                ui.horizontal_wrapped(|ui| {
                    let job_area = area.map(|area| expand(area, 2.0));
                    if ui
                        .add_enabled(
                            idle && job_area.is_some() && !app.probe.flat,
                            egui::Button::new(tr("probe.map_job")),
                        )
                        .on_hover_text(tr("probe.map_job_hint"))
                        .clicked()
                        && let Some(area) = job_area
                    {
                        start_grid(app, area);
                    }
                    let bed = Rect::from_points(
                        Point::new(5.0, 5.0),
                        Point::new(app.machine.width - 5.0, app.machine.height - 5.0),
                    );
                    if ui
                        .add_enabled(idle, egui::Button::new(tr("probe.detect")))
                        .on_hover_text(tr("probe.detect_hint"))
                        .clicked()
                    {
                        start_grid(app, bed);
                    }
                });
            }
            if let Some(map) = &app.probe.map {
                let heights: Vec<f64> = map.heights.iter().flatten().copied().collect();
                let (low, high) = heights
                    .iter()
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), z| {
                        (low.min(*z), high.max(*z))
                    });
                ui.label(trf(
                    "probe.map_summary",
                    &[&map.touched(), &format!("{:.2}", high - low)],
                ));
                let outline = map.footprint();
                ui.horizontal_wrapped(|ui| {
                    if outline.len() >= 3
                        && ui
                            .button(tr("probe.use_outline"))
                            .on_hover_text(tr("probe.use_outline_hint"))
                            .clicked()
                    {
                        add_outline(app, outline);
                    }
                    if ui
                        .button(tr("probe.export_stl"))
                        .on_hover_text(tr("probe.export_stl_hint"))
                        .clicked()
                    {
                        export_stl(app);
                    }
                    if ui.button(tr("probe.clear")).clicked() {
                        app.probe.map = None;
                        app.probe.plane = None;
                    }
                });
                let anchored = matches!(app.job.placement, Placement::Head(_));
                ui.add_enabled_ui(!anchored, |ui| {
                    ui.checkbox(&mut app.probe.follow, tr("probe.follow"))
                        .on_hover_text(tr("probe.follow_hint"));
                    ui.checkbox(&mut app.probe.slope_power, tr("probe.slope_power"))
                        .on_hover_text(tr("probe.slope_power_hint"));
                });
                if anchored {
                    ui.label(
                        RichText::new(tr("probe.follow_needs_origin"))
                            .weak()
                            .small(),
                    );
                }
            }
        });
}

fn expand(area: Rect, margin: f64) -> Rect {
    Rect::from_points(
        Point::new(area.min.x - margin, area.min.y - margin),
        Point::new(area.max.x + margin, area.max.y + margin),
    )
}

fn export_stl(app: &mut MyApp) {
    let Some(map) = &app.probe.map else { return };
    let data = map.to_stl();
    let dialog = rfd::FileDialog::new()
        .add_filter("STL", &["stl"])
        .set_file_name("surface.stl");
    if let Some(path) = dialog.save_file() {
        match std::fs::write(&path, data) {
            Ok(()) => app.notify(trf("file.exported", &[&path.display()]), Level::Info),
            Err(error) => app.notify(trf("file.save_failed", &[&error]), Level::Error),
        }
    }
}

fn start_plane(app: &mut MyApp, area: Rect) {
    let corners = area.corners();
    let center = area.center();
    let points: Vec<Point> = match app.probe.plane_points {
        3 => vec![corners[0], corners[1], Point::new(center.x, area.max.y)],
        4 => corners.to_vec(),
        5 => corners
            .iter()
            .copied()
            .chain(std::iter::once(center))
            .collect(),
        _ => (0..3)
            .flat_map(|row| (0..3).map(move |column| (column, row)))
            .map(|(column, row)| {
                Point::new(
                    area.min.x + area.width() * column as f64 / 2.0,
                    area.min.y + area.height() * row as f64 / 2.0,
                )
            })
            .collect(),
    };
    let offset = app
        .device
        .status
        .as_ref()
        .map_or([0.0; 3], |status| status.work_offset);
    app.device.probe_reports.clear();
    let lines =
        mml_probe::points_program(&points, 5.0, -app.probe.depth, app.settings.probe.seek_feed);
    app.probe.pending_plane = Some((area, points, offset));
    run_auxiliary(app, lines, JobKind::ProbePlane);
}

fn start_grid(app: &mut MyApp, area: Rect) {
    let spec = GridSpec {
        area,
        columns: app.probe.columns,
        rows: app.probe.rows,
        safe_z: 5.0,
        min_z: -app.probe.depth,
        feed: app.settings.probe.seek_feed,
    };
    let offset = app
        .device
        .status
        .as_ref()
        .map_or([0.0; 3], |status| status.work_offset);
    app.device.probe_reports.clear();
    let lines = mml_probe::grid_program(&spec);
    app.probe.pending = Some((spec, offset));
    run_auxiliary(app, lines, JobKind::ProbeGrid);
}

fn add_outline(app: &mut MyApp, outline: Vec<Point>) {
    let path = Path::new(outline, true);
    let center = path.bounds().center();
    let local = Path::new(
        path.points.iter().map(|point| *point - center).collect(),
        true,
    );
    let layer = app.layer_for(false);
    let mut shape = Shape::new(
        tr("probe.piece"),
        layer,
        ShapeKind::Paths { paths: vec![local] },
    );
    shape.transform = Transform::translate(center.x, center.y);
    app.add_shape(shape);
    app.mode = Mode::Design;
}

/// Everyday controls: spindle idle run and proportional jog speed.
pub fn basics_section(app: &mut MyApp, ui: &mut egui::Ui) {
    egui::CollapsingHeader::new(RichText::new(tr("basics.title")).strong())
        .default_open(false)
        .show(ui, |ui| {
            let idle = app.device.state() == MachineState::Idle && !app.device.job_running();
            ui.checkbox(
                &mut app.settings.jog_proportional,
                tr("basics.proportional"),
            )
            .on_hover_text(tr("basics.proportional_hint"));
            if app.machine.kind == MachineKind::Cnc {
                ui.horizontal(|ui| {
                    ui.label(tr("basics.spindle"));
                    ui.add(
                        DragValue::new(&mut app.settings.idle_rpm)
                            .range(0.0..=60_000.0)
                            .suffix(" rpm")
                            .speed(100.0),
                    );
                    if ui
                        .add_enabled(idle, egui::Button::new(tr("basics.spindle_on")))
                        .on_hover_text(tr("basics.spindle_hint"))
                        .clicked()
                    {
                        app.device.line(format!("M3 S{:.0}", app.settings.idle_rpm));
                    }
                    if ui.button(tr("basics.spindle_off")).clicked() {
                        app.device.line("M5");
                    }
                });
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(idle, egui::Button::new(tr("basics.air_on")))
                    .clicked()
                {
                    app.device.line("M8");
                }
                if ui.button(tr("basics.air_off")).clicked() {
                    app.device.line("M9");
                }
            });
        });
}

/// Common GRBL settings with their meaning; written one by one.
const SETTINGS: [(u32, &str); 24] = [
    (0, "setting.0"),
    (1, "setting.1"),
    (3, "setting.3"),
    (5, "setting.5"),
    (6, "setting.6"),
    (20, "setting.20"),
    (21, "setting.21"),
    (22, "setting.22"),
    (23, "setting.23"),
    (24, "setting.24"),
    (25, "setting.25"),
    (27, "setting.27"),
    (30, "setting.30"),
    (31, "setting.31"),
    (32, "setting.32"),
    (100, "setting.100"),
    (101, "setting.101"),
    (102, "setting.102"),
    (110, "setting.110"),
    (111, "setting.111"),
    (120, "setting.120"),
    (121, "setting.121"),
    (130, "setting.130"),
    (131, "setting.131"),
];

pub struct SettingsEditor {
    pub drafts: std::collections::BTreeMap<u32, String>,
}

pub fn controller_settings(app: &mut MyApp, ui: &mut egui::Ui) {
    egui::CollapsingHeader::new(RichText::new(tr("settings.title")).strong())
        .default_open(false)
        .show(ui, |ui| {
            ui.label(RichText::new(tr("settings.hint")).weak().small());
            if ui.button(tr("settings.reload")).clicked() {
                app.device.line("$$");
            }
            let idle = app.device.state() == MachineState::Idle && !app.device.job_running();
            egui::Grid::new("grbl-settings")
                .num_columns(3)
                .striped(true)
                .show(ui, |ui| {
                    for (key, label) in SETTINGS {
                        let Some(current) = app.device.settings.get(&key).cloned() else {
                            continue;
                        };
                        ui.label(format!("${key}")).on_hover_text(tr(label));
                        ui.label(RichText::new(tr(label)).small());
                        let draft = app
                            .settings_editor
                            .drafts
                            .entry(key)
                            .or_insert_with(|| current.clone());
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(draft).desired_width(70.0));
                            let changed = draft.trim() != current.trim();
                            if ui
                                .add_enabled(
                                    changed && idle,
                                    egui::Button::new(tr("settings.write")),
                                )
                                .clicked()
                            {
                                let value = draft.trim().to_owned();
                                app.device.line(format!("${key}={value}"));
                                app.device.settings.insert(key, value);
                            }
                        });
                        ui.end_row();
                    }
                });
        });
}

const TUTORIAL: [(&str, &str); 5] = [
    ("tutorial.1_title", "tutorial.1_text"),
    ("tutorial.2_title", "tutorial.2_text"),
    ("tutorial.3_title", "tutorial.3_text"),
    ("tutorial.4_title", "tutorial.4_text"),
    ("tutorial.5_title", "tutorial.5_text"),
];

pub fn tutorial(app: &mut MyApp, ctx: &egui::Context) {
    let Some(step) = app.tutorial_step else {
        return;
    };
    if !app.library_offer.is_empty() {
        return;
    }
    let (title, text) = TUTORIAL[step.min(TUTORIAL.len() - 1)];
    egui::Window::new(tr("tutorial.window"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.label(
                RichText::new(format!("{}/{} · {}", step + 1, TUTORIAL.len(), tr(title)))
                    .strong()
                    .size(16.0),
            );
            ui.label(tr(text));
            ui.horizontal(|ui| {
                if step > 0 && ui.button(tr("tutorial.previous")).clicked() {
                    app.tutorial_step = Some(step - 1);
                }
                let last = step + 1 == TUTORIAL.len();
                if ui
                    .button(if last {
                        tr("tutorial.finish")
                    } else {
                        tr("tutorial.next")
                    })
                    .clicked()
                {
                    app.tutorial_step = if last { None } else { Some(step + 1) };
                    if last {
                        app.settings.tutorial_done = true;
                    }
                }
                if !last && ui.button(tr("tutorial.skip")).clicked() {
                    app.tutorial_step = None;
                    app.settings.tutorial_done = true;
                }
            });
        });
}

pub fn advanced_settings(app: &mut MyApp, ui: &mut egui::Ui) {
    ui.separator();
    ui.checkbox(&mut app.settings.advanced, tr("advanced.enable"))
        .on_hover_text(tr("advanced.enable_hint"));
    ui.add_enabled_ui(app.settings.advanced, |ui| {
        let mut barriers = app.settings.barriers;
        if ui
            .checkbox(&mut barriers, tr("advanced.barriers"))
            .on_hover_text(tr("advanced.barriers_hint"))
            .changed()
        {
            app.settings.barriers = barriers || !app.settings.advanced;
        }
    });
    if !app.settings.advanced {
        app.settings.barriers = true;
    }
    ui.add_enabled_ui(app.settings.barriers, |ui| {
        ui.checkbox(
            &mut app.settings.require_probe_cnc,
            tr("advanced.require_probe"),
        );
        ui.checkbox(
            &mut app.settings.require_frame_laser,
            tr("advanced.require_frame"),
        );
    });
    if ui.button(tr("tutorial.replay")).clicked() {
        app.tutorial_step = Some(0);
    }
}
