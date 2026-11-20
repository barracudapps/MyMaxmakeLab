//! Settings assistant: looks at what a layer contains and proposes the
//! operation and its parameters, from the material library when it has
//! a matching preset, from cautious defaults otherwise.

use crate::app::MyApp;
use crate::i18n::{tr, trf};
use eframe::egui::{self, RichText};
use mml_core::document::{Document, ShapeKind};
use mml_core::layer::{
    CncProfileParams, DitherMode, FillParams, ImageParams, LayerId, LineParams, Operation,
    ProfileSide,
};
use mml_core::machine::MachineKind;
use mml_materials::Library;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Content {
    Photo,
    LineArt,
    Text,
    Outline,
    Strokes,
}

impl Content {
    fn key(self) -> &'static str {
        match self {
            Self::Photo => "assist.photo",
            Self::LineArt => "assist.line_art",
            Self::Text => "assist.text",
            Self::Outline => "assist.outline",
            Self::Strokes => "assist.strokes",
        }
    }
}

/// Photo when many pixels are mid-tones, line art when the histogram is
/// mostly black and white; for vectors, many small closed shapes look
/// like lettering (fill), large closed shapes like parts (cut), open
/// paths like drawings (line engraving).
pub fn analyze(document: &Document, layer: LayerId) -> Option<Content> {
    let shapes: Vec<_> = document.shapes_on(layer).collect();
    if shapes.is_empty() {
        return None;
    }
    let mut mid_tones = 0usize;
    let mut pixels = 0usize;
    for shape in &shapes {
        if let ShapeKind::Image { image, .. } = &shape.kind {
            pixels += image.pixels.len();
            mid_tones += image
                .pixels
                .iter()
                .filter(|value| (40..=215).contains(*value))
                .count();
        }
    }
    if pixels > 0 {
        return Some(if mid_tones as f64 / pixels as f64 > 0.3 {
            Content::Photo
        } else {
            Content::LineArt
        });
    }
    let paths: Vec<_> = shapes
        .iter()
        .flat_map(|shape| shape.world_paths())
        .collect();
    let open = paths.iter().filter(|path| !path.closed).count();
    if open * 2 > paths.len() {
        return Some(Content::Strokes);
    }
    let small = paths
        .iter()
        .filter(|path| path.closed)
        .filter(|path| {
            let bounds = path.bounds();
            bounds.width().max(bounds.height()) < 15.0
        })
        .count();
    Some(if small >= 5 && small * 2 >= paths.len() {
        Content::Text
    } else {
        Content::Outline
    })
}

fn same_variant(a: &Operation, b: &Operation) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

/// Proposed operation and whether it comes from the library.
pub fn suggest(
    content: Content,
    machine: MachineKind,
    material: Option<&str>,
    thickness: Option<f64>,
    library: &Library,
) -> (Operation, bool) {
    let default = match (content, machine) {
        (Content::Photo | Content::LineArt, MachineKind::Cnc) => {
            Operation::CncVCarve(mml_core::layer::VCarveParams {
                max_depth: 1.0,
                line_spacing: 0.2,
                ..Default::default()
            })
        }
        (Content::Photo, _) => Operation::LaserImage(ImageParams {
            dither: DitherMode::Jarvis,
            dpi: 254.0,
            power_max: 40.0,
            speed: 6000.0,
            ..ImageParams::default()
        }),
        (Content::LineArt, _) => Operation::LaserImage(ImageParams {
            dither: DitherMode::Threshold,
            dpi: 318.0,
            power_max: 40.0,
            speed: 6000.0,
            ..ImageParams::default()
        }),
        (Content::Text, MachineKind::Laser) => Operation::LaserFill(FillParams {
            power: 30.0,
            speed: 6000.0,
            interval: 0.1,
            ..FillParams::default()
        }),
        (Content::Strokes, MachineKind::Laser) => Operation::LaserLine(LineParams {
            power: 30.0,
            speed: 1800.0,
            air_assist: false,
            ..LineParams::default()
        }),
        (Content::Outline, MachineKind::Laser) => Operation::LaserLine(LineParams {
            power: 90.0,
            speed: 300.0,
            passes: 2,
            ..LineParams::default()
        }),
        (Content::Text, MachineKind::Cnc) => {
            Operation::CncVCarve(mml_core::layer::VCarveParams::default())
        }
        (Content::Strokes, MachineKind::Cnc) => Operation::CncProfile(CncProfileParams {
            side: ProfileSide::On,
            depth: 0.5,
            ..CncProfileParams::default()
        }),
        (Content::Outline, MachineKind::Cnc) => Operation::CncProfile(CncProfileParams {
            side: ProfileSide::Outside,
            depth: thickness.unwrap_or(3.0),
            tabs: 4,
            ..CncProfileParams::default()
        }),
    };
    let wants_cut = content == Content::Outline;
    let found = library
        .presets
        .iter()
        .filter(|preset| {
            material.is_none_or(|material| preset.material.eq_ignore_ascii_case(material))
        })
        .filter(|preset| same_variant(&preset.operation, &default))
        .filter(|preset| {
            if !matches!(default, Operation::LaserLine(_)) {
                return true;
            }
            let is_cut = preset.title.to_lowercase().contains("cut");
            is_cut == wants_cut
        })
        .min_by(|a, b| {
            let distance = |thickness_of: Option<f64>| match (thickness, thickness_of) {
                (Some(wanted), Some(value)) => (wanted - value).abs(),
                (_, None) => 50.0,
                (None, Some(_)) => 100.0,
            };
            distance(a.thickness).total_cmp(&distance(b.thickness))
        });
    match found {
        Some(preset) => (preset.operation.clone(), true),
        None => (default, false),
    }
}

