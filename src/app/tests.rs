use std::ffi::OsString;
use std::path::PathBuf;

use eframe::egui;

use super::App;
use crate::media::{Filter, Kind, Media};

/// An App (with no folder opened) whose file list is exactly `names`.
fn app_with(names: &[&str]) -> App {
    let mut app = App::new(None, egui::Context::default());
    app.all = names
        .iter()
        .map(|n| Media {
            path: PathBuf::from(n),
            kind: Kind::Image,
        })
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
