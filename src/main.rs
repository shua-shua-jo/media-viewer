#![windows_subsystem = "windows"]
use eframe::egui;
use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::OsString;
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

/// Name of the subfolder created inside the browsed folder.
const FAV_DIR: &str = "Favorites";

/// Images larger than this (in either dimension) are downscaled before upload to the GPU.
const MAX_TEXTURE_SIDE: u32 = 4096;

/// How many images after / before the current one are decoded in the background.
/// Each preloaded image costs roughly width * height * 4 bytes of memory (e.g. ~48 MB for 12 MP),
/// so lower these if memory matters more than speed.
const PRELOAD_AHEAD: usize = 2;
const PRELOAD_BEHIND: usize = 1;

/// Number of background decoder threads.
const WORKERS: usize = 2;

/// Zoom is relative to "fit to window" (1.0 = whole image visible).
const MIN_ZOOM: f32 = 0.2;
const MAX_ZOOM: f32 = 8.0;
/// Multiplier used by the zoom buttons and the +/- keys.
const ZOOM_STEP: f32 = 1.25;
/// How strongly the mouse wheel zooms (higher = faster).
const SCROLL_ZOOM_SPEED: f32 = 0.002;

const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "bmp"];
const VIDEO_EXTS: &[&str] = &["mp4", "mov", "mkv", "avi", "webm", "m4v"];

/// Identifies one decoded image: the file plus its view-only rotation (quarter turns).
type Key = (PathBuf, u8);

/// Decodes an image, downscales it if huge, and applies the view-only rotation.
/// Runs on a background thread; the file on disk is never modified.
fn decode_image(path: &Path, quarter_turns: u8) -> Result<egui::ColorImage, String> {
    let mut img = image::open(path).map_err(|e| e.to_string())?;
    if img.width() > MAX_TEXTURE_SIDE || img.height() > MAX_TEXTURE_SIDE {
        img = img.thumbnail(MAX_TEXTURE_SIDE, MAX_TEXTURE_SIDE);
    }
    img = match quarter_turns % 4 {
        1 => img.rotate90(),
        2 => img.rotate180(),
        3 => img.rotate270(),
        _ => img,
    };
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        size,
        rgba.as_raw(),
    ))
}

struct Shared {
    /// Images waiting to be decoded, most important first.
    queue: Mutex<VecDeque<Key>>,
    /// Images a worker is decoding right now.
    inflight: Mutex<HashSet<Key>>,
    wake: Condvar,
}

struct Loaded {
    key: Key,
    result: Result<egui::ColorImage, String>,
}

/// A small pool of background threads that decode images so the UI never waits on the disk or decoder.
struct Loader {
    shared: Arc<Shared>,
    rx: Receiver<Loaded>,
}

impl Loader {
    fn new(ctx: egui::Context, workers: usize) -> Self {
        let shared = Arc::new(Shared {
            queue: Mutex::new(VecDeque::new()),
            inflight: Mutex::new(HashSet::new()),
            wake: Condvar::new(),
        });
        let (tx, rx) = mpsc::channel::<Loaded>();

        for _ in 0..workers {
            let shared = Arc::clone(&shared);
            let tx = tx.clone();
            let ctx = ctx.clone();
            thread::spawn(move || {
                loop {
                    // Wait for a job; mark it in flight while still holding the queue lock.
                    let key = {
                        let mut queue = shared.queue.lock().unwrap();
                        loop {
                            if let Some(k) = queue.pop_front() {
                                shared.inflight.lock().unwrap().insert(k.clone());
                                break k;
                            }
                            queue = shared.wake.wait(queue).unwrap();
                        }
                    };

                    let result = catch_unwind(AssertUnwindSafe(|| decode_image(&key.0, key.1)))
                        .unwrap_or_else(|_| Err("decoder crashed on this file".to_string()));

                    let _ = tx.send(Loaded {
                        key: key.clone(),
                        result,
                    });
                    shared.inflight.lock().unwrap().remove(&key);
                    ctx.request_repaint(); // wake the UI so it picks up the result
                }
            });
        }

        Loader { shared, rx }
    }

