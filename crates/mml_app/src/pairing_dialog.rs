//! Pairing window: QR code and address of the drawing page, and the
//! strokes received from the tablet or phone.

use crate::app::{Level, MyApp};
use crate::i18n::{tr, trf};
use crate::pairing::Pairing;
use eframe::egui::{self, Color32, ColorImage, RichText, TextureOptions};
use mml_core::document::{Shape, ShapeKind};
use mml_core::geometry::{Path, Point, Rect, Transform, simplify_polyline, smooth_polyline};

pub struct PairingState {
    pub open: bool,
    pub server: Option<Pairing>,
    pub qr: Option<egui::TextureHandle>,
    pub shared_revision: u64,
    pub use_selection: bool,
    /// Which page the QR code opens: drawing or job monitoring.
    pub show_monitor: bool,
    pub monitor_qr: Option<egui::TextureHandle>,
    last_status: Option<std::time::Instant>,
}

impl Default for PairingState {
    fn default() -> Self {
        Self {
            open: false,
            server: None,
            qr: None,
            shared_revision: u64::MAX,
            use_selection: false,
            show_monitor: false,
            monitor_qr: None,
            last_status: None,
        }
    }
}

fn qr_texture(ctx: &egui::Context, url: &str) -> Option<egui::TextureHandle> {
    let code = qrcode::QrCode::new(url.as_bytes()).ok()?;
    let width = code.width();
    let quiet = 2;
    let scale = 6;
    let size = (width + 2 * quiet) * scale;
    let colors = code.to_colors();
    let mut pixels = vec![Color32::WHITE; size * size];
    for y in 0..width {
        for x in 0..width {
            if colors[y * width + x] == qrcode::Color::Dark {
                for dy in 0..scale {
                    for dx in 0..scale {
                        pixels[((y + quiet) * scale + dy) * size + (x + quiet) * scale + dx] =
                            Color32::BLACK;
                    }
                }
            }
        }
    }
    Some(ctx.load_texture(
        "pairing-qr",
        ColorImage::new([size, size], pixels),
        TextureOptions::NEAREST,
    ))
}

/// State of the machine and of the job for the monitoring page.
fn status_json(app: &MyApp) -> String {
    let device = &app.device;
    let state = device.state();
    let job = device.job.as_ref().filter(|job| job.kind == crate::machine_panel::JobKind::Main).map(|job| {
        let share = if job.total > 0 { job.acknowledged as f64 / job.total as f64 } else { 0.0 };
        let elapsed = job.started.elapsed().as_secs_f64();
        let remaining = (job.estimate.as_secs_f64() * (1.0 - share)).max(0.0);
        serde_json::json!({
            "name": app.job.built.as_ref().and_then(|built| built.external.clone()).unwrap_or_else(|| app.document_name()),
            "done": job.acknowledged,
            "total": job.total,
            "elapsed": elapsed,
            "remaining": remaining,
            "paused": job.paused,
        })
    });
    let message = device
        .anomaly
        .as_ref()
        .map(|(anomaly, _)| format!("{anomaly:?}"))
        .or_else(|| device.alarm.map(crate::i18n::grbl_alarm));
    serde_json::json!({
        "machine": app.machine.name,
        "state": format!("{state:?}"),
        "state_label": if device.is_connected() { crate::machine_panel::state_label(state) } else { tr("device.disconnected") },
        "job": job,
        "position": device.status.as_ref().map(|status| status.work_position),
        "message": message,
        "labels": {
            "pause": tr("monitor.pause"),
            "estop": tr("monitor.estop"),
            "remaining": tr("monitor.remaining"),
            "idle": tr("monitor.idle"),
            "offline": tr("monitor.offline"),
            "note": tr("monitor.note"),
        },
    })
    .to_string()
}

