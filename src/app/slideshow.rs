//! Slideshow: automatically moves to the next photo every few seconds. Videos are skipped.

use std::time::{Duration, Instant};

use eframe::egui;

use super::App;
use crate::media::Kind;

impl App {
    pub(super) fn toggle_slideshow(&mut self) {
        if self.slideshow {
            self.slideshow = false;
            self.status = "Slideshow stopped.".into();
            return;
        }
        if !self.items.iter().any(|m| m.kind == Kind::Image) {
            self.status = "Nothing to show: the slideshow only plays photos.".into();
            return;
        }

        self.slideshow = true;
        // Starting on a video? Go straight to the next photo.
        if self.items.get(self.index).map(|m| m.kind) != Some(Kind::Image) {
            self.advance_slideshow();
        }
        self.slideshow_seen_index = self.index;
        self.restart_slideshow_timer();
        self.status = format!("Slideshow started (every {} s).", self.slideshow_secs);
    }

    pub(super) fn restart_slideshow_timer(&mut self) {
        self.slideshow_next = Instant::now() + Duration::from_secs(self.slideshow_secs as u64);
    }

    /// Moves to the next photo, skipping videos and wrapping around at the end.
    /// Stops the slideshow if there is no photo to move to.
    pub(super) fn advance_slideshow(&mut self) {
        let n = self.items.len();
        for step in 1..=n {
            let i = (self.index + step) % n;
            if self.items[i].kind == Kind::Image {
                self.index = i;
                return;
            }
        }
        self.slideshow = false;
    }

    /// Called every frame while the app is running.
    pub(super) fn tick_slideshow(&mut self, ctx: &egui::Context) {
        if !self.slideshow {
            return;
        }
        if self.items.is_empty() {
            self.slideshow = false;
            return;
        }

        // If the user moved to another photo themselves, give that photo a full interval.
        if self.index != self.slideshow_seen_index {
            self.slideshow_seen_index = self.index;
            self.restart_slideshow_timer();
        }

        if Instant::now() >= self.slideshow_next {
            self.advance_slideshow();
            self.slideshow_seen_index = self.index;
            self.restart_slideshow_timer();
        }

        // Wake up again when the next photo is due, even if nothing else happens.
        let wait = self
            .slideshow_next
            .saturating_duration_since(Instant::now())
            .max(Duration::from_millis(50));
        ctx.request_repaint_after(wait);
    }
}
