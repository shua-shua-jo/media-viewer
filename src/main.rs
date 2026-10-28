use eframe::egui;
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
    texture: Option<egui::TextureHandle>,
}

impl App {
    fn new(initial: Option<PathBuf>) -> Self {
        let mut app = App {
            folder: None,
            items: Vec::new(),
            index: 0,
            status: "Open a folder to begin.".into(),
            loaded_path: None,
            texture: None,
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
        self.loaded_path = None;
        self.texture = None;
    }

    /// Decode the image ourselves (no URI loaders involved) and upload it as a texture.
    fn ensure_texture(&mut self, ctx: &egui::Context, path: &Path) {
        if self.loaded_path.as_deref() == Some(path) {
            return;
        }
        self.loaded_path = Some(path.to_path_buf());
        self.texture = None;

        match image::open(path) {
            Ok(mut img) => {
                if img.width() > MAX_TEXTURE_SIDE || img.height() > MAX_TEXTURE_SIDE {
                    img = img.thumbnail(MAX_TEXTURE_SIDE, MAX_TEXTURE_SIDE);
                }
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
        let (left, right, fav) = ui.ctx().input(|i| {
            (
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::ArrowRight),
                i.key_pressed(egui::Key::F),
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

        let current = self.items.get(self.index).cloned();
        let is_fav = current
            .as_ref()
            .map(|m| self.is_favorite(m))
            .unwrap_or(false);

        if let Some(m) = &current {
            if m.kind == Kind::Image {
                let ctx = ui.ctx().clone();
                self.ensure_texture(&ctx, &m.path);
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
