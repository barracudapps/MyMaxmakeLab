//! Pairing window: QR code and address of the drawing page, and the
//! strokes received from the tablet or phone.

use crate::app::{Level, MyApp};
use crate::i18n::{tr, trf};
use crate::pairing::Pairing;
use eframe::egui::{self, Color32, ColorImage, RichText, TextureOptions};
use mml_core::brush::{self, BrushStyle, StrokePoint};
use mml_core::document::{Shape, ShapeId, ShapeKind};
use mml_core::geometry::{Path, Point, Rect, Transform};
use mml_core::layer::{Layer, LayerId, Operation};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

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
    /// Shapes made from tablet strokes, by session and stroke id, with
    /// a fingerprint of the stroke they were made from.
    owned: HashMap<(String, String), (ShapeId, u64)>,
    /// Layer of each kind of stroke: filled or not, intensity.
    layers: HashMap<(bool, u8), LayerId>,
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
            owned: HashMap::new(),
            layers: HashMap::new(),
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

/// Texts of the drawing page in the language of the application.
fn page_labels() -> String {
    let keys = [
        "pen",
        "eraser",
        "select",
        "line",
        "marker",
        "brush",
        "width",
        "light",
        "medium",
        "strong",
        "undo",
        "redo",
        "clear",
        "send",
        "live",
        "sent",
        "hint",
        "offline",
        "keep_ratio",
        "delete",
        "intensity",
        "style",
        "rejected",
    ];
    let labels: serde_json::Map<String, serde_json::Value> = keys
        .iter()
        .map(|key| {
            (
                key.to_string(),
                serde_json::Value::from(crate::i18n::tr_key(&format!("draw.{key}"))),
            )
        })
        .collect();
    serde_json::Value::Object(labels).to_string()
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
        let skip: Vec<ShapeId> = app.pairing.owned.values().map(|(id, _)| *id).collect();
        server.update_view(&app.document, &skip);
        server.update_labels(page_labels());
        app.pairing.shared_revision = app.revision;
    }
    let mut syncs = Vec::new();
    while let Ok(sync) = server.strokes.try_recv() {
        syncs.push(sync);
    }
    // Only the latest state of each page matters.
    let mut latest: HashMap<String, crate::pairing::Sync> = HashMap::new();
    for sync in syncs {
        latest.insert(sync.session.clone(), sync);
    }
    let area = server.area;
    for sync in latest.into_values() {
        apply_sync(app, area, sync);
    }
}

