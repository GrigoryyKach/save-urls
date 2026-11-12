#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod i18n;
mod kit;
#[allow(dead_code)]
mod nimvoro;
mod search;
mod store;
mod strings;
// Shared with the other NIMVORO tools: some components are unused here.
#[allow(dead_code)]
mod theme;
mod urlx;
mod win;

use eframe::egui;

fn main() -> eframe::Result<()> {
    // A second launch just shows the window of the program that is already running.
    if !win::single_instance() {
        return Ok(());
    }
    let background = std::env::args().any(|a| a == "--background" || a == "--tray");
    let mut store = store::Store::load();
    // Starting language: library setting, then the NIMVORO marker next to the exe, then English. Persist the result.
    let lang = i18n::resolve(store.lib.settings.language.as_deref());
    strings::set_lang(lang);
    if store.lib.settings.language.as_deref() != Some(lang.code()) {
        store.lib.settings.language = Some(lang.code().to_string());
        store.touch();
        store.save_now();
    }
    let store = std::sync::Arc::new(std::sync::Mutex::new(store));

    let viewport = egui::ViewportBuilder::default()
        .with_title("Save URLs")
        .with_app_id("local.save-urls")
        .with_inner_size([1120.0, 740.0])
        .with_min_inner_size([820.0, 560.0])
        .with_icon(std::sync::Arc::new(app_icon()));
    let options = eframe::NativeOptions { viewport, centered: true, ..Default::default() };
    eframe::run_native("Save URLs", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, store, background)))))
}

/// Window icon from `assets/icon.png` (the same artwork that is embedded in the exe).
fn app_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")).unwrap_or_default()
}
