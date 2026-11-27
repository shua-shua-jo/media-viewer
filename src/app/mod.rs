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
mod fullscreen;
mod gif_view;
mod open;
mod persist;
mod preload;
mod shortcuts;
mod slideshow;
mod subfolders;
#[cfg(test)]
mod tests;
mod toolbar;
mod undo;
mod video_view;
mod view;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;

use eframe::egui;

use crate::animation::AnimPlayer;
use crate::config::SLIDESHOW_DEFAULT_SECS;
use crate::config::WORKERS;
use crate::loader::{Key, Loader};
use crate::media::{self, Filter, Media, Sort};
use crate::recycle::Deleted;
use crate::settings::Settings;
use crate::video::Player;

pub struct App {
    folder: Option<PathBuf>,
    /// Every photo/video in the folder.
    all: Vec<Media>,
    /// The subset currently shown (depends on `filter`). `index` points into this list.
    items: Vec<Media>,
    /// Paths present in the Favorites subfolder, relative to it.
    favs: HashSet<PathBuf>,
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
    /// Embedded player for the current video, if any.
    player: Option<Player>,
    /// A video the player failed to start for, so we don't retry every frame.
    player_failed: Option<PathBuf>,
    /// The current sort order.
    sort: Sort,
    /// The settings as last written to disk, to detect changes.
    saved_settings: Settings,
    last_settings_save: Instant,
    fullscreen: bool,
    fullscreen_hint_until: Option<Instant>,
    slideshow: bool,
    slideshow_secs: u32,
    slideshow_next: Instant,
    /// The photo the slideshow last showed; a different index means the user moved themselves.
    slideshow_seen_index: usize,
    undo_stack: Vec<Deleted>,
    include_subfolders: bool,
    anim: Option<AnimPlayer>,
}

impl App {
    pub fn new(initial: Option<PathBuf>, ctx: egui::Context, settings: Settings) -> Self {
        let mut app = App {
            folder: None,
            all: Vec::new(),
            items: Vec::new(),
            favs: HashSet::new(),
            filter: settings.filter,
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
            player: None,
            player_failed: None,
            sort: settings.sort,
            saved_settings: settings.clone(),
            last_settings_save: Instant::now(),
            fullscreen: false,
            fullscreen_hint_until: None,
            slideshow: false,
            slideshow_secs: SLIDESHOW_DEFAULT_SECS,
            slideshow_next: Instant::now(),
            slideshow_seen_index: 0,
            undo_stack: Vec::new(),
            include_subfolders: settings.include_subfolders,
            anim: None,
        };
        match initial {
            Some(p) => app.open_path(p),
            None => {
                // Reopen the folder from last time, if it still exists.
                if let Some(folder) = settings.last_folder.filter(|f| f.is_dir()) {
                    app.load_folder(folder);
                }
            }
        }
        app
    }

    fn load_folder(&mut self, folder: PathBuf) {
        let items = match media::scan_folder(&folder, self.include_subfolders) {
            Ok(scan) => {
                self.status = if scan.truncated {
                    format!(
                        "Showing the first {} files; there are more than that.",
                        scan.items.len()
                    )
                } else {
                    format!("{} files loaded.", scan.items.len())
                };
                scan.items
            }
            Err(e) => {
                self.status = format!("Could not read folder: {e}");
                Vec::new()
            }
        };
        self.all = items;
        media::sort_media(&mut self.all, self.sort);
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
        let folder = &self.folder;
        let visible: Vec<Media> = self
            .all
            .iter()
            .filter(|m| {
                let rel = match folder {
                    Some(f) => m.path.strip_prefix(f).unwrap_or(&m.path),
                    None => m.path.as_path(),
                };
                filter.matches(favs.contains(rel))
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

    /// Re-sorts the file list after the sort order changes, keeping `keep` selected if possible.
    fn resort(&mut self, keep: Option<PathBuf>) {
        media::sort_media(&mut self.all, self.sort);
        self.apply_filter(keep);
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
