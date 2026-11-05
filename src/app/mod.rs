//! The application: its state (`App`) and the core list/filter logic.
//!
//! Behavior is split across child modules, which can all see `App`'s private fields:
//!   - `preload`   : background image loading and the texture cache
//!   - `actions`   : favorite, delete, rotate and zoom actions
//!   - `shortcuts` : keyboard shortcuts
//!   - `toolbar`   : the top toolbar
//!   - `view`      : the per-frame `ui` function, status bar and central image area
//!
//! To add a feature, add a new file here (e.g. `thumbnails.rs`), declare it below,
//! and give it its own `impl App { ... }` block.

mod actions;
mod preload;
mod shortcuts;
#[cfg(test)]
mod tests;
mod toolbar;
mod view;

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::PathBuf;

use eframe::egui;

use crate::config::WORKERS;
use crate::loader::{Key, Loader};
use crate::media::{self, Filter, Media};

pub struct App {
    folder: Option<PathBuf>,
    /// Every photo/video in the folder.
    all: Vec<Media>,
    /// The subset currently shown (depends on `filter`). `index` points into this list.
    items: Vec<Media>,
    /// File names present in the Favorites subfolder, kept in memory so we never hit the disk per frame.
    favs: HashSet<OsString>,
    filter: Filter,
    index: usize,
    status: String,
    /// Background decoder threads.
    loader: Loader,
    /// Ready-to-draw textures for the current image and its neighbors.
    cache: HashMap<Key, egui::TextureHandle>,
    /// Images that could not be decoded (so we don't retry every frame).
    failed: HashSet<Key>,
    /// The set of images we last asked the loader for, in priority order.
    last_wanted: Vec<Key>,
    /// The last texture that was on screen; kept visible while the next one loads.
    shown: Option<egui::TextureHandle>,
    /// Current zoom (1.0 = fit to window) and pan offset in screen points. View-only state.
    zoom: f32,
    /// Zoom value at the end of the previous frame, used to keep the view centered when the slider moves.
    prev_zoom: f32,
    pan: egui::Vec2,
    /// The file the zoom/pan state belongs to; zoom resets when this changes.
    zoom_path: Option<PathBuf>,
    /// View-only rotation per file, in quarter turns clockwise. Never written to disk.
    rotations: HashMap<PathBuf, u8>,
    /// Text in the "go to number" box.
    goto_text: String,
    /// True while the "go to number" box has keyboard focus (so shortcuts don't fire while typing).
    goto_focused: bool,
}

impl App {
    pub fn new(initial: Option<PathBuf>, ctx: egui::Context) -> Self {
        let mut app = App {
            folder: None,
            all: Vec::new(),
            items: Vec::new(),
            favs: HashSet::new(),
            filter: Filter::All,
            index: 0,
            status: "Open a folder to begin.".into(),
            loader: Loader::new(ctx, WORKERS),
            cache: HashMap::new(),
            failed: HashSet::new(),
            last_wanted: Vec::new(),
            shown: None,
            zoom: 1.0,
            prev_zoom: 1.0,
            pan: egui::Vec2::ZERO,
            zoom_path: None,
            rotations: HashMap::new(),
            goto_text: String::new(),
            goto_focused: false,
        };
        if let Some(p) = initial {
            app.load_folder(p);
        }
        app
    }

    fn load_folder(&mut self, folder: PathBuf) {
        let items = match media::scan_folder(&folder) {
            Ok(items) => {
                self.status = format!("{} files loaded.", items.len());
                items
            }
            Err(e) => {
                self.status = format!("Could not read folder: {e}");
                Vec::new()
            }
        };
        self.all = items;
        self.favs = media::load_favs(&folder);
        self.folder = Some(folder);
        self.rotations.clear();
        self.cache.clear();
        self.failed.clear();
        self.last_wanted.clear();
        self.shown = None;
        self.index = 0;
        self.apply_filter(None);
    }

    /// Rebuilds the visible list from `all` according to `filter`.
    /// Tries to stay on `keep` if it is still visible; otherwise stays near the same position.
    fn apply_filter(&mut self, keep: Option<PathBuf>) {
        let filter = self.filter;
        let favs = &self.favs;
        let visible: Vec<Media> = self
            .all
            .iter()
            .filter(|m| {
                let is_fav = m
                    .path
                    .file_name()
                    .map(|n| favs.contains(n))
                    .unwrap_or(false);
                filter.matches(is_fav)
            })
            .cloned()
            .collect();

        let fallback = self.index.min(visible.len().saturating_sub(1));
        let new_index = keep
            .and_then(|p| visible.iter().position(|m| m.path == p))
            .unwrap_or(fallback);
        self.items = visible;
        self.index = new_index;
    }

    /// Moves forward/backward through the visible list, wrapping around at the ends.
    fn step(&mut self, delta: isize) {
        if self.items.is_empty() {
            return;
        }
        let n = self.items.len() as isize;
        self.index = (self.index as isize + delta).rem_euclid(n) as usize;
    }
}