pub struct AssistDialog {
    pub layer: LayerId,
    pub content: Option<Content>,
    pub material: Option<String>,
    pub thickness: f64,
}

pub fn show(app: &mut MyApp, ctx: &egui::Context) {
    let Some(dialog) = &mut app.assist else {
        return;
    };
    let mut open = true;
    let mut apply = None;
    let materials: Vec<String> = app
        .library
        .materials()
        .into_iter()
        .map(str::to_owned)
        .collect();
    egui::Window::new(tr("assist.title"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            let Some(content) = dialog.content else {
                ui.label(tr("assist.empty"));
                return;
            };
            ui.label(RichText::new(trf("assist.detected", &[&tr(content.key())])).strong());
            ui.horizontal(|ui| {
                ui.label(tr("materials.material"));
                let selected = dialog
                    .material
                    .clone()
                    .unwrap_or_else(|| tr("assist.any_material").to_owned());
                egui::ComboBox::from_id_salt("assist-material")
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut dialog.material, None, tr("assist.any_material"));
                        for material in &materials {
                            ui.selectable_value(
                                &mut dialog.material,
                                Some(material.clone()),
                                material,
                            );
                        }
                    });
                ui.label(tr("materials.thickness"));
                ui.add(
                    egui::DragValue::new(&mut dialog.thickness)
                        .range(0.0..=100.0)
                        .suffix(" mm")
                        .speed(0.1),
                );
            });
            let thickness = (dialog.thickness > 0.0).then_some(dialog.thickness);
            let (operation, from_library) = suggest(
                content,
                app.machine.kind,
                dialog.material.as_deref(),
                thickness,
                &app.library,
            );
            ui.label(trf("assist.proposal", &[&tr(operation.kind_key())]));
            ui.label(
                RichText::new(if from_library {
                    tr("assist.from_library")
                } else {
                    tr("assist.from_defaults")
                })
                .weak(),
            );
            if ui
                .button(RichText::new(tr("assist.apply")).strong())
                .clicked()
            {
                apply = Some(operation);
            }
        });
    if let Some(operation) = apply {
        let layer = app.assist.as_ref().map(|dialog| dialog.layer);
        app.assist = None;
        if let Some(layer) = layer.and_then(|id| app.document.layer_mut(id)) {
            layer.operation = operation;
            app.touch();
        }
    } else if !open {
        app.assist = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mml_core::document::{RasterImage, Shape};
    use std::sync::Arc;

    #[test]
    fn photos_and_line_art_are_told_apart() {
        let mut document = Document::default();
        let layer = document.layers[0].id;
        let gradient = RasterImage {
            width: 256,
            height: 1,
            pixels: (0..=255).collect(),
        };
        document.shapes.push(Shape::new(
            "photo",
            layer,
            ShapeKind::Image {
                image: Arc::new(gradient),
                width: 10.0,
                height: 1.0,
            },
        ));
        assert_eq!(analyze(&document, layer), Some(Content::Photo));
        let bitmap = RasterImage {
            width: 4,
            height: 1,
            pixels: vec![0, 255, 0, 255],
        };
        document.shapes[0].kind = ShapeKind::Image {
            image: Arc::new(bitmap),
            width: 4.0,
            height: 1.0,
        };
        assert_eq!(analyze(&document, layer), Some(Content::LineArt));
    }

    #[test]
    fn library_preset_for_the_material_wins_over_defaults() {
        let mut library = Library::default();
        let cut = Operation::LaserLine(LineParams {
            power: 100.0,
            speed: 720.0,
            ..LineParams::default()
        });
        library.presets.push(mml_materials::Preset::new(
            "Basswood Board",
            Some(3.0),
            "Cut",
            cut.clone(),
        ));
        let (operation, from_library) = suggest(
            Content::Outline,
            MachineKind::Laser,
            Some("Basswood Board"),
            Some(3.0),
            &library,
        );
        assert!(from_library);
        assert_eq!(operation, cut);
        let (_, from_library) = suggest(
            Content::Strokes,
            MachineKind::Laser,
            Some("Basswood Board"),
            None,
            &library,
        );
        assert!(
            !from_library,
            "a cut preset must not be proposed for engraving"
        );
    }
}
