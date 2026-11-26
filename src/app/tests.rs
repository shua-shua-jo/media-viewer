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

// ---- slideshow ----

fn app_with_kinds(files: &[(&str, Kind)]) -> App {
    let mut app = App::new(None, egui::Context::default(), Settings::default());
    app.all = files
        .iter()
        .map(|(name, kind)| Media::new(PathBuf::from(name), *kind))
        .collect();
    app.apply_filter(None);
    app
}

#[test]
fn slideshow_advance_skips_videos_and_wraps_around() {
    let mut app = app_with_kinds(&[
        ("a.jpg", Kind::Image),
        ("b.mp4", Kind::Video),
        ("c.jpg", Kind::Image),
    ]);

    app.advance_slideshow();
    assert_eq!(app.index, 2); // skipped the video

    app.advance_slideshow();
    assert_eq!(app.index, 0); // wrapped around
}

#[test]
fn slideshow_does_not_start_when_there_are_only_videos() {
    let mut app = app_with_kinds(&[("a.mp4", Kind::Video), ("b.mov", Kind::Video)]);

    app.toggle_slideshow();

    assert!(!app.slideshow);
}

#[test]
fn starting_the_slideshow_on_a_video_jumps_to_the_next_photo() {
    let mut app = app_with_kinds(&[("a.mp4", Kind::Video), ("b.jpg", Kind::Image)]);

    app.toggle_slideshow();

    assert!(app.slideshow);
    assert_eq!(app.index, 1);
}

#[test]
fn toggling_the_slideshow_twice_stops_it() {
    let mut app = app_with_kinds(&[("a.jpg", Kind::Image), ("b.jpg", Kind::Image)]);

    app.toggle_slideshow();
    assert!(app.slideshow);
    app.toggle_slideshow();
    assert!(!app.slideshow);
}

// ---- undo delete ----

#[test]
fn restored_file_is_added_back_and_selected() {
    let dir = temp_dir("undo");
    for name in ["a.jpg", "b.jpg", "c.jpg"] {
        fs::write(dir.join(name), b"x").unwrap();
    }
    let mut app = App::new(None, egui::Context::default(), Settings::default());
    app.open_path(dir.clone());

    // Pretend b.jpg was deleted (removed from the lists) and then restored on disk.
    app.all.retain(|m| m.path.file_name().unwrap() != "b.jpg");
    app.apply_filter(None);
    assert_eq!(app.items.len(), 2);

    app.reinsert(&app.folder.clone().unwrap().join("b.jpg"));

    assert_eq!(app.items.len(), 3);
    assert_eq!(current_name(&app), "b.jpg");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn restoring_into_a_folder_that_is_not_open_changes_nothing() {
    let open_dir = temp_dir("undo_open");
    let other_dir = temp_dir("undo_other");
    fs::write(open_dir.join("a.jpg"), b"x").unwrap();
    fs::write(other_dir.join("z.jpg"), b"x").unwrap();
    let mut app = App::new(None, egui::Context::default(), Settings::default());
    app.open_path(open_dir.clone());

    app.reinsert(&other_dir.join("z.jpg"));

    assert_eq!(app.items.len(), 1);
    fs::remove_dir_all(&open_dir).unwrap();
    fs::remove_dir_all(&other_dir).unwrap();
}

#[test]
fn only_the_most_recent_deletions_are_remembered() {
    use crate::config::UNDO_LIMIT;
    use crate::recycle::{self, Deleted};

    let mut app = App::new(None, egui::Context::default(), Settings::default());
    for i in 0..(UNDO_LIMIT + 5) {
        app.remember_deleted(Deleted {
            path: PathBuf::from(format!("{i}.jpg")),
            deleted_at_secs: i as i64,
        });
    }

    if recycle::UNDO_SUPPORTED {
        assert_eq!(app.undo_stack.len(), UNDO_LIMIT);
        assert_eq!(
            app.undo_stack.last().unwrap().path,
            PathBuf::from(format!("{}.jpg", UNDO_LIMIT + 4))
        );
    } else {
        assert!(app.undo_stack.is_empty());
    }
}
