//! Things the user can do to the current file: favorite, delete, rotate (view only), zoom.

use std::fs;
use std::path::PathBuf;

use eframe::egui;

use super::App;
use crate::config::{FAV_DIR, MAX_ZOOM, MIN_ZOOM};
use crate::media::{Kind, Media};
use crate::recycle;

impl App {
    /// Where the favorite copy of `media` lives: <folder>/Favorites/<path relative to the folder>.
    /// (Files in subfolders keep their subfolder, so two photos with the same name can't collide.)
    pub(super) fn fav_path(&self, media: &Media) -> Option<PathBuf> {
        let folder = self.folder.as_ref()?;
        Some(folder.join(FAV_DIR).join(self.rel_path(&media.path)))
    }

    pub(super) fn is_favorite(&self, media: &Media) -> bool {
        self.favs.contains(self.rel_path(&media.path))
    }

    /// Favoriting copies the file into <folder>/Favorites; unfavoriting removes that copy.
    /// The original is never touched. (Swap `fs::copy` for `fs::rename` if you prefer a move.)
    pub(super) fn toggle_favorite(&mut self) {
        let Some(media) = self.items.get(self.index).cloned() else {
            return;
        };
        let Some(dest) = self.fav_path(&media) else {
            return;
        };

        let was_fav = dest.exists();
        let result = if was_fav {
            fs::remove_file(&dest).map(|_| "Removed from favorites.".to_string())
        } else {
            dest.parent()
                .map(fs::create_dir_all)
                .transpose()
                .and_then(|_| fs::copy(&media.path, &dest))
                .map(|_| "Added to favorites.".to_string())
        };

        match result {
            Ok(msg) => {
                let rel = self.rel_path(&media.path).to_path_buf();
                if was_fav {
                    self.favs.remove(&rel);
                } else {
                    self.favs.insert(rel);
                }
                self.status = msg;
                // In a filtered view the item may now need to disappear from the list.
                self.apply_filter(Some(media.path.clone()));
            }
            Err(e) => self.status = format!("Favorite failed: {e}"),
        }
    }

    /// Sends the current file to the recycle bin / trash (recoverable) and removes it from the list.
    /// A copy in the Favorites folder, if any, is left alone.
    pub(super) fn delete_current(&mut self) {
        let Some(media) = self.items.get(self.index).cloned() else {
            return;
        };
        match recycle::delete(&media.path) {
            Ok(deleted) => {
                self.remember_deleted(deleted);
                self.rotations.remove(&media.path);
                self.all.retain(|m| m.path != media.path);
                self.items.remove(self.index);
                if self.index >= self.items.len() {
                    self.index = self.items.len().saturating_sub(1);
                }
                self.shown = None;
                self.status = if recycle::UNDO_SUPPORTED {
                    "Moved to recycle bin. Press Ctrl+Z to undo.".into()
                } else {
                    "Moved to recycle bin.".into()
                };
            }
            Err(e) => self.status = format!("Delete failed: {e}"),
        }
    }

    /// Rotates only the on-screen view. The file on disk is not touched.
    pub(super) fn rotate_current(&mut self, clockwise: bool) {
        let Some(media) = self.items.get(self.index).cloned() else {
            return;
        };
        if media.kind != Kind::Image {
            return;
        }
        let r = self.rotations.entry(media.path).or_insert(0);
        *r = if clockwise {
            (*r + 1) % 4
        } else {
            (*r + 3) % 4
        };
        self.status = "Rotated view (file not changed).".into();
    }

    pub(super) fn reset_zoom(&mut self) {
        self.zoom = 1.0;
        self.prev_zoom = 1.0;
        self.pan = egui::Vec2::ZERO;
    }

    pub(super) fn zoom_by(&mut self, factor: f32) {
        self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
    }
}
