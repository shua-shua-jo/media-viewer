#![windows_subsystem = "windows"]
use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

/// Name of the subfolder created inside the browsed folder.
const FAV_DIR: &str = "Favorites";

/// Images larger than this (in either dimension) are downscaled before upload to the GPU.
const MAX_TEXTURE_SIDE: u32 = 4096;

const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "bmp"];
const VIDEO_EXTS: &[&str] = &["mp4", "mov", "mkv", "avi", "webm", "m4v"];

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
    /// Path of the image currently held in `texture` (set even if loading failed).
    loaded_path: Option<PathBuf>,
    /// Quarter turns (0-3, clockwise) that `texture` was built with.
    loaded_rot: u8,
    texture: Option<egui::TextureHandle>,
    /// View-only rotation per file, in quarter turns clockwise. Never written to disk.
    rotations: HashMap<PathBuf, u8>,
    /// Text in the "go to number" box.
    goto_text: String,
    /// True while the "go to number" box has keyboard focus (so shortcuts don't fire while typing).
    goto_focused: bool,
}

impl App {
    fn new(initial: Option<PathBuf>) -> Self {
        let mut app = App {
            folder: None,
            all: Vec::new(),
            items: Vec::new(),
            favs: HashSet::new(),
            filter: Filter::All,
            index: 0,
            status: "Open a folder to begin.".into(),
            loaded_path: None,
            loaded_rot: 0,
            texture: None,
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
        self.loaded_path = None;
        self.texture = None;
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

    /// Decode the image ourselves (no URI loaders involved), apply the view-only rotation,
    /// and upload it as a texture. The file on disk is never modified.
    fn ensure_texture(&mut self, ctx: &egui::Context, path: &Path, quarter_turns: u8) {
        if self.loaded_path.as_deref() == Some(path) && self.loaded_rot == quarter_turns {
            return;
        }
        self.loaded_path = Some(path.to_path_buf());
        self.loaded_rot = quarter_turns;
        self.texture = None;

        match image::open(path) {
            Ok(mut img) => {
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
                let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                self.texture = Some(ctx.load_texture("current_image", color, Default::default()));
            }
            Err(e) => self.status = format!("Could not load image: {e}"),
        }
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
                self.loaded_path = None;
                self.texture = None;
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

        let current = self.items.get(self.index).cloned();
        let is_fav = current
            .as_ref()
            .map(|m| self.is_favorite(m))
            .unwrap_or(false);
        // Snapshot used at the end of the frame to detect changes made from the toolbar.
        let shown = (self.items.len(), self.index, self.favs.len());

        if let Some(m) = &current {
            if m.kind == Kind::Image {
                let ctx = ui.ctx().clone();
                let rot = self.rotations.get(&m.path).copied().unwrap_or(0);
                self.ensure_texture(&ctx, &m.path, rot);
            }
        }

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
            ui.label(format!("{name}    {}", self.status));
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
                if let Some(tex) = &self.texture {
                    ui.centered_and_justified(|ui| {
                        let sized = egui::load::SizedTexture::from_handle(tex);
                        ui.add(egui::Image::new(sized).shrink_to_fit());
                    });
                } else {
                    ui.centered_and_justified(|ui| ui.label("Could not load this image."));
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
        Box::new(|_cc| Ok(Box::new(App::new(initial)))),
    )
}
