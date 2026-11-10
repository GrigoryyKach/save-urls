#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod icon;
mod kit;
mod nimvoro;
mod search;
mod store;
// Общий с Random Wheel модуль оформления: часть компонентов здесь не используется.
#[allow(dead_code)]
mod theme;
mod urlx;
mod win;

use eframe::egui;

fn main() -> eframe::Result<()> {
    // Второй запуск просто показывает окно уже работающей программы.
    if !win::single_instance() {
        return Ok(());
    }
    let background = std::env::args().any(|a| a == "--background" || a == "--tray");
    let store = std::sync::Arc::new(std::sync::Mutex::new(store::Store::load()));

    let viewport = egui::ViewportBuilder::default()
        .with_title("Save URLs")
        .with_app_id("local.save-urls")
        .with_inner_size([1120.0, 740.0])
        .with_min_inner_size([820.0, 560.0])
        .with_icon(std::sync::Arc::new(egui::IconData { rgba: icon::icon_rgba(64), width: 64, height: 64 }));
    let options = eframe::NativeOptions { viewport, centered: true, ..Default::default() };
    eframe::run_native("Save URLs", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, store, background)))))
}
