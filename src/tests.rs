//! Unit tests for folder scanning, favorites reading and the filter rules.
//! Run with:  cargo test

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::media::{
    Filter, Kind, Media, Sort, load_favs, natural_cmp, scan_folder, scan_limited, sort_media,
};

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

    let items = scan_folder(&dir, false).unwrap().items;
    let names: Vec<String> = items
        .iter()
        .map(|m| m.path.file_name().unwrap().to_string_lossy().to_string())
        .collect();

    assert_eq!(names, ["a.PNG", "b.jpg", "clip.mp4"]);
    assert!(items[0].kind == Kind::Image); // upper-case extension still recognised
    assert!(items[2].kind == Kind::Video);

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn scan_empty_folder_gives_empty_list() {
    let dir = temp_dir("empty");
    assert!(scan_folder(&dir, false).unwrap().items.is_empty());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn scan_missing_folder_is_an_error() {
    assert!(scan_folder(Path::new("/this/folder/does/not/exist/xyz"), false).is_err());
}

#[test]
fn favorites_are_read_from_the_subfolder() {
    let dir = temp_dir("favs");
    fs::create_dir(dir.join("Favorites")).unwrap();
    fs::write(dir.join("Favorites").join("x.jpg"), b"x").unwrap();

    let favs = load_favs(&dir);
    assert_eq!(favs.len(), 1);
    assert!(favs.contains(Path::new("x.jpg")));

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn no_favorites_folder_means_no_favorites() {
    let dir = temp_dir("nofavs");
    assert!(load_favs(&dir).is_empty());
    fs::remove_dir_all(&dir).unwrap();
}

// ---- sorting ----

fn media(name: &str, size: u64, modified_secs: Option<u64>) -> Media {
    let mut m = Media::new(PathBuf::from(name), Kind::Image);
    m.size = size;
    m.modified = modified_secs.map(|s| UNIX_EPOCH + Duration::from_secs(s));
    m
}

fn names(items: &[Media]) -> Vec<String> {
    items
        .iter()
        .map(|m| m.path.file_name().unwrap().to_string_lossy().to_string())
        .collect()
}

#[test]
fn natural_order_compares_numbers_and_ignores_case() {
    let mut v = vec!["img10.jpg", "IMG2.jpg", "img1.jpg", "Img03.jpg"];
    v.sort_by(|a, b| natural_cmp(a, b));
    assert_eq!(v, ["img1.jpg", "IMG2.jpg", "Img03.jpg", "img10.jpg"]);
}

#[test]
fn sort_by_name_both_directions() {
    let mut items = vec![
        media("b10.jpg", 1, None),
        media("b2.jpg", 1, None),
        media("a.jpg", 1, None),
    ];

    sort_media(&mut items, Sort::NameAsc);
    assert_eq!(names(&items), ["a.jpg", "b2.jpg", "b10.jpg"]);

    sort_media(&mut items, Sort::NameDesc);
    assert_eq!(names(&items), ["b10.jpg", "b2.jpg", "a.jpg"]);
}

#[test]
fn sort_by_size_both_directions() {
    let mut items = vec![
        media("a.jpg", 300, None),
        media("b.jpg", 100, None),
        media("c.jpg", 200, None),
    ];

    sort_media(&mut items, Sort::SizeAsc);
    assert_eq!(names(&items), ["b.jpg", "c.jpg", "a.jpg"]);

    sort_media(&mut items, Sort::SizeDesc);
    assert_eq!(names(&items), ["a.jpg", "c.jpg", "b.jpg"]);
}

#[test]
fn sort_by_date_treats_missing_dates_as_oldest() {
    let mut items = vec![
        media("a.jpg", 1, Some(200)),
        media("b.jpg", 1, None),
        media("c.jpg", 1, Some(100)),
    ];

    sort_media(&mut items, Sort::DateAsc);
    assert_eq!(names(&items), ["b.jpg", "c.jpg", "a.jpg"]);

    sort_media(&mut items, Sort::DateDesc);
    assert_eq!(names(&items), ["a.jpg", "c.jpg", "b.jpg"]);
}

#[test]
fn equal_sizes_fall_back_to_name_order() {
    let mut items = vec![
        media("c.jpg", 5, None),
        media("a.jpg", 5, None),
        media("b.jpg", 5, None),
    ];
    sort_media(&mut items, Sort::SizeAsc);
    assert_eq!(names(&items), ["a.jpg", "b.jpg", "c.jpg"]);
}

// ---- subfolders ----

fn names_of(items: &[Media]) -> Vec<String> {
    let mut v: Vec<String> = items
        .iter()
        .map(|m| m.path.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    v.sort();
    v
}

#[test]
fn recursive_scan_includes_subfolders_but_skips_favorites_hidden_and_other_files() {
    let dir = temp_dir("recursive");
    let write = |rel: &str| {
        let path = dir.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"x").unwrap();
    };
    write("a.jpg");
    write("sub/b.jpg");
    write("sub/deeper/c.png");
    write("sub/notes.txt");
    write(".hidden/d.jpg");
    write("Favorites/e.jpg");
    write("sub/Favorites/f.jpg");

    let flat = scan_folder(&dir, false).unwrap();
    assert_eq!(names_of(&flat.items), ["a.jpg"]);

    let deep = scan_folder(&dir, true).unwrap();
    assert_eq!(names_of(&deep.items), ["a.jpg", "b.jpg", "c.png"]);
    assert!(!deep.truncated);

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn scan_stops_at_the_file_limit_and_says_so() {
    let dir = temp_dir("limit");
    for i in 0..5 {
        fs::write(dir.join(format!("{i}.jpg")), b"x").unwrap();
    }

    let capped = scan_limited(&dir, false, 3).unwrap();
    assert_eq!(capped.items.len(), 3);
    assert!(capped.truncated);

    // Exactly at the limit is not "truncated".
    let exact = scan_limited(&dir, false, 5).unwrap();
    assert_eq!(exact.items.len(), 5);
    assert!(!exact.truncated);

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn name_sort_groups_files_by_folder_first() {
    let root = PathBuf::from("root");
    let mut items = vec![
        Media::new(root.join("b").join("img1.jpg"), Kind::Image),
        Media::new(root.join("a").join("img10.jpg"), Kind::Image),
        Media::new(root.join("a").join("img2.jpg"), Kind::Image),
    ];

    sort_media(&mut items, Sort::NameAsc);

    let order: Vec<PathBuf> = items.iter().map(|m| m.path.clone()).collect();
    assert_eq!(
        order,
        [
            root.join("a").join("img2.jpg"),
            root.join("a").join("img10.jpg"),
            root.join("b").join("img1.jpg"),
        ]
    );
}

#[test]
fn favorites_in_subfolders_are_read_as_relative_paths() {
    let dir = temp_dir("favs_nested");
    fs::create_dir_all(dir.join("Favorites").join("Day1")).unwrap();
    fs::write(dir.join("Favorites").join("top.jpg"), b"x").unwrap();
    fs::write(dir.join("Favorites").join("Day1").join("nested.jpg"), b"x").unwrap();

    let favs = load_favs(&dir);

    assert_eq!(favs.len(), 2);
    assert!(favs.contains(Path::new("top.jpg")));
    assert!(favs.contains(&PathBuf::from("Day1").join("nested.jpg")));

    fs::remove_dir_all(&dir).unwrap();
}
