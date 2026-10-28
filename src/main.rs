use eframe::egui;
use std::collections::HashMap;
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

#[derive(Clone)]
struct Media {
    path: PathBuf,
    kind: Kind,
}

struct App {
    folder: Option<PathBuf>,
    items: Vec<Media>,
    index: usize,
    status: String,
    /// Path of the image currently held in `texture` (set even if loading failed).
    loaded_path: Option<PathBuf>,
    /// Quarter turns (0-3, clockwise) that `texture` was built with.
    loaded_rot: u8,
    texture: Option<egui::TextureHandle>,
    /// View-only rotation per file, in quarter turns clockwise. Never written to disk.
    rotations: HashMap<PathBuf, u8>,
}

impl App {
    fn new(initial: Option<PathBuf>) -> Self {
        let mut app = App {
            folder: None,
            items: Vec::new(),
            index: 0,
            status: "Open a folder to begin.".into(),
            loaded_path: None,
            loaded_rot: 0,
            texture: None,
            rotations: HashMap::new(),
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
        self.items = items;
        self.index = 0;
        self.folder = Some(folder);
        self.rotations.clear();
        self.loaded_path = None;
        self.texture = None;
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
        self.fav_path(media).map(|p| p.exists()).unwrap_or(false)
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

        let result = if dest.exists() {
            fs::remove_file(&dest).map(|_| "Removed from favorites.".to_string())
        } else {
            dest.parent()
                .map(fs::create_dir_all)
                .transpose()
                .and_then(|_| fs::copy(&media.path, &dest))
                .map(|_| "Added to favorites.".to_string())
        };

        self.status = match result {
            Ok(msg) => msg,
            Err(e) => format!("Favorite failed: {e}"),
        };
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

        if let Some(m) = &current {
            if m.kind == Kind::Image {
                let ctx = ui.ctx().clone();
                let rot = self.rotations.get(&m.path).copied().unwrap_or(0);
                self.ensure_texture(&ctx, &m.path, rot);
            }
        }

        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("📂 Open folder").clicked() {
                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        self.load_folder(dir);
                    }
                }
                ui.separator();
                if ui.button("◀").clicked() {
                    self.step(-1);
                }
                if ui.button("▶").clicked() {
                    self.step(1);
                }
                if !self.items.is_empty() {
                    ui.label(format!("{} / {}", self.index + 1, self.items.len()));
                }
                ui.separator();
                let label = if is_fav {
                    "★ Favorited"
                } else {
                    "☆ Favorite (F)"
                };
                if ui
                    .add_enabled(current.is_some(), egui::Button::new(label))
                    .clicked()
                {
                    self.toggle_favorite();
                }
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
                ui.centered_and_justified(|ui| ui.label("No photos or videos to show."));
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
    }
}

fn main() -> eframe::Result {
    // Optional: pass a folder on the command line, e.g. `cargo run --release -- C:\Photos`
    let initial = std::env::args().nth(1).map(PathBuf::from);

    eframe::run_native(
        "Media Viewer",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(App::new(initial)))),
    )
}
