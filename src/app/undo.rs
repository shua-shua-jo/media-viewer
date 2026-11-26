//! Undo for deleted files: remembers what was sent to the Recycle Bin and restores it on request.

use std::path::Path;

use super::App;
use crate::config::UNDO_LIMIT;
use crate::media;
use crate::recycle::{self, Deleted};

impl App {
    /// Remembers a deleted file so it can be restored. Only the most recent `UNDO_LIMIT` are kept.
    pub(super) fn remember_deleted(&mut self, deleted: Deleted) {
        if !recycle::UNDO_SUPPORTED {
            return;
        }
        self.undo_stack.push(deleted);
        if self.undo_stack.len() > UNDO_LIMIT {
            self.undo_stack.remove(0);
        }
    }

    /// Restores the most recently deleted file from the Recycle Bin.
    pub(super) fn undo_delete(&mut self) {
        if !recycle::UNDO_SUPPORTED {
            self.status =
                "Undo isn't available on this system; restore the file from the Trash.".into();
            return;
        }
        let Some(deleted) = self.undo_stack.pop() else {
            self.status = "Nothing to undo.".into();
            return;
        };

        let name = deleted
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        match recycle::restore(&deleted) {
            Ok(()) => {
                self.reinsert(&deleted.path);
                self.status = format!("Restored {name}.");
            }
            Err(e) => self.status = format!("Could not restore {name}: {e}"),
        }
    }

    /// Adds a restored file back to the list (if its folder is the one currently open) and selects it.
    pub(super) fn reinsert(&mut self, path: &Path) {
        if path.parent().is_none() || path.parent() != self.folder.as_deref() {
            return; // restored into a folder that isn't open; nothing to show
        }
        let Some(restored) = media::media_for(path) else {
            return;
        };
        if !self.all.iter().any(|m| m.path == restored.path) {
            self.all.push(restored);
        }
        media::sort_media(&mut self.all, self.sort);
        self.apply_filter(Some(path.to_path_buf()));
    }
}
