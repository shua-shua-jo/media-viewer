#![cfg_attr(all(not(debug_assertions), not(test)), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

mod app;
mod config;
mod loader;
mod media;
mod recycle;
mod settings;
#[cfg(test)]
mod tests;
mod video;
mod widgets;

use std::path::PathBuf;

use app::App;
use config::{MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH};
use eframe::egui;

fn load_icon() -> egui::IconData {
    let img = image::load_from_memory(include_bytes!("../assets/icon.png"))
        .expect("could not read icon.png")
        .into_rgba8();
    let (width, height) = img.dimensions();
    egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    }
}

fn main() -> eframe::Result {
    // Optional: pass a folder on the command line, e.g. `cargo run --release -- C:\Photos`
    let initial = std::env::args().nth(1).map(PathBuf::from);

    let settings = settings::Settings::load();

    let mut viewport = egui::ViewportBuilder::default()
        .with_icon(load_icon())
        .with_min_inner_size([MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT]);
    if let Some((w, h)) = settings.window_size {
        viewport = viewport.with_inner_size([
            (w as f32).max(MIN_WINDOW_WIDTH),
            (h as f32).max(MIN_WINDOW_HEIGHT),
        ]);
    }
    if settings.maximized {
        viewport = viewport.with_maximized(true);
    }

    eframe::run_native(
        "Media Viewer",
        eframe::NativeOptions {
            viewport,
            ..Default::default()
        },
        Box::new(move |cc| Ok(Box::new(App::new(initial, cc.egui_ctx.clone(), settings)))),
    )
}
