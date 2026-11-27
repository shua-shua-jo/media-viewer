//! What a "media file" is, how folders are scanned and sorted, and which files are favorites.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs;
use std::io;
use std::iter::Peekable;
use std::path::{Path, PathBuf};
use std::str::Chars;
use std::time::SystemTime;

use crate::config::{FAV_DIR, IMAGE_EXTS, MAX_SCAN_DEPTH, MAX_SCAN_FILES, VIDEO_EXTS};

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

/// Whether `path` looks like a photo or a video, judging by its extension.
fn kind_of(path: &Path) -> Option<Kind> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();
    if IMAGE_EXTS.contains(&ext.as_str()) {
        Some(Kind::Image)
    } else if VIDEO_EXTS.contains(&ext.as_str()) {
        Some(Kind::Video)
    } else {
        None
    }
}

/// Builds a `Media` entry for a single existing file (`None` if it isn't a supported photo or video).
pub fn media_for(path: &Path) -> Option<Media> {
    if !path.is_file() {
        return None;
    }
    let kind = kind_of(path)?;
    let meta = fs::metadata(path).ok();
    let mut media = Media::new(path.to_path_buf(), kind);
    media.size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
    media.modified = meta.and_then(|m| m.modified().ok());
    Some(media)
}

/// The result of scanning a folder.
pub struct Scan {
    pub items: Vec<Media>,
    /// True if scanning stopped early because the file limit (`MAX_SCAN_FILES`) was reached.
    pub truncated: bool,
}

/// Lists the photos and videos in `folder`, sorted by name. With `recursive`, subfolders are
/// included too. The `Favorites` folder, hidden folders (starting with `.`) and system folders
/// are never scanned, and links to folders are not followed (they could form loops).
pub fn scan_folder(folder: &Path, recursive: bool) -> io::Result<Scan> {
    scan_limited(folder, recursive, MAX_SCAN_FILES)
}

/// Like `scan_folder`, with an explicit cap on the number of files (so tests can exercise the limit).
pub fn scan_limited(folder: &Path, recursive: bool, max_files: usize) -> io::Result<Scan> {
    let mut items: Vec<Media> = Vec::new();
    let mut truncated = false;
    let mut pending: Vec<(PathBuf, usize)> = vec![(folder.to_path_buf(), 0)];

    'scan: while let Some((dir, depth)) = pending.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if depth == 0 => return Err(e), // the folder you asked for must be readable
            Err(_) => continue,                    // an unreadable subfolder is just skipped
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let path = entry.path();

            if file_type.is_dir() {
                if recursive && depth < MAX_SCAN_DEPTH && !skip_dir(&path) {
                    pending.push((path, depth + 1));
                }
                continue;
            }
            // Real files, and links to files.
            if !(file_type.is_file() || (file_type.is_symlink() && path.is_file())) {
                continue;
            }
            let Some(kind) = kind_of(&path) else {
                continue;
            };

            if items.len() >= max_files {
                truncated = true;
                break 'scan;
            }
            let meta = entry.metadata().ok();
            let mut media = Media::new(path, kind);
            media.size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            media.modified = meta.and_then(|m| m.modified().ok());
            items.push(media);
        }
    }

    sort_media(&mut items, Sort::NameAsc);
    Ok(Scan { items, truncated })
}

/// Folders that are never scanned: our own Favorites copies, hidden and system folders.
fn skip_dir(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    name.starts_with('.')
        || name.starts_with('$')
        || name.eq_ignore_ascii_case(FAV_DIR)
        || name.eq_ignore_ascii_case("System Volume Information")
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

fn parent_name(m: &Media) -> Cow<'_, str> {
    m.path
        .parent()
        .map(|p| p.to_string_lossy())
        .unwrap_or_default()
}

fn compare(a: &Media, b: &Media, sort: Sort) -> Ordering {
    // Files are grouped by folder first (only matters when subfolders are included), then by name.
    let by_name = || {
        natural_cmp(&parent_name(a), &parent_name(b))
            .then_with(|| natural_cmp(&file_name(a), &file_name(b)))
            .then_with(|| a.path.cmp(&b.path))
    };
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

/// Reads the files already in <folder>/Favorites, as paths relative to that folder:
/// `photo.jpg`, or `Day1/photo.jpg` for the favorite of a file that lives in a subfolder.
pub fn load_favs(folder: &Path) -> HashSet<PathBuf> {
    let root = folder.join(FAV_DIR);
    let mut set = HashSet::new();
    let mut pending: Vec<(PathBuf, usize)> = vec![(root.clone(), 0)];

    while let Some((dir, depth)) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue; // no Favorites folder yet, or it can't be read
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if file_type.is_dir() && depth < MAX_SCAN_DEPTH {
                pending.push((path, depth + 1));
            } else if file_type.is_file()
                && let Ok(rel) = path.strip_prefix(&root)
            {
                set.insert(rel.to_path_buf());
            }
        }
    }
    set
}
