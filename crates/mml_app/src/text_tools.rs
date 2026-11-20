//! Lettering and artwork tools: text with fonts and shapes (arc, wave,
//! bulge), deformation of any vector shape, and SVG code insertion.

use crate::app::{Level, MyApp};
use crate::i18n::{tr, trf};
use eframe::egui::{self, Color32, DragValue, Pos2, RichText, Sense, Stroke};
use mml_core::document::{Shape, ShapeKind};
use mml_core::geometry::{Path, Point, Transform, bounds_of};
use mml_core::warp::{Warp, warp_paths};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WarpChoice {
    None,
    ArcOver,
    ArcUnder,
    Wave,
    Bulge,
}

/// Shape settings shared by the text dialog and the deform dialog.
#[derive(Debug, Clone)]
pub struct WarpSettings {
    pub choice: WarpChoice,
    pub radius: f64,
    pub amplitude: f64,
    pub wavelength: f64,
    pub bulge: f64,
}

impl Default for WarpSettings {
    fn default() -> Self {
        Self {
            choice: WarpChoice::None,
            radius: 40.0,
            amplitude: 3.0,
            wavelength: 40.0,
            bulge: 0.4,
        }
    }
}

impl WarpSettings {
    pub fn warp(&self) -> Option<Warp> {
        match self.choice {
            WarpChoice::None => None,
            WarpChoice::ArcOver => Some(Warp::ArcOver {
                radius: self.radius,
            }),
            WarpChoice::ArcUnder => Some(Warp::ArcUnder {
                radius: self.radius,
            }),
            WarpChoice::Wave => Some(Warp::Wave {
                amplitude: self.amplitude,
                wavelength: self.wavelength,
            }),
            WarpChoice::Bulge => Some(Warp::Bulge { amount: self.bulge }),
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for (choice, key) in [
                (WarpChoice::None, "warp.none"),
                (WarpChoice::ArcOver, "warp.arc_over"),
                (WarpChoice::ArcUnder, "warp.arc_under"),
                (WarpChoice::Wave, "warp.wave"),
                (WarpChoice::Bulge, "warp.bulge"),
            ] {
                ui.selectable_value(&mut self.choice, choice, tr(key));
            }
        });
        ui.horizontal(|ui| match self.choice {
            WarpChoice::ArcOver | WarpChoice::ArcUnder => {
                ui.label(tr("warp.radius"));
                ui.add(
                    DragValue::new(&mut self.radius)
                        .range(2.0..=2000.0)
                        .suffix(" mm")
                        .speed(0.5),
                );
            }
            WarpChoice::Wave => {
                ui.label(tr("warp.amplitude"));
                ui.add(
                    DragValue::new(&mut self.amplitude)
                        .range(0.1..=200.0)
                        .suffix(" mm")
                        .speed(0.1),
                );
                ui.label(tr("warp.wavelength"));
                ui.add(
                    DragValue::new(&mut self.wavelength)
                        .range(2.0..=2000.0)
                        .suffix(" mm")
                        .speed(0.5),
                );
            }
            WarpChoice::Bulge => {
                ui.label(tr("warp.amount"));
                ui.add(egui::Slider::new(&mut self.bulge, -0.9..=2.0));
            }
            WarpChoice::None => {}
        });
    }
}

pub struct TextDialog {
    pub position: Point,
    pub text: String,
    pub style: mml_io::TextStyle,
    pub families: Vec<String>,
    pub bundled: Vec<String>,
    pub warp: WarpSettings,
}

impl TextDialog {
    pub fn new(position: Point) -> Self {
        Self {
            position,
            text: String::new(),
            style: mml_io::TextStyle {
                family: "Montserrat".into(),
                ..mml_io::TextStyle::default()
            },
            families: mml_io::font_families(),
            bundled: mml_io::bundled_families(),
            warp: WarpSettings::default(),
        }
    }

    fn paths(&self) -> Vec<Path> {
        let paths = mml_io::text_to_paths(&self.text, &self.style);
        match self.warp.warp() {
            Some(warp) => warp_paths(&paths, warp),
            None => paths,
        }
    }
}

