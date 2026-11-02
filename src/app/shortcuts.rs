//! Keyboard shortcuts.

use eframe::egui;

use super::App;
use crate::config::ZOOM_STEP;

impl App {
    /// Left/Right: previous/next   F: favorite   R/L: rotate view   Delete: recycle bin
    /// + / - : zoom   0: fit to window
    pub(super) fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        // Don't treat keys as shortcuts while the user is typing in the number box.
        if self.goto_focused {
            return;
        }

        let (left, right, fav, del, rot_r, rot_l, zoom_in, zoom_out, zoom_fit) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::ArrowRight),
                i.key_pressed(egui::Key::F),
                i.key_pressed(egui::Key::Delete),
                i.key_pressed(egui::Key::R),
                i.key_pressed(egui::Key::L),
                i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals),
                i.key_pressed(egui::Key::Minus),
                i.key_pressed(egui::Key::Num0),
            )
        });

        if left {
            self.step(-1);
        }
        if right {
            self.step(1);
        }
        if fav {
            self.toggle_favorite();
        }
        if rot_r {
            self.rotate_current(true);
        }
        if rot_l {
            self.rotate_current(false);
        }
        if del {
            self.delete_current();
        }
        if zoom_in {
            self.zoom_by(ZOOM_STEP);
        }
        if zoom_out {
            self.zoom_by(1.0 / ZOOM_STEP);
        }
        if zoom_fit {
            self.reset_zoom();
        }
    }
}
