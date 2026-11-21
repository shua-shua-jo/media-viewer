//! Stand-in used when the app is built without the `video` feature.

use std::path::Path;

use eframe::egui;

/// Can never be constructed, so none of its methods can ever be called.
pub enum Player {}

impl Player {
    pub fn open(_path: &Path, _ctx: &egui::Context) -> Result<Player, String> {
        Err("this build has no embedded video playback".to_string())
    }

    pub fn path(&self) -> &Path {
        match *self {}
    }

    pub fn show(&mut self, _ui: &mut egui::Ui) {
        match *self {}
    }

    pub fn toggle_pause(&mut self) {
        match *self {}
    }
}