/// Small live preview of paths, fitted in the available width.
fn preview(ui: &mut egui::Ui, paths: &[Path]) {
    let (response, painter) =
        ui.allocate_painter(egui::vec2(ui.available_width(), 120.0), Sense::hover());
    let rect = response.rect;
    painter.rect_filled(rect, 10.0, ui.visuals().extreme_bg_color);
    let bounds = bounds_of(paths);
    if bounds.is_empty() {
        return;
    }
    let scale = ((rect.width() - 20.0) / bounds.width().max(1e-6) as f32)
        .min((rect.height() - 20.0) / bounds.height().max(1e-6) as f32);
    let center = bounds.center();
    let to_screen = |point: Point| {
        Pos2::new(
            rect.center().x + (point.x - center.x) as f32 * scale,
            rect.center().y - (point.y - center.y) as f32 * scale,
        )
    };
    let stroke = Stroke::new(1.2, crate::theme::ACCENT);
    for path in paths {
        let points: Vec<Pos2> = path.points.iter().map(|point| to_screen(*point)).collect();
        if points.len() > 1 {
            painter.add(if path.closed {
                egui::Shape::closed_line(points, stroke)
            } else {
                egui::Shape::line(points, stroke)
            });
        }
    }
}

fn add_paths(app: &mut MyApp, name: String, paths: Vec<Path>, at: Point) {
    let bounds = bounds_of(&paths);
    let center = bounds.center();
    let local = paths
        .into_iter()
        .map(|path| {
            Path::new(
                path.points
                    .into_iter()
                    .map(|point| point - center)
                    .collect(),
                path.closed,
            )
        })
        .collect();
    let layer = app.layer_for(false);
    let mut shape = Shape::new(name, layer, ShapeKind::Paths { paths: local });
    shape.transform =
        Transform::translate(at.x + bounds.width() / 2.0, at.y + bounds.height() / 2.0);
    app.add_shape(shape);
}

fn add_fonts(app: &mut MyApp) -> bool {
    let Some(directory) = crate::settings::fonts_dir() else {
        return false;
    };
    let Some(files) = rfd::FileDialog::new()
        .add_filter(tr("text.font_files"), &["ttf", "otf", "ttc"])
        .pick_files()
    else {
        return false;
    };
    if std::fs::create_dir_all(&directory).is_err() {
        return false;
    }
    let copied = files
        .iter()
        .filter(|file| {
            file.file_name()
                .is_some_and(|name| std::fs::copy(file, directory.join(name)).is_ok())
        })
        .count();
    mml_io::add_font_directory(&directory);
    app.notify(trf("text.fonts_added", &[&copied]), Level::Info);
    true
}

pub fn text_dialog(app: &mut MyApp, ctx: &egui::Context) {
    if app.text_dialog.is_none() {
        return;
    }
    let mut open = true;
    let mut create = false;
    let mut want_fonts = false;
    let dialog = app.text_dialog.as_mut().unwrap();
    egui::Window::new(tr("text.title"))
        .open(&mut open)
        .collapsible(false)
        .default_width(420.0)
        .show(ctx, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut dialog.text)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY)
                    .hint_text(tr("text.hint")),
            );
            ui.horizontal(|ui| {
                ui.label(tr("text.font"));
                egui::ComboBox::from_id_salt("font")
                    .selected_text(&dialog.style.family)
                    .height(360.0)
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        ui.label(RichText::new(tr("text.bundled")).weak().small());
                        for family in &dialog.bundled {
                            ui.selectable_value(&mut dialog.style.family, family.clone(), family);
                        }
                        ui.separator();
                        ui.label(RichText::new(tr("text.system")).weak().small());
                        for family in dialog
                            .families
                            .iter()
                            .filter(|family| !dialog.bundled.contains(family))
                        {
                            ui.selectable_value(&mut dialog.style.family, family.clone(), family);
                        }
                    });
                want_fonts = ui
                    .small_button("+")
                    .on_hover_text(tr("text.add_fonts"))
                    .clicked();
            });
            ui.horizontal(|ui| {
                ui.label(tr("text.size"));
                ui.add(
                    DragValue::new(&mut dialog.style.size)
                        .range(1.0..=500.0)
                        .suffix(" mm")
                        .speed(0.2),
                );
                ui.checkbox(&mut dialog.style.bold, tr("text.bold"));
                ui.checkbox(&mut dialog.style.italic, tr("text.italic"));
            });
            ui.label(RichText::new(tr("text.shape")).strong());
            dialog.warp.ui(ui);
            preview(ui, &dialog.paths());
            create = ui
                .add_enabled(
                    !dialog.text.trim().is_empty(),
                    egui::Button::new(RichText::new(tr("text.create")).strong()),
                )
                .clicked();
        });
    if want_fonts
        && add_fonts(app)
        && let Some(dialog) = &mut app.text_dialog
    {
        dialog.families = mml_io::font_families();
    }
    if create {
        let dialog = app.text_dialog.take().unwrap();
        let paths = dialog.paths();
        if paths.is_empty() {
            app.notify(tr("text.no_font"), Level::Error);
            return;
        }
        let name = dialog
            .text
            .lines()
            .next()
            .unwrap_or_default()
            .chars()
            .take(24)
            .collect::<String>();
        add_paths(app, name, paths, dialog.position);
    } else if !open {
        app.text_dialog = None;
    }
}

