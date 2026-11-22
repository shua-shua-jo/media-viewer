//! Opening a folder or a single file: command line / "Open with", drag and drop, and the file picker.

use std::path::{Path, PathBuf};

use eframe::egui;

use super::App;
use crate::media::Filter;

impl App {
    /// Opens `path`. A folder is browsed directly. For a file, its folder is browsed and the view
    /// jumps straight to that file.
    pub(super) fn open_path(&mut self, path: PathBuf) {
        // Work with an absolute path so a bare "photo.jpg" has a real parent folder.
        let path = std::path::absolute(&path).unwrap_or(path);

        if path.is_dir() {
            self.load_folder(path);
            return;
        }
        if !path.is_file() {
            self.status = format!("Could not open {}: not found.", path.display());
            return;
        }
        let Some(folder) = path.parent().map(Path::to_path_buf) else {
            self.status = format!("Could not open {}.", path.display());
            return;
        };

        // A filter such as "Favorites" could hide the file we were asked to show.
        self.filter = Filter::All;
        self.load_folder(folder);

        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        match self.position_of(&path) {
            Some(i) => {
                self.index = i;
                self.status = format!("Opened {name}.");
            }
            None => {
                self.status =
                    format!("{name} isn't a supported photo or video; showing its folder.");
            }
        }
    }

    /// Index of `path` in the visible list, matching by file name (exact first, then ignoring case).
    fn position_of(&self, path: &Path) -> Option<usize> {
        let name = path.file_name()?;
        if let Some(i) = self
            .items
            .iter()
            .position(|m| m.path.file_name() == Some(name))
        {
            return Some(i);
        }
        let wanted = name.to_string_lossy().to_lowercase();
        self.items.iter().position(|m| {
            m.path
                .file_name()
                .map(|f| f.to_string_lossy().to_lowercase() == wanted)
                .unwrap_or(false)
        })
    }

    /// Opens the first file or folder dropped onto the window.
    pub(super) fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        // In recent egui versions a dropped file is an object whose `path()` returns the file's path.
        let first = ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf()));
        if let Some(path) = first {
            self.open_path(path);
        }
    }
}
