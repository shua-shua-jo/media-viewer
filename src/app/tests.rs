//! Unit tests for navigation and filtering. Run with:  cargo test

use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use eframe::egui;

use super::App;
use crate::media::{Filter, Kind, Media, Sort};
use crate::settings::Settings;

/// An App (with no folder opened) whose file list is exactly `names`.
fn app_with(names: &[&str]) -> App {
    let mut app = App::new(None, egui::Context::default(), Settings::default());
    app.all = names
        .iter()
        .map(|n| Media::new(PathBuf::from(n), Kind::Image))
        .collect();
    app.apply_filter(None);
    app
}

#[test]
fn step_wraps_around_both_ends() {
    let mut app = app_with(&["a.jpg", "b.jpg", "c.jpg"]);
    app.step(-1);
    assert_eq!(app.index, 2);
    app.step(1);
    assert_eq!(app.index, 0);
    app.step(4);
    assert_eq!(app.index, 1);
}

#[test]
fn step_on_empty_list_does_nothing() {
    let mut app = app_with(&[]);
    app.step(1);
    app.step(-1);
    assert_eq!(app.index, 0);
}

#[test]
fn favorites_filter_shows_only_favorites_and_keeps_position() {
    let mut app = app_with(&["a.jpg", "b.jpg", "c.jpg"]);
    app.favs.insert(OsString::from("b.jpg"));
    app.favs.insert(OsString::from("c.jpg"));
    app.index = 2; // looking at c.jpg

    app.filter = Filter::Favorites;
    app.apply_filter(Some(PathBuf::from("c.jpg")));

    assert_eq!(app.items.len(), 2);
    assert_eq!(app.items[app.index].path, PathBuf::from("c.jpg"));
}

#[test]
fn not_favorites_filter_hides_favorites() {
    let mut app = app_with(&["a.jpg", "b.jpg", "c.jpg"]);
    app.favs.insert(OsString::from("b.jpg"));

    app.filter = Filter::NotFavorites;
    app.apply_filter(None);

    let names: Vec<PathBuf> = app.items.iter().map(|m| m.path.clone()).collect();
    assert_eq!(names, [PathBuf::from("a.jpg"), PathBuf::from("c.jpg")]);
}

#[test]
fn filter_that_matches_nothing_leaves_an_empty_list_at_index_zero() {
    let mut app = app_with(&["a.jpg", "b.jpg"]);
    app.index = 1;

    app.filter = Filter::Favorites; // there are no favorites
    app.apply_filter(None);

    assert!(app.items.is_empty());
    assert_eq!(app.index, 0);
}

#[test]
fn position_stays_near_the_same_spot_when_the_current_item_disappears() {
    let mut app = app_with(&["a.jpg", "b.jpg", "c.jpg", "d.jpg"]);
    app.favs.insert(OsString::from("a.jpg"));
    app.favs.insert(OsString::from("b.jpg"));
    app.favs.insert(OsString::from("c.jpg"));
    app.filter = Filter::Favorites;
    app.apply_filter(None);
    app.index = 2; // c.jpg

    // "Un-favorite" c.jpg: it leaves the Favorites view, and we should land on the nearest valid spot.
    app.favs.remove(&OsString::from("c.jpg"));
    app.apply_filter(Some(PathBuf::from("c.jpg")));

    assert_eq!(app.items.len(), 2);
    assert!(app.index < app.items.len());
}

/// Creates an empty, uniquely named folder under the system temp directory.
fn temp_dir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "media_viewer_app_test_{tag}_{}_{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn current_name(app: &App) -> String {
    app.items[app.index]
        .path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string()
}

#[test]
fn opening_a_file_shows_its_folder_and_selects_the_file() {
    let dir = temp_dir("openfile");
    for name in ["a.jpg", "b.jpg", "c.jpg"] {
        fs::write(dir.join(name), b"x").unwrap();
    }
    let mut app = App::new(None, egui::Context::default(), Settings::default());

    app.open_path(dir.join("b.jpg"));

    assert_eq!(app.items.len(), 3);
    assert_eq!(current_name(&app), "b.jpg");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn opening_a_file_resets_a_filter_that_would_hide_it() {
    let dir = temp_dir("openfilter");
    fs::write(dir.join("a.jpg"), b"x").unwrap();
    let mut app = App::new(None, egui::Context::default(), Settings::default());
    app.filter = Filter::Favorites; // nothing is a favorite, so this would hide everything

    app.open_path(dir.join("a.jpg"));

    assert!(app.filter == Filter::All);
    assert_eq!(app.items.len(), 1);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn opening_a_folder_starts_at_the_first_item() {
    let dir = temp_dir("openfolder");
    for name in ["a.jpg", "b.jpg"] {
        fs::write(dir.join(name), b"x").unwrap();
    }
    let mut app = App::new(None, egui::Context::default(), Settings::default());

    app.open_path(dir.clone());

    assert_eq!(app.items.len(), 2);
    assert_eq!(app.index, 0);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn opening_an_unsupported_file_still_shows_its_folder() {
    let dir = temp_dir("openother");
    fs::write(dir.join("a.jpg"), b"x").unwrap();
    fs::write(dir.join("notes.txt"), b"x").unwrap();
    let mut app = App::new(None, egui::Context::default(), Settings::default());

    app.open_path(dir.join("notes.txt"));

    assert_eq!(app.items.len(), 1);
    assert_eq!(app.index, 0);
    assert!(app.status.contains("isn't a supported"));
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn opening_a_missing_path_reports_an_error_and_does_not_crash() {
    let mut app = App::new(None, egui::Context::default(), Settings::default());

    app.open_path(PathBuf::from("/this/path/does/not/exist/photo.jpg"));

    assert!(app.items.is_empty());
    assert!(app.status.contains("not found"));
}

#[test]
fn changing_the_sort_keeps_the_current_photo_selected() {
    let mut app = app_with(&["a.jpg", "b.jpg", "c.jpg"]);
    app.index = 0; // looking at a.jpg

    app.sort = Sort::NameDesc;
    app.resort(Some(PathBuf::from("a.jpg")));

    let order: Vec<PathBuf> = app.items.iter().map(|m| m.path.clone()).collect();
    assert_eq!(
        order,
        [
            PathBuf::from("c.jpg"),
            PathBuf::from("b.jpg"),
            PathBuf::from("a.jpg")
        ]
    );
    assert_eq!(app.items[app.index].path, PathBuf::from("a.jpg"));
}
