//! Quick laser setup for paper and board: pick the sheet and the laser,
//! each layer gets cut, perforated, marked or engraved settings.

use crate::app::{Level, MyApp};
use crate::assist::Content;
use crate::i18n::{tr, trf};
use eframe::egui::{self, Color32, DragValue, RichText};
use mml_core::layer::{FillParams, LayerId, LineParams, Operation};
use mml_materials::paper::{self, Recipe, Role, Sheet, Source};

pub struct PaperDialog {
    /// Role of each layer; `None` leaves the layer untouched.
    roles: Vec<(LayerId, Option<Role>)>,
}

impl PaperDialog {
    /// Roles guessed from what each layer holds: outlines are cut,
    /// strokes marked, lettering engraved; images are left alone.
    pub fn new(app: &MyApp) -> Self {
        let roles = app
            .document
            .layers
            .iter()
            .filter_map(|layer| {
                let role = match crate::assist::analyze(&app.document, layer.id)? {
                    Content::Outline => Some(Role::Cut),
                    Content::Strokes => Some(Role::Mark),
                    Content::Text => Some(Role::Engrave),
                    Content::Photo | Content::LineArt => None,
                };
                Some((layer.id, role))
            })
            .collect();
        Self { roles }
    }
}

fn summary(recipe: &Recipe) -> String {
    trf(
        "paper.summary",
        &[
            &format!("{:.0}", recipe.speed),
            &format!("{:.0}", recipe.power),
            &recipe.passes,
        ],
    )
}

pub fn show(app: &mut MyApp, ctx: &egui::Context) {
    let Some(dialog) = &mut app.paper else {
        return;
    };
    let mut open = true;
    let mut apply = false;
    let mut test_grid = false;
    let no_laser =
        app.machine.kind == mml_core::machine::MachineKind::Cnc && !app.machine.laser_mode;
    let setup = &mut app.settings.paper;
    egui::Window::new(tr("paper.window"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(460.0)
        .show(ctx, |ui| {
            if no_laser {
                ui.add(
                    egui::Label::new(
                        RichText::new(tr("paper.no_laser")).color(crate::theme::WARNING),
                    )
                    .wrap(),
                );
            }
            ui.horizontal_wrapped(|ui| {
                for sheet in Sheet::ALL {
                    ui.selectable_value(&mut setup.sheet, sheet, tr(sheet.key()));
                }
            });
            egui::Grid::new("paper-laser")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label(tr("paper.source"));
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut setup.source, Source::Diode, tr("paper.diode"));
                        ui.radio_value(&mut setup.source, Source::Co2, tr("paper.co2"));
                    });
                    ui.end_row();
                    ui.label(tr("paper.watts"))
                        .on_hover_text(tr("paper.watts_hint"));
                    ui.add(
                        DragValue::new(&mut setup.watts)
                            .range(0.5..=150.0)
                            .suffix(" W")
                            .speed(0.5),
                    );
                    ui.end_row();
                    ui.label(tr("paper.max_speed"));
                    ui.add(
                        DragValue::new(&mut setup.max_speed)
                            .range(300.0..=60_000.0)
                            .suffix(" mm/min")
                            .speed(50.0),
                    );
                    ui.end_row();
                });
            ui.separator();
            if dialog.roles.is_empty() {
                ui.label(RichText::new(tr("paper.no_layers")).weak());
            }
            for (layer_id, role) in &mut dialog.roles {
                let Some(layer) = app.document.layer(*layer_id) else {
                    continue;
                };
                let [r, g, b] = layer.color;
                ui.horizontal_wrapped(|ui| {
                    let (swatch, _) =
                        ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(swatch, 2.0, Color32::from_rgb(r, g, b));
                    ui.label(&layer.name);
                    egui::ComboBox::from_id_salt(("paper-role", *layer_id))
                        .selected_text(role.map_or(tr("paper.role_keep"), |role| tr(role.key())))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(role, None, tr("paper.role_keep"));
                            for candidate in Role::ALL {
                                ui.selectable_value(role, Some(candidate), tr(candidate.key()));
                            }
                        });
                    if let Some(role) = role {
                        ui.label(RichText::new(summary(&paper::recipe(setup, *role))).weak());
                    }
                });
            }
            ui.separator();
            for key in ["paper.tip_fire", "paper.tip_hold", "paper.tip_focus"] {
                ui.add(egui::Label::new(RichText::new(tr(key)).small()).wrap());
            }
            if setup.source == Source::Diode && setup.sheet == Sheet::Paper {
                ui.add(
                    egui::Label::new(
                        RichText::new(tr("paper.tip_white"))
                            .small()
                            .color(crate::theme::WARNING),
                    )
                    .wrap(),
                );
            }
            ui.add(egui::Label::new(RichText::new(tr("paper.tip_test")).small().weak()).wrap());
            ui.horizontal(|ui| {
                let any = dialog.roles.iter().any(|(_, role)| role.is_some());
                if ui
                    .add_enabled(
                        any,
                        egui::Button::new(RichText::new(tr("paper.apply")).strong()),
                    )
                    .clicked()
                {
                    apply = true;
                }
                if ui
                    .button(tr("paper.test_grid"))
                    .on_hover_text(tr("paper.test_grid_hint"))
                    .clicked()
                {
                    test_grid = true;
                }
            });
        });
    if apply {
        let setup = app.settings.paper;
        let roles = dialog.roles.clone();
        app.begin_edit("paper.title");
        let mut count = 0;
        for (layer_id, role) in roles {
            let (Some(role), Some(layer)) = (role, app.document.layer_mut(layer_id)) else {
                continue;
            };
            let recipe = paper::recipe(&setup, role);
            layer.operation = match role {
                Role::Engrave => Operation::LaserFill(FillParams {
                    speed: recipe.speed,
                    power: recipe.power,
                    passes: recipe.passes,
                    interval: recipe.interval,
                    air_assist: recipe.air_assist,
                    ..FillParams::default()
                }),
                _ => {
                    // Keep the bridges already set on a cut layer.
                    let previous = match &layer.operation {
                        Operation::LaserLine(params) => params.clone(),
                        _ => LineParams::default(),
                    };
                    let (dash_on, dash_off) = recipe.dash.unwrap_or((previous.dash_on, 0.0));
                    Operation::LaserLine(LineParams {
                        speed: recipe.speed,
                        power: recipe.power,
                        passes: recipe.passes,
                        air_assist: recipe.air_assist,
                        z_step: 0.0,
                        dash_on,
                        dash_off,
                        ..previous
                    })
                }
            };
            count += 1;
        }
        app.touch();
        app.notify(trf("paper.applied", &[&count]), Level::Info);
        app.paper = None;
        return;
    }
    if test_grid {
        let recipe = paper::recipe(&app.settings.paper, Role::Engrave);
        let max_speed = app.settings.paper.max_speed;
        app.makers = Some(crate::makers_dialog::MakersDialog::test_grid(
            mml_core::makers::test_grid::TestGridParams {
                power: (
                    (recipe.power * 0.5).max(1.0).round(),
                    (recipe.power * 1.5).min(100.0).round(),
                ),
                speed: (
                    (recipe.speed * 0.5).round(),
                    (recipe.speed * 1.5).min(max_speed).round(),
                ),
                ..Default::default()
            },
        ));
    }
    if !open {
        app.paper = None;
    }
}
