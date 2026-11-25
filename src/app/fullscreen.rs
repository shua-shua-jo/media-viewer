//! Fullscreen mode: hides the toolbar and status bar so the photo fills the screen.

use std::time::{Duration, Instant};

use eframe::egui;

use super::App;

/// How long the "how to exit" hint stays on screen after entering fullscreen.
const HINT_SECONDS: u64 = 4;

impl App {
    pub(super) fn toggle_fullscreen(&mut self, ctx: &egui::Context) {
        let on = !self.fullscreen;
        self.set_fullscreen(ctx, on);
    }

    pub(super) fn set_fullscreen(&mut self, ctx: &egui::Context, on: bool) {
        self.fullscreen = on;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(on));
        if on {
            // The number box is hidden in fullscreen, so it can't keep the keyboard.
            self.goto_focused = false;
            self.fullscreen_hint_until = Some(Instant::now() + Duration::from_secs(HINT_SECONDS));
            ctx.request_repaint_after(Duration::from_secs(HINT_SECONDS));
        } else {
            self.fullscreen_hint_until = None;
        }
    }

    /// Shows a short hint after entering fullscreen, because the toolbar is gone.
    pub(super) fn draw_fullscreen_hint(&mut self, ui: &egui::Ui) {
        let Some(until) = self.fullscreen_hint_until else {
            return;
        };
        if !self.fullscreen || Instant::now() >= until {
            self.fullscreen_hint_until = None;
            return;
        }

        let rect = ui.max_rect();
        let pos = egui::pos2(rect.center().x, rect.bottom() - 32.0);
        let font = egui::FontId::proportional(18.0);
        let text =
            "Esc or F11: exit fullscreen   |   Left / Right: previous / next   |   S: slideshow";
        let painter = ui.painter();
        // A black copy underneath keeps the white text readable on bright photos.
        painter.text(
            pos + egui::vec2(1.0, 1.0),
            egui::Align2::CENTER_BOTTOM,
            text,
            font.clone(),
            egui::Color32::BLACK,
        );
        painter.text(
            pos,
            egui::Align2::CENTER_BOTTOM,
            text,
            font,
            egui::Color32::WHITE,
        );
    }
}
