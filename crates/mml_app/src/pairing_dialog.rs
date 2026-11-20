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
}

impl Default for PairingState {
    fn default() -> Self {
        Self {
            open: false,
            server: None,
            qr: None,
            shared_revision: u64::MAX,
            use_selection: false,
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

/// Receives strokes and keeps the design shown on the tablet current.
pub fn poll(app: &mut MyApp) {
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
                    ui.label(tr("pairing.scan"));
                    if let Some(texture) = &app.pairing.qr {
                        ui.vertical_centered(|ui| {
                            ui.image((texture.id(), egui::vec2(220.0, 220.0)))
                        });
                    }
                    ui.horizontal(|ui| {
                        ui.monospace(&server.url);
                        if ui.small_button(tr("pairing.copy")).clicked() {
                            ui.ctx().copy_text(server.url.clone());
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
