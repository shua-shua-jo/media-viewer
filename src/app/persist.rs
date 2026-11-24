//! Saving settings (last folder, filter, sort order, window size) while the app runs.

use std::time::{Duration, Instant};

use eframe::egui;

use super::App;
use crate::settings::Settings;

impl App {
    /// Called every frame. Writes the settings file when something changed, at most about once a
    /// second (so dragging the window edge doesn't write constantly), and immediately on close.
    pub(super) fn persist_settings(&mut self, ctx: &egui::Context) {
        let now = self.settings_snapshot(ctx);
        if now == self.saved_settings {
            return;
        }

        let closing = ctx.input(|i| i.viewport().close_requested());
        if !closing && self.last_settings_save.elapsed() < Duration::from_secs(1) {
            // Changed again very recently: check back shortly, even if nothing else happens.
            ctx.request_repaint_after(Duration::from_millis(500));
            return;
        }

        if let Err(e) = now.save() {
            self.status = format!("Could not save settings: {e}");
        }
        // Remember it even if saving failed, so a read-only config folder doesn't retry every second.
        self.saved_settings = now;
        self.last_settings_save = Instant::now();
    }

    fn settings_snapshot(&self, ctx: &egui::Context) -> Settings {
        let (rect, maximized, fullscreen, minimized) = ctx.input(|i| {
            let v = i.viewport();
            (
                v.inner_rect,
                v.maximized.unwrap_or(false),
                v.fullscreen.unwrap_or(false),
                v.minimized.unwrap_or(false),
            )
        });

        // Keep the previous size while maximized, fullscreen or minimized, so "restore" goes back to it.
        let window_size = match rect {
            Some(r) if !maximized && !fullscreen && !minimized => {
                Some((r.width().round() as u32, r.height().round() as u32))
            }
            _ => self.saved_settings.window_size,
        };

        Settings {
            last_folder: self.folder.clone(),
            filter: self.filter,
            sort: self.sort,
            window_size,
            maximized,
        }
    }
}
