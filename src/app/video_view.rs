//! Shows the current video: the embedded player when available, otherwise a button that opens
//! the system's default player.

use eframe::egui;

use super::App;
use crate::media::Media;
use crate::video::Player;

impl App {
    pub(super) fn show_video(&mut self, ui: &mut egui::Ui, media: &Media) {
        // Start (or switch) the embedded player for this file.
        let wrong_player = self
            .player
            .as_ref()
            .map(|p| p.path() != media.path.as_path())
            .unwrap_or(true);
        if wrong_player && self.player_failed.as_ref() != Some(&media.path) {
            self.player = None; // dropping the old player stops its decoder thread
            match Player::open(&media.path, ui.ctx()) {
                Ok(p) => self.player = Some(p),
                Err(reason) => {
                    self.player_failed = Some(media.path.clone());
                    self.status = format!("Embedded playback unavailable: {reason}");
                }
            }
        }

        if let Some(player) = &mut self.player {
            player.show(ui);
            return;
        }

        // Fallback: no embedded player in this build (or it could not start).
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() / 3.0);
            ui.heading("🎬 Video");
            if ui.button("▶ Play in default player").clicked()
                && let Err(e) = open::that(&media.path)
            {
                self.status = format!("Could not open video: {e}");
            }
        });
    }
}
