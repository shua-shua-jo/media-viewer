//! Background loading: decides which images to keep in memory, uploads finished
//! decodes as textures, and reports the state of the current image.

use eframe::egui;

use super::App;
use crate::config::{PRELOAD_AHEAD, PRELOAD_BEHIND};
use crate::loader::Key;
use crate::media::{Kind, Media};

/// What the UI needs to know about the current image this frame.
pub(super) struct ImageState {
    /// The current image's texture, if it has finished loading.
    pub tex: Option<egui::TextureHandle>,
    /// True if decoding the current image failed.
    pub failed: bool,
    /// True while the current image is still being decoded.
    pub loading: bool,
}

impl App {
    /// Per-frame entry point: collect finished decodes, re-plan the preload window,
    /// and report the state of the current image.
    pub(super) fn sync_images(
        &mut self,
        ctx: &egui::Context,
        current: &Option<Media>,
    ) -> ImageState {
        let current_key: Option<Key> = current
            .as_ref()
            .filter(|m| m.kind == Kind::Image)
            .map(|m| self.key_for(m));

        self.pump_loader(ctx, current_key.as_ref());
        self.update_wanted();

        let tex = current_key
            .as_ref()
            .and_then(|k| self.cache.get(k).cloned());
        let failed = current_key
            .as_ref()
            .map(|k| self.failed.contains(k))
            .unwrap_or(false);

        match (&tex, current) {
            (Some(t), _) => self.shown = Some(t.clone()),
            (None, None) => self.shown = None,
            (None, Some(m)) if m.kind == Kind::Video => self.shown = None,
            _ => {} // keep showing the previous image while this one loads
        }

        let loading = current_key.is_some() && tex.is_none() && !failed;
        ImageState {
            tex,
            failed,
            loading,
        }
    }

    /// The cache key for a file: its path plus its current view-only rotation.
    fn key_for(&self, media: &Media) -> Key {
        (
            media.path.clone(),
            self.rotations.get(&media.path).copied().unwrap_or(0),
        )
    }

    /// Collects finished decodes from the background threads and uploads them as textures.
    /// At most one upload per frame, so several large images arriving together can't cause a hitch.
    fn pump_loader(&mut self, ctx: &egui::Context, current_key: Option<&Key>) {
        while let Some(loaded) = self.loader.try_recv() {
            if !self.last_wanted.contains(&loaded.key) {
                continue; // we've moved on; drop it
            }
            match loaded.result {
                Ok(color) => {
                    let name = loaded.key.0.to_string_lossy().to_string();
                    let tex = ctx.load_texture(name, color, Default::default());
                    self.cache.insert(loaded.key, tex);
                    ctx.request_repaint(); // handle any remaining results next frame
                    break;
                }
                Err(e) => {
                    if Some(&loaded.key) == current_key {
                        self.status = format!("Could not load image: {e}");
                    }
                    self.failed.insert(loaded.key);
                }
            }
        }
    }

    /// Works out which images should be in memory (current + a few neighbors), frees the rest,
    /// and tells the loader what to decode next. Does nothing if the set hasn't changed.
    fn update_wanted(&mut self) {
        let n = self.items.len() as isize;
        let mut offsets: Vec<isize> = vec![0];
        for k in 1..=PRELOAD_AHEAD.max(PRELOAD_BEHIND) {
            if k <= PRELOAD_AHEAD {
                offsets.push(k as isize);
            }
            if k <= PRELOAD_BEHIND {
                offsets.push(-(k as isize));
            }
        }

        let mut wanted: Vec<Key> = Vec::new();
        if n > 0 {
            for off in offsets {
                let i = (self.index as isize + off).rem_euclid(n) as usize;
                let m = &self.items[i];
                if m.kind != Kind::Image {
                    continue;
                }
                let key = self.key_for(m);
                if !wanted.contains(&key) {
                    wanted.push(key);
                }
            }
        }

        if wanted == self.last_wanted {
            return;
        }

        // Free anything outside the window, then queue what's missing.
        self.cache.retain(|k, _| wanted.contains(k));
        self.failed.retain(|k| wanted.contains(k));
        let cache = &self.cache;
        let failed = &self.failed;
        self.loader
            .set_wanted(&wanted, |k| cache.contains_key(k) || failed.contains(k));
        self.last_wanted = wanted;
    }
}