    /// Replaces the waiting jobs with `wanted` (in priority order), skipping anything
    /// already finished (`skip`) or currently being decoded.
    fn set_wanted(&self, wanted: &[Key], skip: impl Fn(&Key) -> bool) {
        {
            let mut queue = self.shared.queue.lock().unwrap();
            let inflight = self.shared.inflight.lock().unwrap();
            queue.clear();
            for k in wanted {
                if !skip(k) && !inflight.contains(k) {
                    queue.push_back(k.clone());
                }
            }
        }
        self.shared.wake.notify_all();
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Image,
    Video,
}

/// Which files the viewer shows.
#[derive(Clone, Copy, PartialEq)]
enum Filter {
    All,
    Favorites,
    NotFavorites,
}

#[derive(Clone)]
struct Media {
    path: PathBuf,
    kind: Kind,
}

struct App {
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
    fn new(initial: Option<PathBuf>, ctx: egui::Context) -> Self {
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
        let mut items = Vec::new();
        match fs::read_dir(&folder) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_file() {
                        continue; // skips the Favorites subfolder too
                    }
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.to_lowercase())
                        .unwrap_or_default();
                    let kind = if IMAGE_EXTS.contains(&ext.as_str()) {
                        Kind::Image
                    } else if VIDEO_EXTS.contains(&ext.as_str()) {
                        Kind::Video
                    } else {
                        continue;
                    };
                    items.push(Media { path, kind });
                }
                items.sort_by(|a, b| a.path.file_name().cmp(&b.path.file_name()));
                self.status = format!("{} files loaded.", items.len());
            }
            Err(e) => self.status = format!("Could not read folder: {e}"),
        }
        self.all = items;
        self.favs = load_favs(&folder);
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
                match filter {
                    Filter::All => true,
                    Filter::Favorites => is_fav,
                    Filter::NotFavorites => !is_fav,
                }
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

    /// Collects finished decodes from the background threads and uploads them as textures.
    /// At most one upload per frame, so several large images arriving together can't cause a hitch.
    fn pump_loader(&mut self, ctx: &egui::Context, current_key: Option<&Key>) {
        while let Ok(loaded) = self.loader.rx.try_recv() {
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
                let key = (
                    m.path.clone(),
                    self.rotations.get(&m.path).copied().unwrap_or(0),
                );
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

    fn fav_path(&self, media: &Media) -> Option<PathBuf> {
        let folder = self.folder.as_ref()?;
        Some(folder.join(FAV_DIR).join(media.path.file_name()?))
    }

    fn is_favorite(&self, media: &Media) -> bool {
        media
            .path
            .file_name()
            .map(|n| self.favs.contains(n))
            .unwrap_or(false)
    }

    /// Favoriting copies the file into <folder>/Favorites; unfavoriting removes that copy.
    /// The original is never touched. (Swap `fs::copy` for `fs::rename` if you prefer a move.)
    fn toggle_favorite(&mut self) {
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
                if let Some(name) = media.path.file_name() {
                    if was_fav {
                        self.favs.remove(name);
                    } else {
                        self.favs.insert(name.to_os_string());
                    }
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
    fn delete_current(&mut self) {
        let Some(media) = self.items.get(self.index).cloned() else {
            return;
        };
        match trash::delete(&media.path) {
            Ok(()) => {
                self.rotations.remove(&media.path);
                self.all.retain(|m| m.path != media.path);
                self.items.remove(self.index);
                if self.index >= self.items.len() {
                    self.index = self.items.len().saturating_sub(1);
                }
                self.shown = None;
                self.status = "Moved to recycle bin.".into();
            }
            Err(e) => self.status = format!("Delete failed: {e}"),
        }
    }

    /// Rotates only the on-screen view. The file on disk is not touched.
    fn rotate_current(&mut self, clockwise: bool) {
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

    fn reset_zoom(&mut self) {
        self.zoom = 1.0;
        self.prev_zoom = 1.0;
        self.pan = egui::Vec2::ZERO;
    }

    fn zoom_by(&mut self, factor: f32) {
        self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
    }

    fn step(&mut self, delta: isize) {
        if self.items.is_empty() {
            return;
        }
        let n = self.items.len() as isize;
        self.index = (self.index as isize + delta).rem_euclid(n) as usize;
    }
}

/// Reads the names of the files already in <folder>/Favorites (empty if the folder doesn't exist).
fn load_favs(folder: &Path) -> HashSet<OsString> {
    let mut set = HashSet::new();
    if let Ok(entries) = fs::read_dir(folder.join(FAV_DIR)) {
        for entry in entries.flatten() {
            if entry.path().is_file() {
                set.insert(entry.file_name());
            }
        }
    }
    set
}

/// Draws the image filling the available area with zoom (mouse wheel / pinch), drag-to-pan and
/// double-click-to-reset. Zooming only changes how the already-uploaded texture is drawn on the GPU,
/// so it costs essentially nothing.
fn draw_zoomable(
    ui: &mut egui::Ui,
    tex: &egui::TextureHandle,
    zoom: &mut f32,
    prev_zoom: &mut f32,
    pan: &mut egui::Vec2,
) {
    let (rect, resp) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
    let tex_size = tex.size_vec2();
    // Scale that makes the whole image fit (never enlarges small images past 100% when "fit").
    let fit = (rect.width() / tex_size.x)
        .min(rect.height() / tex_size.y)
        .min(1.0);

    // The toolbar slider/buttons changed the zoom: keep the middle of the view where it was.
    if (*zoom - *prev_zoom).abs() > f32::EPSILON && *prev_zoom > 0.0 {
        *pan *= *zoom / *prev_zoom;
    }

    // Mouse wheel / trackpad pinch: zoom towards the cursor.
    if resp.hovered() {
        let (scroll, pinch, pointer) = ui.input(|i| {
            (
                i.smooth_scroll_delta.y,
                i.zoom_delta(),
                i.pointer.hover_pos(),
            )
        });
        let factor = pinch * (scroll * SCROLL_ZOOM_SPEED).exp();
        if (factor - 1.0).abs() > 1e-4 {
            let new_zoom = (*zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
            let ratio = new_zoom / *zoom;
            if let Some(p) = pointer {
                let center = rect.center() + *pan;
                let new_center = p - (p - center) * ratio;
                *pan = new_center - rect.center();
            }
            *zoom = new_zoom;
        }
    }

    if resp.dragged() {
        *pan += resp.drag_delta();
    }
    if resp.double_clicked() {
        *zoom = 1.0;
        *pan = egui::Vec2::ZERO;
    }

    // Keep the image from being dragged away; when it's smaller than the view it stays centered.
    let size = tex_size * fit * *zoom;
    let max_pan = ((size - rect.size()) * 0.5).max(egui::Vec2::ZERO);
    pan.x = pan.x.clamp(-max_pan.x, max_pan.x);
    pan.y = pan.y.clamp(-max_pan.y, max_pan.y);
    *prev_zoom = *zoom;

    if resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if resp.hovered() && (max_pan.x > 0.0 || max_pan.y > 0.0) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }

    let image_rect = egui::Rect::from_center_size(rect.center() + *pan, size);
    let full_uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    ui.painter_at(rect)
        .image(tex.id(), image_rect, full_uv, egui::Color32::WHITE);
}

/// Light red used for a favorited heart.
const HEART_RED: egui::Color32 = egui::Color32::from_rgb(255, 120, 140);

/// Draws a heart (a rotated square plus two circles) centered on `center`. `d` is the square's half-diagonal.
fn draw_heart(painter: &egui::Painter, center: egui::Pos2, d: f32, color: egui::Color32) {
    let cx = center.x;
    let cy = center.y + 0.1 * d; // shift so the whole heart is vertically centered
    let diamond = vec![
        egui::pos2(cx, cy - d),
        egui::pos2(cx + d, cy),
        egui::pos2(cx, cy + d),
        egui::pos2(cx - d, cy),
    ];
    painter.add(egui::Shape::convex_polygon(
        diamond,
        color,
        egui::Stroke::NONE,
    ));
    let r = d * std::f32::consts::FRAC_1_SQRT_2;
    painter.circle_filled(egui::pos2(cx - d / 2.0, cy - d / 2.0), r, color);
    painter.circle_filled(egui::pos2(cx + d / 2.0, cy - d / 2.0), r, color);
}

/// A clickable heart: hollow outline when `filled` is false, solid light red when true.
fn heart_button(ui: &mut egui::Ui, filled: bool) -> egui::Response {
    let d = 7.0_f32;
    let size = egui::vec2(2.414 * d + 10.0, 2.207 * d + 8.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

    if ui.is_rect_visible(rect) {
        let scale = if response.hovered() { 1.1 } else { 1.0 };
        let painter = ui.painter();
        if filled {
            draw_heart(painter, rect.center(), d * scale, HEART_RED);
        } else {
            // Hollow look: outline-colored heart with a smaller background-colored heart on top.
            let outline = ui.visuals().widgets.inactive.fg_stroke.color;
            draw_heart(painter, rect.center(), d * scale, outline);
            draw_heart(
                painter,
                rect.center(),
                d * scale * 0.72,
                ui.visuals().panel_fill,
            );
        }
    }
    response
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Keyboard shortcuts
        let (left, right, fav, del, rot_r, rot_l) = ui.ctx().input(|i| {
            (
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::ArrowRight),
                i.key_pressed(egui::Key::F),
                i.key_pressed(egui::Key::Delete),
                i.key_pressed(egui::Key::R),
                i.key_pressed(egui::Key::L),
            )
        });
        // Don't treat keys as shortcuts while the user is typing in the number box.
        let typing = self.goto_focused;
        let (left, right, fav, del, rot_r, rot_l) = (
            left && !typing,
            right && !typing,
            fav && !typing,
            del && !typing,
            rot_r && !typing,
            rot_l && !typing,
        );
        if left {
            self.step(-1);
        }
        if right {
            self.step(1);
        }
        if fav {
            self.toggle_favorite();
        }
        if rot_r {
            self.rotate_current(true);
        }
        if rot_l {
            self.rotate_current(false);
        }
        if del {
            self.delete_current();
        }

        // Zoom shortcuts: + / - / 0
        let (zin, zout, zfit) = ui.ctx().input(|i| {
            (
                i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals),
                i.key_pressed(egui::Key::Minus),
                i.key_pressed(egui::Key::Num0),
            )
        });
        if !typing {
            if zin {
                self.zoom_by(ZOOM_STEP);
            }
            if zout {
                self.zoom_by(1.0 / ZOOM_STEP);
            }
            if zfit {
                self.reset_zoom();
            }
        }

        let current = self.items.get(self.index).cloned();
        // Each new photo/video starts at "fit to window".
        let cur_path = current.as_ref().map(|m| m.path.clone());
        if cur_path != self.zoom_path {
            self.reset_zoom();
            self.zoom_path = cur_path;
        }
        let is_fav = current
            .as_ref()
            .map(|m| self.is_favorite(m))
            .unwrap_or(false);
        // Snapshot used at the end of the frame to detect changes made from the toolbar.
        let shown = (self.items.len(), self.index, self.favs.len());

        // Background loading: pick up finished images, then (re)plan what to keep and preload.
        let current_key: Option<Key> =
            current.as_ref().filter(|m| m.kind == Kind::Image).map(|m| {
                (
                    m.path.clone(),
                    self.rotations.get(&m.path).copied().unwrap_or(0),
                )
            });
        let ctx = ui.ctx().clone();
        self.pump_loader(&ctx, current_key.as_ref());
        self.update_wanted();

        let cur_tex = current_key
            .as_ref()
            .and_then(|k| self.cache.get(k).cloned());
        let cur_failed = current_key
            .as_ref()
            .map(|k| self.failed.contains(k))
            .unwrap_or(false);
        match (&cur_tex, &current) {
            (Some(t), _) => self.shown = Some(t.clone()),
            (None, None) => self.shown = None,
            (None, Some(m)) if m.kind == Kind::Video => self.shown = None,
            _ => {} // keep showing the previous image while this one loads
        }
        let loading = current_key.is_some() && cur_tex.is_none() && !cur_failed;

        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("📂 Open folder").clicked() {
                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        self.load_folder(dir);
                    }
                }
                ui.separator();
                let before = self.filter;
                ui.selectable_value(&mut self.filter, Filter::All, "All");
                ui.selectable_value(&mut self.filter, Filter::Favorites, "Favorites");
                ui.selectable_value(&mut self.filter, Filter::NotFavorites, "Not favorites");
                if self.filter != before {
                    self.apply_filter(current.as_ref().map(|m| m.path.clone()));
                }
                ui.separator();
                if ui.button("◀").clicked() {
                    self.step(-1);
                }
                if ui.button("▶").clicked() {
                    self.step(1);
                }
                if !self.items.is_empty() {
                    // Keep the box in sync with the current position unless the user is typing.
                    if !self.goto_focused {
                        self.goto_text = (self.index + 1).to_string();
                    }
                    let resp = ui
                        .add(egui::TextEdit::singleline(&mut self.goto_text).desired_width(44.0))
                        .on_hover_text("Type a number and press Enter to jump");
                    self.goto_focused = resp.has_focus();
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        match self.goto_text.trim().parse::<usize>() {
                            Ok(n) if n >= 1 && n <= self.items.len() => {
                                self.index = n - 1;
                                self.status = format!("Jumped to {n}.");
                            }
                            _ => {
                                self.status =
                                    format!("Enter a number from 1 to {}.", self.items.len());
                            }
                        }
                        ui.ctx().request_repaint();
                    }
                    ui.label(format!("/ {}", self.items.len()));
                }
                ui.separator();
                ui.add_enabled_ui(current.is_some(), |ui| {
                    if heart_button(ui, is_fav)
                        .on_hover_text(if is_fav {
                            "Remove from favorites (F)"
                        } else {
                            "Add to favorites (F)"
                        })
                        .clicked()
                    {
                        self.toggle_favorite();
                    }
                });
                ui.separator();
                let is_image = current
                    .as_ref()
                    .map(|m| m.kind == Kind::Image)
                    .unwrap_or(false);
                if ui
                    .add_enabled(is_image, egui::Button::new("Rotate left (L)"))
                    .clicked()
                {
                    self.rotate_current(false);
                }
                if ui
                    .add_enabled(is_image, egui::Button::new("Rotate right (R)"))
                    .clicked()
                {
                    self.rotate_current(true);
                }
                ui.separator();
                ui.add_enabled_ui(is_image, |ui| {
                    if ui.button("-").on_hover_text("Zoom out (-)").clicked() {
                        self.zoom_by(1.0 / ZOOM_STEP);
                    }
                    ui.add(
                        egui::Slider::new(&mut self.zoom, MIN_ZOOM..=MAX_ZOOM)
                            .logarithmic(true)
                            .show_value(false),
                    );
                    if ui.button("+").on_hover_text("Zoom in (+)").clicked() {
                        self.zoom_by(ZOOM_STEP);
                    }
                    ui.label(format!("{:.0}%", self.zoom * 100.0))
                        .on_hover_text("Relative to fit-to-window");
                    if ui
                        .button("Fit (0)")
                        .on_hover_text("Reset zoom (or double-click the image)")
                        .clicked()
                    {
                        self.reset_zoom();
                    }
                });
                ui.separator();
                if ui
                    .add_enabled(current.is_some(), egui::Button::new("🗑 Delete (Del)"))
                    .clicked()
                {
                    self.delete_current();
                }
            });
        });

        egui::Panel::bottom("status").show(ui, |ui| {
            let name = current
                .as_ref()
                .and_then(|m| m.path.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("");
            let loading_note = if loading { "    Loading…" } else { "" };
            ui.label(format!("{name}    {}{loading_note}", self.status));
        });

        egui::CentralPanel::default().show(ui, |ui| match &current {
            None => {
                let msg = if self.all.is_empty() {
                    "No photos or videos to show."
                } else {
                    "Nothing matches this filter."
                };
                ui.centered_and_justified(|ui| ui.label(msg));
            }
            Some(m) if m.kind == Kind::Image => {
                // The current image if ready; otherwise the previous one (unless this one failed).
                let tex = cur_tex.as_ref().or(if cur_failed {
                    None
                } else {
                    self.shown.as_ref()
                });
                if let Some(tex) = tex {
                    draw_zoomable(ui, tex, &mut self.zoom, &mut self.prev_zoom, &mut self.pan);
                } else if cur_failed {
                    ui.centered_and_justified(|ui| ui.label("Could not load this image."));
                } else {
                    ui.centered_and_justified(|ui| ui.label("Loading…"));
                }
            }
            Some(m) => {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() / 3.0);
                    ui.heading("🎬 Video");
                    if ui.button("▶ Play in default player").clicked() {
                        if let Err(e) = open::that(&m.path) {
                            self.status = format!("Could not open video: {e}");
                        }
                    }
                });
            }
        });

        // If a toolbar action changed what is shown, draw one more frame right away.
        if shown != (self.items.len(), self.index, self.favs.len()) {
            ui.ctx().request_repaint();
        }
    }
}

fn main() -> eframe::Result {
    fn load_icon() -> egui::IconData {
        let img = image::load_from_memory(include_bytes!("../assets/icon.png"))
            .expect("could not read icon.png")
            .into_rgba8();
        let (width, height) = img.dimensions();
        egui::IconData {
            rgba: img.into_raw(),
            width,
            height,
        }
    }
    // Optional: pass a folder on the command line, e.g. `cargo run --release -- C:\Photos`
    let initial = std::env::args().nth(1).map(PathBuf::from);

    eframe::run_native(
        "Media Viewer",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_icon(load_icon()),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(App::new(initial, cc.egui_ctx.clone())))),
    )
}
