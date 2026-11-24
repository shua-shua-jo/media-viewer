//! What a "media file" is, how folders are scanned and sorted, and which files are favorites.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::iter::Peekable;
use std::path::{Path, PathBuf};
use std::str::Chars;
use std::time::SystemTime;

use crate::config::{FAV_DIR, IMAGE_EXTS, VIDEO_EXTS};

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Image,
    Video,
}

/// Which files the viewer shows.
#[derive(Clone, Copy, PartialEq, Debug)]
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

/// The order files are shown in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sort {
    NameAsc,
    NameDesc,
    DateAsc,
    DateDesc,
    SizeAsc,
    SizeDesc,
}

impl Sort {
    pub const ALL: [Sort; 6] = [
        Sort::NameAsc,
        Sort::NameDesc,
        Sort::DateDesc,
        Sort::DateAsc,
        Sort::SizeDesc,
        Sort::SizeAsc,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Sort::NameAsc => "Name (A-Z)",
            Sort::NameDesc => "Name (Z-A)",
            Sort::DateAsc => "Date modified (oldest first)",
            Sort::DateDesc => "Date modified (newest first)",
            Sort::SizeAsc => "Size (smallest first)",
            Sort::SizeDesc => "Size (largest first)",
        }
    }
}

#[derive(Clone)]
pub struct Media {
    pub path: PathBuf,
    pub kind: Kind,
    /// File size in bytes (0 if unknown).
    pub size: u64,
    /// Last-modified time, if the file system reports one.
    pub modified: Option<SystemTime>,
}

impl Media {
    pub fn new(path: PathBuf, kind: Kind) -> Self {
        Media {
            path,
            kind,
            size: 0,
            modified: None,
        }
    }
}

/// Lists the photos and videos directly inside `folder`, sorted by name.
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
        let meta = entry.metadata().ok();
        let mut media = Media::new(path, kind);
        media.size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        media.modified = meta.and_then(|m| m.modified().ok());
        items.push(media);
    }
    sort_media(&mut items, Sort::NameAsc);
    Ok(items)
}

/// Sorts `items` in place. Ties (for example two files with the same size) fall back to name order.
pub fn sort_media(items: &mut [Media], sort: Sort) {
    items.sort_by(|a, b| compare(a, b, sort));
}

fn file_name(m: &Media) -> Cow<'_, str> {
    m.path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default()
}

fn compare(a: &Media, b: &Media, sort: Sort) -> Ordering {
    let by_name = || natural_cmp(&file_name(a), &file_name(b)).then_with(|| a.path.cmp(&b.path));
    match sort {
        Sort::NameAsc => by_name(),
        Sort::NameDesc => by_name().reverse(),
        // `None` (no date available) sorts as the oldest.
        Sort::DateAsc => a.modified.cmp(&b.modified).then_with(by_name),
        Sort::DateDesc => b.modified.cmp(&a.modified).then_with(by_name),
        Sort::SizeAsc => a.size.cmp(&b.size).then_with(by_name),
        Sort::SizeDesc => b.size.cmp(&a.size).then_with(by_name),
    }
}

/// "Natural" name order: ignores case and compares runs of digits as numbers,
/// so `img2.jpg` comes before `img10.jpg`.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut a = a.chars().peekable();
    let mut b = b.chars().peekable();
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                if x.is_ascii_digit() && y.is_ascii_digit() {
                    let na = take_digits(&mut a);
                    let nb = take_digits(&mut b);
                    let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                    // Longer run (after leading zeros) is the bigger number; same length compares as text.
                    let ord = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                    if ord != Ordering::Equal {
                        return ord;
                    }
                } else {
                    let (lx, ly) = (lower(x), lower(y));
                    if lx != ly {
                        return lx.cmp(&ly);
                    }
                    a.next();
                    b.next();
                }
            }
        }
    }
}

fn take_digits(it: &mut Peekable<Chars<'_>>) -> String {
    let mut digits = String::new();
    while let Some(&c) = it.peek() {
        if c.is_ascii_digit() {
            digits.push(c);
            it.next();
        } else {
            break;
        }
    }
    digits
}

fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
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
