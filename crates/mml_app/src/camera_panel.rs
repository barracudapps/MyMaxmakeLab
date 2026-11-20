//! Camera window: photo or live snapshots of the bed, four-click
//! calibration, rectified overlay under the design.

use crate::app::{Level, MyApp};
use crate::i18n::{tr, trf};
use eframe::egui::{self, Color32, ColorImage, Pos2, RichText, Sense, Stroke, TextureOptions};
use mml_core::geometry::Point;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

pub struct CameraState {
    pub open: bool,
    photo: Option<image::RgbaImage>,
    photo_texture: Option<egui::TextureHandle>,
    clicks: Vec<Point>,
    pub opacity: f32,
    pub live: bool,
    last_fetch: Option<Instant>,
    fetching: Option<Receiver<Result<Vec<u8>, String>>>,
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            open: false,
            photo: None,
            photo_texture: None,
            clicks: Vec::new(),
            opacity: 0.6,
            live: false,
            last_fetch: None,
            fetching: None,
        }
    }
}

const CORNER_KEYS: [&str; 4] = [
    "camera.front_left",
    "camera.front_right",
    "camera.back_right",
    "camera.back_left",
];

fn set_photo(app: &mut MyApp, ctx: &egui::Context, data: &[u8]) -> Result<(), String> {
    let photo = image::load_from_memory(data)
        .map_err(|error| error.to_string())?
        .to_rgba8();
    let size = [photo.width() as usize, photo.height() as usize];
    let texture = ctx.load_texture(
        "camera-photo",
        ColorImage::from_rgba_unmultiplied(size, photo.as_raw()),
        TextureOptions::LINEAR,
    );
    app.camera.photo_texture = Some(texture);
    app.camera.photo = Some(photo);
    refresh_overlay(app, ctx);
    Ok(())
}

/// Rectifies the current photo with the saved corners and shows it on
/// the canvas.
fn refresh_overlay(app: &mut MyApp, ctx: &egui::Context) {
    let (Some(photo), Some(corners)) = (&app.camera.photo, app.settings.camera_corners) else {
        return;
    };
    let corners = corners.map(|[x, y]| Point::new(x, y));
    let bed = (app.machine.width, app.machine.height);
    let resolution = (1600.0 / bed.0.max(bed.1)).clamp(0.5, 4.0);
    if let Some(rectified) = mml_io::camera::rectify(photo, corners, bed, resolution) {
        let size = [rectified.width() as usize, rectified.height() as usize];
        let texture = ctx.load_texture(
            "camera-overlay",
            ColorImage::from_rgba_unmultiplied(size, rectified.as_raw()),
            TextureOptions::LINEAR,
        );
        app.canvas.camera = Some((texture, app.camera.opacity));
    }
}

/// Live mode: fetches a new snapshot every two seconds in background.
pub fn poll(app: &mut MyApp, ctx: &egui::Context) {
    if let Some(receiver) = &app.camera.fetching
        && let Ok(result) = receiver.try_recv()
    {
        app.camera.fetching = None;
        match result.and_then(|data| set_photo(app, ctx, &data)) {
            Ok(()) => {}
            Err(error) => {
                app.camera.live = false;
                app.notify(trf("camera.fetch_failed", &[&error]), Level::Error);
            }
        }
    }
    let due = app
        .camera
        .last_fetch
        .is_none_or(|time| time.elapsed() > Duration::from_secs(2));
    if app.camera.live
        && due
        && app.camera.fetching.is_none()
        && !app.settings.camera_url.is_empty()
    {
        let (sender, receiver) = channel();
        let url = app.settings.camera_url.clone();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let _ = sender.send(mml_io::camera::fetch_snapshot(&url));
            repaint.request_repaint();
        });
        app.camera.fetching = Some(receiver);
        app.camera.last_fetch = Some(Instant::now());
    }
    if app.camera.live {
        ctx.request_repaint_after(Duration::from_millis(500));
    }
}