fn fingerprint(stroke: &crate::pairing::Stroke) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    stroke.style.hash(&mut hasher);
    stroke.level.hash(&mut hasher);
    stroke.width.to_bits().hash(&mut hasher);
    for point in &stroke.points {
        for value in point {
            value.to_bits().hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// Share of the layer power or depth for each intensity.
const LEVELS: [f64; 3] = [0.35, 0.65, 1.0];

/// Layer receiving the strokes of one kind, created on first use with
/// the power (laser) or depth (CNC) of its intensity.
fn stroke_layer(app: &mut MyApp, filled: bool, level: u8) -> LayerId {
    if let Some(id) = app.pairing.layers.get(&(filled, level))
        && app.document.layer(*id).is_some()
    {
        return *id;
    }
    let share = LEVELS[(level.clamp(1, 3) - 1) as usize];
    let mut operation = match (app.machine.kind, filled) {
        (mml_core::machine::MachineKind::Laser, true) => Operation::LaserFill(Default::default()),
        (mml_core::machine::MachineKind::Cnc, true) => Operation::CncPocket(Default::default()),
        _ => app.default_vector_operation(),
    };
    match &mut operation {
        Operation::LaserLine(params) => params.power = (params.power * share).round(),
        Operation::LaserFill(params) => params.power = (params.power * share).round(),
        Operation::CncProfile(params) => {
            params.depth = (params.depth * share * 100.0).round() / 100.0
        }
        Operation::CncPocket(params) => {
            params.depth = (params.depth * share * 100.0).round() / 100.0
        }
        _ => {}
    }
    let style = if filled {
        tr("tablet.filled")
    } else {
        tr("tablet.lines")
    };
    let name = trf(
        "tablet.layer",
        &[
            &style,
            &tr(["tablet.light", "tablet.medium", "tablet.strong"]
                [(level.clamp(1, 3) - 1) as usize]),
        ],
    );
    let layer = Layer::new(name, app.document.next_layer_color(), operation);
    let id = layer.id;
    app.document.layers.push(layer);
    app.pairing.layers.insert((filled, level), id);
    id
}

fn stroke_shape(app: &mut MyApp, area: Rect, stroke: &crate::pairing::Stroke) -> Option<Shape> {
    let points: Vec<StrokePoint> = stroke
        .points
        .iter()
        .map(|[x, y, pressure]| StrokePoint {
            at: Point::new(area.min.x + x, area.min.y + y),
            pressure: *pressure,
        })
        .collect();
    let paths = brush::outline(&points, stroke.style, stroke.width);
    if paths.is_empty() || (stroke.style == BrushStyle::Line && paths[0].points.len() < 2) {
        return None;
    }
    let filled = stroke.style != BrushStyle::Line;
    let layer = stroke_layer(app, filled, stroke.level);
    let mut bounds = Rect::EMPTY;
    paths
        .iter()
        .flat_map(|path| &path.points)
        .for_each(|point| bounds.include(*point));
    let center = bounds.center();
    let local: Vec<Path> = paths
        .iter()
        .map(|path| {
            Path::new(
                path.points.iter().map(|point| *point - center).collect(),
                path.closed,
            )
        })
        .collect();
    let mut shape = Shape::new(tr("shape.tablet"), layer, ShapeKind::Paths { paths: local });
    shape.transform = Transform::translate(center.x, center.y);
    Some(shape)
}

/// Mirrors the strokes of one page: new strokes are added, changed ones
/// (moved, resized, partly erased) replaced, erased ones removed. Shapes
/// deleted or edited on the computer are left as they are until the
/// tablet changes that stroke again.
fn apply_sync(app: &mut MyApp, area: Rect, sync: crate::pairing::Sync) {
    let session = sync.session.clone();
    let listed: std::collections::HashSet<&str> = sync
        .strokes
        .iter()
        .map(|stroke| stroke.id.as_str())
        .collect();
    let erased: Vec<(String, String)> = app
        .pairing
        .owned
        .keys()
        .filter(|(owner, id)| *owner == session && !listed.contains(id.as_str()))
        .cloned()
        .collect();
    let changed: Vec<&crate::pairing::Stroke> = sync
        .strokes
        .iter()
        .filter(|stroke| {
            app.pairing
                .owned
                .get(&(session.clone(), stroke.id.clone()))
                .is_none_or(|(_, print)| *print != fingerprint(stroke))
        })
        .collect();
    if erased.is_empty() && changed.is_empty() {
        return;
    }
    app.begin_edit("history.tablet");
    for key in erased {
        if let Some((shape_id, _)) = app.pairing.owned.remove(&key) {
            app.document.shapes.retain(|shape| shape.id != shape_id);
            app.selection.retain(|id| *id != shape_id);
        }
    }
    for stroke in changed {
        let key = (session.clone(), stroke.id.clone());
        let previous = app.pairing.owned.get(&key).map(|(id, _)| *id);
        let Some(mut shape) = stroke_shape(app, area, stroke) else {
            continue;
        };
        let index =
            previous.and_then(|id| app.document.shapes.iter().position(|shape| shape.id == id));
        if let Some(index) = index {
            shape.id = app.document.shapes[index].id;
        }
        let id = shape.id;
        match index {
            Some(index) => app.document.shapes[index] = shape,
            None => app.document.shapes.push(shape),
        }
        app.pairing.owned.insert(key, (id, fingerprint(stroke)));
    }
    app.end_edit();
    let count = app.pairing.owned.len();
    if let Some(server) = &mut app.pairing.server {
        server.received = count;
    }
}

pub fn show(app: &mut MyApp, ctx: &egui::Context) {
    if !app.pairing.open {
        return;
    }
    if let Some(server) = &app.pairing.server
        && app.pairing.qr.is_none()
    {
        app.pairing.qr = qr_texture(ctx, &server.url);
        app.pairing.monitor_qr = qr_texture(ctx, &server.monitor_url);
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
