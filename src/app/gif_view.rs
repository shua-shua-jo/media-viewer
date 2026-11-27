//! Keeps an animated-GIF player running for the current file when it is a GIF.

use eframe::egui;

use super::App;
use crate::animation::{AnimPlayer, is_gif};
use crate::media::{Kind, Media};

impl App {
    /// Called every frame: starts, replaces or stops the GIF player to match the current file,
    /// then lets it advance. The still first frame (from the normal image loader) is shown until
    /// the player has its first frame, so there is never a blank gap.
    pub(super) fn update_animation(&mut self, ctx: &egui::Context, current: &Option<Media>) {
        let want = current
            .as_ref()
            .filter(|m| m.kind == Kind::Image && is_gif(&m.path))
            .map(|m| self.key_for(m));

        match want {
            None => self.anim = None, // not on a GIF: dropping the player stops its thread
            Some((path, rotation)) => {
                let already = self
                    .anim
                    .as_ref()
                    .map(|a| a.is_for(&path, rotation))
                    .unwrap_or(false);
                if !already {
                    self.anim = Some(AnimPlayer::open(&path, rotation, ctx));
                }
                if let Some(player) = &mut self.anim {
                    player.pump(ctx);
                }
            }
        }
    }
}