/// Deforms the selected shape in place (primitives become paths).
pub fn deform_dialog(app: &mut MyApp, ctx: &egui::Context) {
    let Some((id, settings)) = &mut app.deform else {
        return;
    };
    let id = *id;
    let Some(shape) = app.document.shape(id).cloned() else {
        app.deform = None;
        return;
    };
    let mut open = true;
    let mut apply = false;
    let local = shape.local_paths();
    egui::Window::new(tr("warp.title"))
        .open(&mut open)
        .collapsible(false)
        .default_width(380.0)
        .show(ctx, |ui| {
            settings.ui(ui);
            let preview_paths = settings
                .warp()
                .map_or_else(|| local.clone(), |warp| warp_paths(&local, warp));
            preview(ui, &preview_paths);
            apply = ui
                .add_enabled(
                    settings.warp().is_some(),
                    egui::Button::new(RichText::new(tr("dialog.apply")).strong()),
                )
                .clicked();
        });
    if apply && let Some(warp) = settings.warp() {
        let warped = warp_paths(&local, warp);
        app.begin_edit();
        if let Some(target) = app.document.shape_mut(id) {
            target.kind = ShapeKind::Paths { paths: warped };
        }
        app.end_edit();
        app.deform = None;
    } else if !open {
        app.deform = None;
    }
}

pub struct SvgCodeDialog {
    pub code: String,
}

impl Default for SvgCodeDialog {
    fn default() -> Self {
        Self {
            code: concat!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"60mm\" height=\"60mm\" viewBox=\"0 0 60 60\">\n",
                "  <style>.cut { fill: none; stroke: red; }</style>\n",
                "  <circle class=\"cut\" cx=\"30\" cy=\"30\" r=\"28\"/>\n",
                "  <path d=\"M30 8 L36 24 L53 24 L39 34 L44 51 L30 41 L16 51 L21 34 L7 24 L24 24 Z\" fill=\"blue\"/>\n",
                "</svg>\n"
            )
            .to_owned(),
        }
    }
}

/// Accepts SVG (with its CSS) or an HTML snippet containing an `<svg>`.
fn extract_svg(code: &str) -> &str {
    match (code.find("<svg"), code.rfind("</svg>")) {
        (Some(start), Some(end)) if end > start => &code[start..end + "</svg>".len()],
        _ => code,
    }
}

pub fn svg_code_dialog(app: &mut MyApp, ctx: &egui::Context) {
    let Some(dialog) = &mut app.svg_code else {
        return;
    };
    let mut open = true;
    let mut insert = false;
    egui::Window::new(tr("svgcode.title"))
        .open(&mut open)
        .default_width(520.0)
        .show(ctx, |ui| {
            ui.label(RichText::new(tr("svgcode.hint")).weak());
            egui::ScrollArea::vertical()
                .max_height(280.0)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut dialog.code)
                            .code_editor()
                            .desired_rows(12)
                            .desired_width(f32::INFINITY),
                    );
                });
            let result = mml_io::import_svg(extract_svg(&dialog.code).as_bytes());
            match &result {
                Ok(groups) => {
                    let paths: Vec<Path> = groups
                        .iter()
                        .flat_map(|group| group.paths.clone())
                        .collect();
                    preview(ui, &paths);
                }
                Err(error) => {
                    ui.label(
                        RichText::new(error.to_string()).color(Color32::from_rgb(222, 74, 66)),
                    );
                }
            }
            insert = ui
                .add_enabled(
                    result.is_ok(),
                    egui::Button::new(RichText::new(tr("svgcode.insert")).strong()),
                )
                .clicked();
        });
    if insert {
        let code = extract_svg(&app.svg_code.take().unwrap().code).to_owned();
        if let Ok(groups) = mml_io::import_svg(code.as_bytes()) {
            app.import_groups(groups, tr("svgcode.name"));
        }
    } else if !open {
        app.svg_code = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_is_extracted_from_html() {
        let html = "<html><body><p>logo</p><svg viewBox=\"0 0 1 1\"><rect width=\"1\" height=\"1\"/></svg></body></html>";
        assert_eq!(
            extract_svg(html),
            "<svg viewBox=\"0 0 1 1\"><rect width=\"1\" height=\"1\"/></svg>"
        );
        assert_eq!(extract_svg("plain"), "plain");
    }

    #[test]
    fn default_svg_code_imports() {
        let code = SvgCodeDialog::default().code;
        let groups = mml_io::import_svg(extract_svg(&code).as_bytes()).unwrap();
        assert_eq!(
            groups.len(),
            2,
            "red outline and blue star are separate colours"
        );
    }
}
