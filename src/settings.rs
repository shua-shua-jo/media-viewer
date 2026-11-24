//! Remembered settings: last folder, filter, sort order and window size.
//!
//! Stored as a small `key=value` text file in the user's config folder:
//!   Windows: %APPDATA%\media-viewer\settings.txt
//!   macOS:   ~/Library/Application Support/media-viewer/settings.txt
//!   Linux:   $XDG_CONFIG_HOME/media-viewer/settings.txt (or ~/.config/media-viewer/settings.txt)
//! Deleting the file resets everything to the defaults.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::media::{Filter, Sort};

/// Window sizes outside this range (in points) are treated as corrupt and ignored.
const MIN_SANE_SIZE: u32 = 200;
const MAX_SANE_SIZE: u32 = 20_000;

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub last_folder: Option<PathBuf>,
    pub filter: Filter,
    pub sort: Sort,
    /// Window size in points (not saved while maximized or fullscreen).
    pub window_size: Option<(u32, u32)>,
    pub maximized: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            last_folder: None,
            filter: Filter::All,
            sort: Sort::NameAsc,
            window_size: None,
            maximized: false,
        }
    }
}

impl Settings {
    /// Loads the saved settings, or the defaults if there are none (or they can't be read).
    pub fn load() -> Settings {
        settings_path()
            .map(|p| Settings::load_from(&p))
            .unwrap_or_default()
    }

    pub fn load_from(path: &Path) -> Settings {
        fs::read_to_string(path)
            .map(|text| parse(&text))
            .unwrap_or_default()
    }

    pub fn save(&self) -> io::Result<()> {
        match settings_path() {
            Some(path) => self.save_to(&path),
            None => Ok(()), // no known config folder: nothing to do
        }
    }

    /// Writes to a temporary file first, so a crash can't leave a half-written settings file.
    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, self.to_text())?;
        fs::rename(&tmp, path)
    }

    fn to_text(&self) -> String {
        let mut out =
            String::from("# Media Viewer settings. Safe to delete; the app recreates it.\n");
        if let Some(folder) = self.last_folder.as_ref().and_then(|p| p.to_str())
            && !folder.contains('\n')
        {
            out.push_str(&format!("last_folder={folder}\n"));
        }
        out.push_str(&format!("filter={}\n", filter_key(self.filter)));
        out.push_str(&format!("sort={}\n", sort_key(self.sort)));
        if let Some((w, h)) = self.window_size {
            out.push_str(&format!("window_width={w}\nwindow_height={h}\n"));
        }
        out.push_str(&format!("maximized={}\n", self.maximized));
        out
    }
}

fn parse(text: &str) -> Settings {
    let mut s = Settings::default();
    let (mut width, mut height) = (None, None);

    for line in text.lines() {
        if line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "last_folder" => {
                if !value.is_empty() {
                    s.last_folder = Some(PathBuf::from(value));
                }
            }
            "filter" => {
                if let Some(f) = filter_from(value.trim()) {
                    s.filter = f;
                }
            }
            "sort" => {
                if let Some(v) = sort_from(value.trim()) {
                    s.sort = v;
                }
            }
            "window_width" => width = value.trim().parse::<u32>().ok(),
            "window_height" => height = value.trim().parse::<u32>().ok(),
            "maximized" => s.maximized = value.trim() == "true",
            _ => {} // unknown keys (for example from a newer version) are ignored
        }
    }

    if let (Some(w), Some(h)) = (width, height) {
        let sane = MIN_SANE_SIZE..=MAX_SANE_SIZE;
        if sane.contains(&w) && sane.contains(&h) {
            s.window_size = Some((w, h));
        }
    }
    s
}

fn filter_key(f: Filter) -> &'static str {
    match f {
        Filter::All => "all",
        Filter::Favorites => "favorites",
        Filter::NotFavorites => "not_favorites",
    }
}

fn filter_from(s: &str) -> Option<Filter> {
    match s {
        "all" => Some(Filter::All),
        "favorites" => Some(Filter::Favorites),
        "not_favorites" => Some(Filter::NotFavorites),
        _ => None,
    }
}

fn sort_key(s: Sort) -> &'static str {
    match s {
        Sort::NameAsc => "name_asc",
        Sort::NameDesc => "name_desc",
        Sort::DateAsc => "date_asc",
        Sort::DateDesc => "date_desc",
        Sort::SizeAsc => "size_asc",
        Sort::SizeDesc => "size_desc",
    }
}

fn sort_from(s: &str) -> Option<Sort> {
    match s {
        "name_asc" => Some(Sort::NameAsc),
        "name_desc" => Some(Sort::NameDesc),
        "date_asc" => Some(Sort::DateAsc),
        "date_desc" => Some(Sort::DateDesc),
        "size_asc" => Some(Sort::SizeAsc),
        "size_desc" => Some(Sort::SizeDesc),
        _ => None,
    }
}

#[cfg(windows)]
fn config_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(PathBuf::from)
}

#[cfg(target_os = "macos")]
fn config_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
}

fn settings_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("media-viewer").join("settings.txt"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn sample() -> Settings {
        Settings {
            last_folder: Some(PathBuf::from("/photos/trip = 2026")),
            filter: Filter::Favorites,
            sort: Sort::DateDesc,
            window_size: Some((1100, 720)),
            maximized: true,
        }
    }

    #[test]
    fn text_round_trip_keeps_everything() {
        let s = sample();
        assert_eq!(parse(&s.to_text()), s);
    }

    #[test]
    fn unknown_keys_comments_and_garbage_are_ignored() {
        let s = parse("# comment\nnonsense\nfuture_setting=1\nfilter=favorites\n");
        assert!(s.filter == Filter::Favorites);
        assert_eq!(s.sort, Sort::NameAsc);
    }

    #[test]
    fn invalid_values_fall_back_to_the_defaults() {
        let s = parse(
            "filter=banana\nsort=sideways\nwindow_width=5\nwindow_height=700\nmaximized=maybe\n",
        );
        assert_eq!(s, Settings::default());
    }

    #[test]
    fn absurd_window_sizes_are_dropped() {
        let s = parse("window_width=99999\nwindow_height=700\n");
        assert_eq!(s.window_size, None);
    }

    #[test]
    fn save_and_load_through_a_file() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "media_viewer_settings_test_{}_{nanos}",
            std::process::id()
        ));
        let path = dir.join("settings.txt"); // the folder doesn't exist yet; save_to creates it

        let s = sample();
        s.save_to(&path).unwrap();
        assert_eq!(Settings::load_from(&path), s);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_file_gives_the_defaults() {
        assert_eq!(
            Settings::load_from(Path::new("/this/file/does/not/exist.txt")),
            Settings::default()
        );
    }
}
