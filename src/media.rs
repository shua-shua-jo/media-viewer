//! What a "media file" is, how folders are scanned, and which files are favorites.

use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::config::{FAV_DIR, IMAGE_EXTS, VIDEO_EXTS};

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Image,
    Video,
}

/// Which files the viewer shows.
#[derive(Clone, Copy, PartialEq)]
pub enum Filter {
    All,
    Favorites,
    NotFavorites,
}

impl Filter {
    /// Whether a file should be shown, given whether it is a favorite.
    pub fn matches(self, is_fav: bool) -> bool {
        match self {
            Filter::All => true,
            Filter::Favorites => is_fav,
            Filter::NotFavorites => !is_fav,
        }
    }
}

#[derive(Clone)]
pub struct Media {
    pub path: PathBuf,
    pub kind: Kind,
}

/// Lists the photos and videos directly inside `folder`, sorted by file name.
/// Subfolders (including the Favorites folder) are not scanned.
pub fn scan_folder(folder: &Path) -> io::Result<Vec<Media>> {
    let mut items = Vec::new();
    for entry in fs::read_dir(folder)?.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
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
    Ok(items)
}

/// Reads the names of the files already in <folder>/Favorites (empty if the folder doesn't exist).
pub fn load_favs(folder: &Path) -> HashSet<OsString> {
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
