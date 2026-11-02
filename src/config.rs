//! All tunable settings in one place. Change a value here and rebuild.

use eframe::egui;

/// Name of the subfolder created inside the browsed folder for favorites.
pub const FAV_DIR: &str = "Favorites";

pub const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "bmp"];
pub const VIDEO_EXTS: &[&str] = &["mp4", "mov", "mkv", "avi", "webm", "m4v"];

/// Images larger than this (in either dimension) are downscaled before upload to the GPU.
pub const MAX_TEXTURE_SIDE: u32 = 4096;

/// How many images after / before the current one are decoded in the background.
/// Each preloaded image costs roughly width * height * 4 bytes of memory (e.g. ~48 MB for 12 MP),
/// so lower these if memory matters more than speed.
pub const PRELOAD_AHEAD: usize = 2;
pub const PRELOAD_BEHIND: usize = 1;

/// Number of background decoder threads.
pub const WORKERS: usize = 2;

/// Zoom is relative to "fit to window" (1.0 = whole image visible).
pub const MIN_ZOOM: f32 = 0.2;
pub const MAX_ZOOM: f32 = 8.0;
/// Multiplier used by the zoom buttons and the +/- keys.
pub const ZOOM_STEP: f32 = 1.25;
/// How strongly the mouse wheel zooms (higher = faster).
pub const SCROLL_ZOOM_SPEED: f32 = 0.002;

/// Light red used for a favorited heart.
pub const HEART_RED: egui::Color32 = egui::Color32::from_rgb(255, 120, 140);

/// Smallest size the window can be shrunk to (in points). The toolbar wraps below this width
/// anyway, but anything narrower makes the controls hard to use.
pub const MIN_WINDOW_WIDTH: f32 = 600.0;
pub const MIN_WINDOW_HEIGHT: f32 = 400.0;
