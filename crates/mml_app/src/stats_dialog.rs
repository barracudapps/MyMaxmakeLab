//! Workshop dashboard: what ran, for how long, on which machine and
//! with which tool, how worn the tools are and what it all cost.

use crate::app::MyApp;
use crate::i18n::{tr, trf};
use eframe::egui::{self, Color32, DragValue, RichText, Sense, Stroke};
use mml_core::stats::{ToolCost, summarize};

fn kpi(ui: &mut egui::Ui, title: &str, value: String) {
    egui::Frame::group(ui.style())
        .corner_radius(10.0)
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.set_min_width(130.0);
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                ui.label(RichText::new(title).weak());
                ui.label(RichText::new(value).size(22.0).strong());
            });
        });
}

fn bar(ui: &mut egui::Ui, share: f32, color: Color32, width: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 12.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, 6.0, ui.visuals().extreme_bg_color);
    let mut filled = rect;
    filled.set_width(rect.width() * share.clamp(0.0, 1.0));
    ui.painter().rect_filled(filled, 6.0, color);
}

fn hours(value: f64) -> String {
    if value < 1.0 {
        format!("{:.0} min", value * 60.0)
    } else {
        format!("{value:.1} h")
    }
}

pub fn show(app: &mut MyApp, ctx: &egui::Context) {
    if !app.stats_open {
        return;
    }
    let mut open = true;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.as_secs());
    egui::Window::new(tr("stats.title"))
        .open(&mut open)
        .default_width(760.0)
        .default_height(700.0)
        .vscroll(true)
        .show(ctx, |ui| {
            let costs = app.settings.costs.clone();
            let currency = costs.currency.clone();
            let summary = summarize(&app.usage, &costs, 30, now);
            if summary.jobs == 0 {
                ui.label(RichText::new(tr("stats.empty")).weak());
            }
            ui.horizontal_wrapped(|ui| {
                kpi(
                    ui,
                    tr("stats.jobs"),
                    format!("{} / {}", summary.completed, summary.jobs),
                );
                kpi(ui, tr("stats.hours"), hours(summary.hours));
                kpi(
                    ui,
                    tr("stats.cost"),
                    format!("{:.2} {currency}", summary.cost),
                );
                let per_hour = if summary.hours > 0.0 {
                    summary.cost / summary.hours
                } else {
                    0.0
                };
                kpi(
                    ui,
                    tr("stats.per_hour"),
                    format!("{per_hour:.2} {currency}/h"),
                );
            });
            ui.add_space(8.0);
            ui.label(RichText::new(tr("stats.last_days")).strong());
            let peak = summary.days.iter().copied().fold(0.0f64, f64::max).max(0.1);
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), 90.0), Sense::hover());
            let width = rect.width() / summary.days.len() as f32;
            for (index, day) in summary.days.iter().enumerate() {
                let height = (*day / peak) as f32 * (rect.height() - 4.0);
                let column = egui::Rect::from_min_max(
                    egui::pos2(
                        rect.left() + index as f32 * width + 1.0,
                        rect.bottom() - height,
                    ),
                    egui::pos2(
                        rect.left() + (index + 1) as f32 * width - 1.0,
                        rect.bottom(),
                    ),
                );
                ui.painter().rect_filled(column, 2.0, crate::theme::ACCENT);
            }
            ui.painter().line_segment(
                [rect.left_bottom(), rect.right_bottom()],
                Stroke::new(1.0, ui.visuals().weak_text_color()),
            );
            ui.add_space(8.0);
            ui.columns(2, |columns| {
                columns[0].label(RichText::new(tr("stats.machines")).strong());
                let most = summary
                    .machines
                    .first()
                    .map_or(1.0, |(_, hours)| hours.max(0.01));
                for (name, worked) in &summary.machines {
                    columns[0].horizontal(|ui| {
                        ui.label(name);
                        bar(ui, (*worked / most) as f32, crate::theme::ACCENT, 90.0);
                        ui.label(RichText::new(hours(*worked)).weak());
                    });
                }
                columns[1].label(RichText::new(tr("stats.tools")).strong());
                for (label, worked, worn) in &summary.tools {
                    columns[1].horizontal(|ui| {
                        ui.label(label);
                        match worn {
                            Some(share) => {
                                let color = if *share > 0.9 {
                                    crate::theme::DANGER
                                } else if *share > 0.6 {
                                    crate::theme::WARNING
                                } else {
                                    crate::theme::SUCCESS
                                };
                                bar(ui, *share as f32, color, 70.0);
                                ui.label(
                                    RichText::new(format!(
                                        "{} · {:.0} %",
                                        hours(*worked),
                                        share * 100.0
                                    ))
                                    .weak(),
                                );
                                if *share > 0.9 {
                                    ui.label(
                                        RichText::new(tr("stats.replace"))
                                            .color(crate::theme::DANGER),
                                    );
                                }
                            }
                            None => {
                                ui.label(RichText::new(hours(*worked)).weak());
                            }
                        }
                    });
                }
            });
            ui.add_space(8.0);
            egui::CollapsingHeader::new(RichText::new(tr("stats.costs")).strong()).show(ui, |ui| {
                let costs = &mut app.settings.costs;
                egui::Grid::new("costs")
                    .num_columns(2)
                    .spacing([10.0, 5.0])
                    .show(ui, |ui| {
                        ui.label(tr("stats.currency"));
                        ui.add(egui::TextEdit::singleline(&mut costs.currency).desired_width(40.0));
                        ui.end_row();
                        ui.label(tr("stats.machine_price"))
                            .on_hover_text(tr("stats.machine_price_hint"));
                        ui.horizontal(|ui| {
                            ui.add(
                                DragValue::new(&mut costs.machine_price)
                                    .range(0.0..=1e6)
                                    .speed(10.0),
                            );
                            ui.label(tr("stats.over"));
                            ui.add(
                                DragValue::new(&mut costs.machine_life_hours)
                                    .range(1.0..=1e6)
                                    .suffix(" h")
                                    .speed(10.0),
                            );
                        });
                        ui.end_row();
                        ui.label(tr("stats.power"));
                        ui.add(
                            DragValue::new(&mut costs.power_watts)
                                .range(0.0..=20_000.0)
                                .suffix(" W")
                                .speed(5.0),
                        );
                        ui.end_row();
                        ui.label(tr("stats.electricity"));
                        ui.add(
                            DragValue::new(&mut costs.electricity_per_kwh)
                                .range(0.0..=5.0)
                                .suffix("/kWh")
                                .speed(0.01),
                        );
                        ui.end_row();
                        ui.label(tr("stats.consumables"))
                            .on_hover_text(tr("stats.consumables_hint"));
                        ui.add(
                            DragValue::new(&mut costs.consumables_per_hour)
                                .range(0.0..=1000.0)
                                .suffix("/h")
                                .speed(0.05),
                        );
                        ui.end_row();
                        ui.label(tr("stats.labour"));
                        ui.add(
                            DragValue::new(&mut costs.labour_per_hour)
                                .range(0.0..=1000.0)
                                .suffix("/h")
                                .speed(0.5),
                        );
                        ui.end_row();
                    });
                ui.label(RichText::new(tr("stats.tool_costs")).strong())
                    .on_hover_text(tr("stats.tool_costs_hint"));
                // Every tool seen in the history can be priced.
                for (label, ..) in &summary.tools {
                    if costs.tool(label).is_none() {
                        costs.tools.push(ToolCost {
                            label: label.clone(),
                            price: 0.0,
                            life_hours: 0.0,
                        });
                    }
                }
                egui::Grid::new("tool-costs").num_columns(3).show(ui, |ui| {
                    for tool in &mut costs.tools {
                        ui.label(&tool.label);
                        ui.add(
                            DragValue::new(&mut tool.price)
                                .range(0.0..=10_000.0)
                                .prefix(tr("stats.price"))
                                .speed(0.5),
                        );
                        ui.add(
                            DragValue::new(&mut tool.life_hours)
                                .range(0.0..=100_000.0)
                                .prefix(tr("stats.life"))
                                .suffix(" h")
                                .speed(0.5),
                        );
                        ui.end_row();
                    }
                });
            });
            ui.label(
                RichText::new(trf("stats.records", &[&app.usage.len()]))
                    .weak()
                    .small(),
            );
        });
    app.stats_open = open;
}
