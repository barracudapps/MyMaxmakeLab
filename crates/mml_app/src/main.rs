#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod assist;
mod camera_panel;
mod canvas;
mod firmware;
mod i18n;
mod icons;
mod machine_panel;
mod pairing;
mod pairing_dialog;
mod panels;
mod preview;
mod relief_dialog;
mod settings;
mod sprue_dialog;
mod text_tools;
mod theme;
mod tools_dialog;
mod view3d;
mod workflow;

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("MyMaxmakeLab")
            .with_app_id("mymaxmakelab")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([980.0, 620.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "MyMaxmakeLab",
        options,
        Box::new(|creation| Ok(Box::new(app::MyApp::new(creation)))),
    )
}