/// Receives strokes and keeps the design shown on the tablet current.
pub fn poll(app: &mut MyApp) {
    let Some(server) = &app.pairing.server else {
        return;
    };
    // Commands from the phone: only pause and emergency stop.
    let mut commands = Vec::new();
    while let Ok(command) = server.remote.try_recv() {
        commands.push(command);
    }
    let due = app
        .pairing
        .last_status
        .is_none_or(|time| time.elapsed().as_millis() > 500);
    if due {
        server.update_status(status_json(app));
        app.pairing.last_status = Some(std::time::Instant::now());
    }
    for command in commands {
        match command {
            crate::pairing::RemoteCommand::Pause => {
                if app.device.job_running() {
                    app.device.send(mml_device::Command::Pause);
                    app.notify(tr("monitor.paused_remote"), Level::Warning);
                }
            }
            crate::pairing::RemoteCommand::EmergencyStop => {
                app.device.send(mml_device::Command::EmergencyStop);
                app.notify(tr("monitor.estop_remote"), Level::Error);
            }
        }
    }
    let Some(server) = &app.pairing.server else {
        return;
    };
    if app.pairing.shared_revision != app.revision {
        server.update_view(&app.document);
        app.pairing.shared_revision = app.revision;
    }
    let mut batches = Vec::new();
    while let Ok(strokes) = server.strokes.try_recv() {
        batches.push(strokes);
    }
    if batches.is_empty() {
        return;
    }
    let area = server.area;
    app.begin_edit("history.tablet");
    let layer = app.layer_for(false);
    let mut added = 0;
    for stroke in batches.into_iter().flatten() {
        let points: Vec<Point> = stroke
            .iter()
            .map(|[x, y]| Point::new(area.min.x + x, area.min.y + y))
            .collect();
        let smoothed = smooth_polyline(&points, 5);
        let mut points = simplify_polyline(&smoothed, 0.05);
        let closed = points.len() > 3 && points[0].distance(*points.last().unwrap()) < 1.5;
        if closed {
            points.pop();
        }
        let path = Path::new(points, closed);
        let center = path.bounds().center();
        let local = Path::new(
            path.points.iter().map(|point| *point - center).collect(),
            closed,
        );
        let mut shape = Shape::new(
            tr("shape.tablet"),
            layer,
            ShapeKind::Paths { paths: vec![local] },
        );
        shape.transform = Transform::translate(center.x, center.y);
        app.document.shapes.push(shape);
        added += 1;
    }
    app.end_edit();
    if let Some(server) = &mut app.pairing.server {
        server.received += added;
    }
}

pub fn show(app: &mut MyApp, ctx: &egui::Context) {
    if !app.pairing.open {
        return;
    }
    let mut open = true;
    let mut start = None;
    let mut stop = false;
    egui::Window::new(tr("pairing.title"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.set_max_width(380.0);
            match &app.pairing.server {
                None => {
                    ui.label(tr("pairing.intro"));
                    let selection = app.document.bounds_of(&app.selection);
                    ui.horizontal(|ui| {
                        ui.label(tr("pairing.area"));
                        ui.selectable_value(
                            &mut app.pairing.use_selection,
                            false,
                            tr("pairing.bed"),
                        );
                        ui.add_enabled_ui(!selection.is_empty(), |ui| {
                            ui.selectable_value(
                                &mut app.pairing.use_selection,
                                true,
                                tr("pairing.selection"),
                            );
                        });
                    });
                    if ui
                        .button(RichText::new(tr("pairing.start")).strong())
                        .clicked()
                    {
                        let bed = Rect::from_points(
                            Point::default(),
                            Point::new(app.machine.width, app.machine.height),
                        );
                        start = Some(if app.pairing.use_selection && !selection.is_empty() {
                            selection
                        } else {
                            bed
                        });
                    }
                }
                Some(server) => {
                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut app.pairing.show_monitor,
                            false,
                            tr("pairing.page_draw"),
                        );
                        ui.selectable_value(
                            &mut app.pairing.show_monitor,
                            true,
                            tr("pairing.page_monitor"),
                        )
                        .on_hover_text(tr("pairing.page_monitor_hint"));
                    });
                    ui.label(tr("pairing.scan"));
                    let (qr, url) = if app.pairing.show_monitor {
                        (&app.pairing.monitor_qr, &server.monitor_url)
                    } else {
                        (&app.pairing.qr, &server.url)
                    };
                    if let Some(texture) = qr {
                        ui.vertical_centered(|ui| {
                            ui.image((texture.id(), egui::vec2(220.0, 220.0)))
                        });
                    }
                    ui.horizontal(|ui| {
                        ui.monospace(url);
                        if ui.small_button(tr("pairing.copy")).clicked() {
                            ui.ctx().copy_text(url.clone());
                        }
                    });
                    ui.label(
                        RichText::new(trf(
                            "pairing.area_size",
                            &[
                                &format!("{:.0}", server.area.width()),
                                &format!("{:.0}", server.area.height()),
                            ],
                        ))
                        .weak(),
                    );
                    ui.label(trf("pairing.received", &[&server.received]));
                    ui.label(RichText::new(tr("pairing.privacy")).weak().small());
                    stop = ui.button(tr("pairing.stop")).clicked();
                }
            }
        });
    if let Some(area) = start {
        match Pairing::start(area) {
            Ok(server) => {
                app.pairing.qr = qr_texture(ctx, &server.url);
                app.pairing.monitor_qr = qr_texture(ctx, &server.monitor_url);
                app.pairing.shared_revision = u64::MAX;
                app.pairing.server = Some(server);
            }
            Err(error) => app.notify(trf("pairing.failed", &[&error]), Level::Error),
        }
    }
    if stop || !open {
        app.pairing.server = None;
        app.pairing.qr = None;
    }
    app.pairing.open = open && !stop;
}
