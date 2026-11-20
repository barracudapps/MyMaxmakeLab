//! Random ornament generator: pick a style, get a frieze or a tiling,
//! roll new variations until one pleases, insert it as vectors.

use crate::app::MyApp;
use crate::i18n::tr;
use eframe::egui::{self, DragValue, Pos2, RichText, Sense, Stroke};
use mml_core::document::{Shape, ShapeKind};
use mml_core::ornament::{OrnamentLayout, OrnamentParams, OrnamentStyle, generate};

#[derive(Default)]
pub struct OrnamentDialog {
    pub params: OrnamentParams,
    preview: Option<(OrnamentParams, Vec<mml_core::geometry::Path>)>,
}

pub fn show(app: &mut MyApp, ctx: &egui::Context) {
    let Some(dialog) = &mut app.ornament else {
        return;
    };
    let mut open = true;
    let mut insert = false;
    egui::Window::new(tr("ornament.title"))
        .open(&mut open)
        .collapsible(false)
        .default_width(520.0)
        .show(ctx, |ui| {
            let params = &mut dialog.params;
            ui.horizontal_wrapped(|ui| {
                for style in OrnamentStyle::ALL {
                    ui.selectable_value(&mut params.style, style, tr(style.key()));
                }
            });
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut params.layout,
                    OrnamentLayout::Bands,
                    tr("ornament.bands"),
                );
                ui.selectable_value(
                    &mut params.layout,
                    OrnamentLayout::Tiles,
                    tr("ornament.tiles"),
                );
                ui.separator();
                ui.add(
                    DragValue::new(&mut params.width)
                        .range(10.0..=2000.0)
                        .prefix("W ")
                        .suffix(" mm"),
                );
                ui.add(
                    DragValue::new(&mut params.height)
                        .range(10.0..=2000.0)
                        .prefix("H ")
                        .suffix(" mm"),
                );
                ui.label(tr("ornament.cell"));
                ui.add(
                    DragValue::new(&mut params.cell)
                        .range(2.0..=200.0)
                        .suffix(" mm")
                        .speed(0.2),
                );
            });
            ui.horizontal(|ui| {
                if ui
                    .button(RichText::new(tr("ornament.roll")).strong())
                    .on_hover_text(tr("ornament.roll_hint"))
                    .clicked()
                {
                    let nanos = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |time| time.subsec_nanos() as u64);
                    params.seed = params
                        .seed
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(nanos | 1);
                }
                ui.label(RichText::new(format!("#{}", params.seed % 100_000)).weak())
                    .on_hover_text(tr("ornament.seed_hint"));
            });
            if dialog
                .preview
                .as_ref()
                .is_none_or(|(shown, _)| shown != params)
            {
                dialog.preview = Some((*params, generate(params)));
            }
            let paths = &dialog.preview.as_ref().unwrap().1;
            let available = ui.available_width();
            let scale = (available / params.width as f32).min(300.0 / params.height as f32);
            let size = egui::vec2(params.width as f32, params.height as f32) * scale;
            let (response, painter) = ui.allocate_painter(size, Sense::hover());
            let rect = response.rect;
            painter.rect_filled(rect, 6.0, ui.visuals().extreme_bg_color);
            let stroke = Stroke::new(1.2, crate::theme::ACCENT);
            for path in paths {
                let mut points: Vec<Pos2> = path
                    .points
                    .iter()
                    .map(|point| {
                        Pos2::new(
                            rect.left() + point.x as f32 * scale,
                            rect.bottom() - point.y as f32 * scale,
                        )
                    })
                    .collect();
                if path.closed
                    && let Some(first) = points.first().copied()
                {
                    points.push(first);
                }
                painter.add(egui::Shape::line(points, stroke));
            }
            insert = ui
                .button(RichText::new(tr("ornament.insert")).strong())
                .clicked();
        });
    if insert && let Some(dialog) = app.ornament.take() {
        let paths = generate(&dialog.params);
        let layer = app.layer_for(false);
        let name = format!(
            "{} #{}",
            tr(dialog.params.style.key()),
            dialog.params.seed % 100_000
        );
        app.add_shape(Shape::new(name, layer, ShapeKind::Paths { paths }));
        app.canvas.request_fit();
    } else if !open {
        app.ornament = None;
    }
}
