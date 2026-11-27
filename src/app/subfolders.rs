//! Helpers for browsing with subfolders included.

use std::path::{Path, PathBuf};

use super::App;
use crate::media::Media;

impl App {
    /// Scans the open folder again (after the "Subfolders" option changes), staying on `keep`.
    pub(super) fn rescan(&mut self, keep: Option<PathBuf>) {
        let Some(folder) = self.folder.clone() else {
            return;
        };
        self.load_folder(folder);
        if let Some(path) = keep
            && let Some(i) = self.position_of(&path)
        {
            self.index = i;
        }
    }

    /// `path` relative to the open folder (`Day1/photo.jpg`), or unchanged if it isn't inside it.
    pub(super) fn rel_path<'a>(&self, path: &'a Path) -> &'a Path {
        match &self.folder {
            Some(folder) => path.strip_prefix(folder).unwrap_or(path),
            None => path,
        }
    }

    /// What to show for a file in the status bar: its path relative to the open folder.
    pub(super) fn display_name(&self, media: &Media) -> String {
        self.rel_path(&media.path).to_string_lossy().to_string()
    }
}