pub fn show(app: &mut MyApp, ctx: &egui::Context) {
    if !app.camera.open {
        return;
    }
    let mut open = true;
    let mut load = false;
    let mut calibrated = false;
    egui::Window::new(tr("camera.title"))
        .open(&mut open)
        .default_width(560.0)
        .show(ctx, |ui| {
            ui.label(RichText::new(tr("camera.intro")).weak());
            ui.horizontal(|ui| {
                load = ui.button(tr("camera.load_photo")).clicked();
                ui.label(tr("camera.url"));
                ui.add(
                    egui::TextEdit::singleline(&mut app.settings.camera_url)
                        .hint_text("http://192.168.1.20:8080/shot.jpg")
                        .desired_width(220.0),
                );
                ui.checkbox(&mut app.camera.live, tr("camera.live"))
                    .on_hover_text(tr("camera.live_hint"));
            });
            if ui
                .add(
                    egui::Slider::new(&mut app.camera.opacity, 0.1..=1.0)
                        .text(tr("camera.opacity")),
                )
                .changed()
                && let Some((_, opacity)) = &mut app.canvas.camera
            {
                *opacity = app.camera.opacity;
            }
            let Some(texture) = &app.camera.photo_texture else {
                return;
            };
            let calibrating =
                app.settings.camera_corners.is_none() || !app.camera.clicks.is_empty();
            if calibrating {
                let next = app.camera.clicks.len().min(3);
                ui.label(
                    RichText::new(trf("camera.click", &[&(next + 1), &tr(CORNER_KEYS[next])]))
                        .strong(),
                );
            } else {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tr("camera.calibrated")).color(crate::theme::SUCCESS));
                    if ui.button(tr("camera.recalibrate")).clicked() {
                        app.settings.camera_corners = None;
                        app.camera.clicks.clear();
                        app.canvas.camera = None;
                    }
                });
            }
            let size = texture.size_vec2();
            let scale = (ui.available_width() / size.x).min(420.0 / size.y);
            let (response, painter) = ui.allocate_painter(size * scale, Sense::click());
            let rect = response.rect;
            painter.image(
                texture.id(),
                rect,
                egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
            let to_screen = |point: Point| {
                Pos2::new(
                    rect.left() + point.x as f32 * scale,
                    rect.top() + point.y as f32 * scale,
                )
            };
            let shown: Vec<Point> = if calibrating {
                app.camera.clicks.clone()
            } else {
                app.settings
                    .camera_corners
                    .map(|corners| corners.map(|[x, y]| Point::new(x, y)).to_vec())
                    .unwrap_or_default()
            };
            for (index, point) in shown.iter().enumerate() {
                painter.circle_filled(to_screen(*point), 6.0, crate::theme::ACCENT);
                painter.text(
                    to_screen(*point) + egui::vec2(9.0, -9.0),
                    egui::Align2::LEFT_BOTTOM,
                    (index + 1).to_string(),
                    egui::FontId::proportional(14.0),
                    Color32::WHITE,
                );
            }
            if shown.len() > 1 {
                let mut outline: Vec<Pos2> = shown.iter().map(|point| to_screen(*point)).collect();
                if shown.len() == 4 {
                    outline.push(outline[0]);
                }
                painter.add(egui::Shape::line(
                    outline,
                    Stroke::new(1.5, crate::theme::ACCENT),
                ));
            }
            if calibrating
                && response.clicked()
                && let Some(position) = response.interact_pointer_pos()
            {
                let point = Point::new(
                    ((position.x - rect.left()) / scale) as f64,
                    ((position.y - rect.top()) / scale) as f64,
                );
                app.camera.clicks.push(point);
                if app.camera.clicks.len() == 4 {
                    let clicks = std::mem::take(&mut app.camera.clicks);
                    app.settings.camera_corners =
                        Some([0, 1, 2, 3].map(|index| [clicks[index].x, clicks[index].y]));
                    calibrated = true;
                }
            }
        });
    if load
        && let Some(path) = rfd::FileDialog::new()
            .add_filter(tr("file.importable"), &["png", "jpg", "jpeg", "bmp"])
            .pick_file()
    {
        match std::fs::read(&path)
            .map_err(|error| error.to_string())
            .and_then(|data| set_photo(app, ctx, &data))
        {
            Ok(()) => {}
            Err(error) => app.notify(
                trf("file.open_failed", &[&path.display(), &error]),
                Level::Error,
            ),
        }
    }
    if calibrated {
        refresh_overlay(app, ctx);
    }
    app.camera.open = open;
}
