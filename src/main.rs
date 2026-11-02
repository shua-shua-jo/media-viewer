#![windows_subsystem = "windows"]

mod app;
mod config;
mod loader;
mod media;
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

    eframe::run_native(
        "Media Viewer",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_icon(load_icon())
                .with_min_inner_size([MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT]),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(App::new(initial, cc.egui_ctx.clone())))),
    )
}
