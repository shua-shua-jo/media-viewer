//! Moving files to the Recycle Bin / Trash, and (where the OS allows it) putting them back.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// A file we sent to the Recycle Bin, with enough information to find it there again.
#[derive(Debug, Clone)]
pub struct Deleted {
    /// Where the file was before it was deleted.
    pub path: PathBuf,
    /// When we deleted it (seconds since 1970), used to pick the right bin entry if several share a name.
    pub deleted_at_secs: i64,
}

/// Whether "undo delete" works on this system. The `trash` crate can list and restore bin entries
/// on Windows and Linux, but not on macOS.
#[cfg(any(windows, all(unix, not(target_os = "macos"))))]
pub const UNDO_SUPPORTED: bool = true;
#[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
pub const UNDO_SUPPORTED: bool = false;

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Sends `path` to the Recycle Bin / Trash.
pub fn delete(path: &Path) -> Result<Deleted, String> {
    let deleted_at_secs = now_secs();
    trash::delete(path).map_err(|e| e.to_string())?;
    Ok(Deleted {
        path: path.to_path_buf(),
        deleted_at_secs,
    })
}

/// Entries deleted up to this many seconds before our recorded time still count as "ours"
/// (clock rounding between us and the operating system).
#[cfg(any(windows, all(unix, not(target_os = "macos"))))]
const SLACK_SECS: i64 = 5;

/// Puts a file back from the Recycle Bin to where it was.
#[cfg(any(windows, all(unix, not(target_os = "macos"))))]
pub fn restore(item: &Deleted) -> Result<(), String> {
    use trash::os_limited;

    let parent = item.path.parent().ok_or("the file has no folder")?;
    let name = item.path.file_name().ok_or("the file has no name")?;

    let entries = os_limited::list().map_err(|e| e.to_string())?;
    let best = entries
        .into_iter()
        .filter(|t| {
            same_folder(&t.original_parent, parent)
                && names_match(&t.name, name)
                && t.time_deleted >= item.deleted_at_secs - SLACK_SECS
        })
        .max_by_key(|t| t.time_deleted);

    match best {
        Some(entry) => os_limited::restore_all([entry]).map_err(|e| e.to_string()),
        None => Err("it is no longer in the Recycle Bin (was it emptied or restored?)".to_string()),
    }
}

#[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
pub fn restore(_item: &Deleted) -> Result<(), String> {
    Err("undo isn't supported on this system; restore the file from the Trash instead".to_string())
}

fn normalize(p: &Path) -> String {
    p.to_string_lossy()
        .trim_end_matches(['/', '\\'])
        .to_lowercase()
}

/// Same folder, ignoring case and a trailing separator.
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn same_folder(a: &Path, b: &Path) -> bool {
    normalize(a) == normalize(b)
}

/// The bin may list a file with or without its extension (Windows can hide known extensions),
/// so accept either, ignoring case.
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn names_match(bin_name: &OsStr, file_name: &OsStr) -> bool {
    let bin = bin_name.to_string_lossy().to_lowercase();
    let full = file_name.to_string_lossy().to_lowercase();
    let stem = Path::new(file_name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    bin == full || bin == stem
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_with_or_without_extension_and_ignoring_case() {
        let file = OsStr::new("Holiday.JPG");
        assert!(names_match(OsStr::new("Holiday.JPG"), file));
        assert!(names_match(OsStr::new("holiday.jpg"), file));
        assert!(names_match(OsStr::new("Holiday"), file));
        assert!(!names_match(OsStr::new("Holiday2"), file));
        assert!(!names_match(OsStr::new("Other.jpg"), file));
    }

    #[test]
    fn folders_match_ignoring_case_and_trailing_separators() {
        assert!(same_folder(
            Path::new("C:\\Photos\\Trip"),
            Path::new("c:\\photos\\trip\\")
        ));
        assert!(same_folder(
            Path::new("/home/me/pics/"),
            Path::new("/home/me/pics")
        ));
        assert!(!same_folder(
            Path::new("/home/me/pics"),
            Path::new("/home/me/other")
        ));
    }
}
