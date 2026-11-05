use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::media::{Filter, Kind, load_favs, scan_folder};

/// Creates an empty, uniquely named folder under the system temp directory.
fn temp_dir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "media_viewer_test_{tag}_{}_{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn filter_rules() {
    assert!(Filter::All.matches(true));
    assert!(Filter::All.matches(false));
    assert!(Filter::Favorites.matches(true));
    assert!(!Filter::Favorites.matches(false));
    assert!(!Filter::NotFavorites.matches(true));
    assert!(Filter::NotFavorites.matches(false));
}

#[test]
fn scan_keeps_only_media_sorted_by_name() {
    let dir = temp_dir("scan");
    for name in ["b.jpg", "a.PNG", "notes.txt", "clip.mp4"] {
        fs::write(dir.join(name), b"x").unwrap();
    }
    // Files inside subfolders (like Favorites) must not be listed.
    fs::create_dir(dir.join("Favorites")).unwrap();
    fs::write(dir.join("Favorites").join("c.jpg"), b"x").unwrap();

    let items = scan_folder(&dir).unwrap();
    let names: Vec<String> = items
        .iter()
        .map(|m| m.path.file_name().unwrap().to_string_lossy().to_string())
        .collect();

    assert_eq!(names, ["a.PNG", "b.jpg", "clip.mp4"]);
    assert!(items[0].kind == Kind::Image); // upper-case extension still recognized
    assert!(items[2].kind == Kind::Video);

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn scan_empty_folder_gives_empty_list() {
    let dir = temp_dir("empty");
    assert!(scan_folder(&dir).unwrap().is_empty());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn scan_missing_folder_is_an_error() {
    assert!(scan_folder(Path::new("/this/folder/does/not/exist/xyz")).is_err());
}

#[test]
fn favorites_are_read_from_the_subfolder() {
    let dir = temp_dir("favs");
    fs::create_dir(dir.join("Favorites")).unwrap();
    fs::write(dir.join("Favorites").join("x.jpg"), b"x").unwrap();

    let favs = load_favs(&dir);
    assert_eq!(favs.len(), 1);
    assert!(favs.contains(std::ffi::OsStr::new("x.jpg")));

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn no_favorites_folder_means_no_favorites() {
    let dir = temp_dir("nofavs");
    assert!(load_favs(&dir).is_empty());
    fs::remove_dir_all(&dir).unwrap();
}
