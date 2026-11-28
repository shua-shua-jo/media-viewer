//! The top toolbar. Kept small on purpose: rarely used options live in the File and View menus.
//!
//!   File ▾  View ▾  [filter ▾]  |  ◀ ▶ [n] / total  |  ♥ ↺ ↻  |  Delete  Undo
//!
//! Zoom controls and file details live in the bottom bar (see `view.rs`).

use eframe::egui;

use super::App;
use crate::config::{IMAGE_EXTS, SLIDESHOW_CHOICES, VIDEO_EXTS};
use crate::media::{Filter, Kind, Media, Sort};
use crate::recycle;
use crate::widgets::{Arrow, arrow_button, heart_button};

impl App {
    pub(super) fn show_toolbar(
        &mut self,
        ui: &mut egui::Ui,
        current: &Option<Media>,
        is_fav: bool,
    ) {
        egui::Panel::top("toolbar").show(ui, |ui| {
            // Wrapped, so on a narrow window the groups flow onto a second line instead of being cut off.
            ui.horizontal_wrapped(|ui| {
                self.file_menu(ui);
                self.view_menu(ui, current);
                self.filter_combo(ui, current);
                self.sort_combo(ui, current);
                ui.separator();
                self.navigation(ui);
                ui.separator();
                self.item_actions(ui, current, is_fav);
                ui.separator();
                self.delete_controls(ui, current);
            });
        });
    }

    /// File ▾ : open a folder or a single file.
    fn file_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("File", |ui| {
            if ui.button("Open folder...").clicked()
                && let Some(dir) = rfd::FileDialog::new().pick_folder()
            {
                self.load_folder(dir);
            }
            if ui.button("Open file...").clicked() {
                let exts: Vec<&str> = IMAGE_EXTS
                    .iter()
                    .chain(VIDEO_EXTS.iter())
                    .copied()
                    .collect();
                if let Some(file) = rfd::FileDialog::new()
                    .add_filter("Photos and videos", &exts)
                    .pick_file()
                {
                    self.open_path(file);
                }
            }
            if ui.button("Keybinds").clicked() {
                self.show_keybinds = true;
            }
        });
    }

    /// View ▾ : sort order, subfolders, fullscreen and slideshow.
    fn view_menu(&mut self, ui: &mut egui::Ui, current: &Option<Media>) {
        let keep = || current.as_ref().map(|m| m.path.clone());
        // let sort_before = self.sort;
        let subfolders_before = self.include_subfolders;

        ui.menu_button("View", |ui| {
            // ui.label(egui::RichText::new("Sort by").weak());
            // for s in Sort::ALL {
            //     if ui.selectable_label(self.sort == s, s.label()).clicked() {
            //         self.sort = s;
            //     }
            // }

            // ui.separator();
            ui.checkbox(&mut self.include_subfolders, "Include subfolders")
                .on_hover_text("Also show photos and videos in subfolders");

            ui.separator();
            if ui
                .add(egui::Button::new("Fullscreen").shortcut_text("F11"))
                .clicked()
            {
                self.toggle_fullscreen(ui.ctx());
            }
            let slideshow_label = if self.slideshow {
                "Stop slideshow"
            } else {
                "Start slideshow"
            };
            if ui
                .add(egui::Button::new(slideshow_label).shortcut_text("S"))
                .clicked()
            {
                self.toggle_slideshow();
            }
            ui.menu_button("Slideshow interval", |ui| {
                for secs in SLIDESHOW_CHOICES {
                    if ui
                        .selectable_label(self.slideshow_secs == secs, format!("{secs} seconds"))
                        .clicked()
                    {
                        self.slideshow_secs = secs;
                        self.restart_slideshow_timer();
                    }
                }
            });
        });

        // // The menu closure has already run by now, so react to what it changed.
        // if self.sort != sort_before {
        //     self.resort(keep());
        // }
        if self.include_subfolders != subfolders_before {
            self.rescan(keep());
        }
    }

    /// Which photos to show. Kept visible (not in a menu) so an active filter is never a surprise.
    fn filter_combo(&mut self, ui: &mut egui::Ui, current: &Option<Media>) {
        let before = self.filter;
        let selected = match self.filter {
            Filter::All => "All",
            Filter::Favorites => "Favorites",
            Filter::NotFavorites => "Not favorites",
        };
        egui::ComboBox::from_id_salt("filter")
            .selected_text(selected)
            .width(100.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.filter, Filter::All, "All");
                ui.selectable_value(&mut self.filter, Filter::Favorites, "Favorites");
                ui.selectable_value(&mut self.filter, Filter::NotFavorites, "Not favorites");
            })
            .response
            .on_hover_text("Which photos and videos to show");
        if self.filter != before {
            self.apply_filter(current.as_ref().map(|m| m.path.clone()));
        }
    }

    /// Previous / next and the jump-to-number box.
    fn navigation(&mut self, ui: &mut egui::Ui) {
        if arrow_button(ui, Arrow::Left, "Previous (←)").clicked() {
            self.step(-1);
        }
        if arrow_button(ui, Arrow::Right, "Next (→)").clicked() {
            self.step(1);
        }
        if !self.items.is_empty() {
            // Keep the box in sync with the current position unless the user is typing.
            if !self.goto_focused {
                self.goto_text = (self.index + 1).to_string();
            }
            let resp = ui
                .add(egui::TextEdit::singleline(&mut self.goto_text).desired_width(44.0))
                .on_hover_text("Type a number and press Enter to jump");
            self.goto_focused = resp.has_focus();
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                match self.goto_text.trim().parse::<usize>() {
                    Ok(n) if n >= 1 && n <= self.items.len() => {
                        self.index = n - 1;
                        self.status = format!("Jumped to {n}.");
                    }
                    _ => {
                        self.status = format!("Enter a number from 1 to {}.", self.items.len());
                    }
                }
                ui.ctx().request_repaint();
            }
            ui.label(format!("/ {}", self.items.len()));
        }
    }

    /// Favorite heart and rotate buttons.
    fn item_actions(&mut self, ui: &mut egui::Ui, current: &Option<Media>, is_fav: bool) {
        ui.add_enabled_ui(current.is_some(), |ui| {
            if heart_button(ui, is_fav)
                .on_hover_text(if is_fav {
                    "Remove from favorites (F)"
                } else {
                    "Add to favorites (F)"
                })
                .clicked()
            {
                self.toggle_favorite();
            }
        });

        let is_image = current
            .as_ref()
            .map(|m| m.kind == Kind::Image)
            .unwrap_or(false);
        if ui
            .add_enabled(is_image, egui::Button::new("↺"))
            .on_hover_text("Rotate left (L)")
            .clicked()
        {
            self.rotate_current(false);
        }
        if ui
            .add_enabled(is_image, egui::Button::new("↻"))
            .on_hover_text("Rotate right (R)")
            .clicked()
        {
            self.rotate_current(true);
        }
    }

    /// Delete and undo, plus a stop button that only appears while a slideshow is running.
    fn delete_controls(&mut self, ui: &mut egui::Ui, current: &Option<Media>) {
        if ui
            .add_enabled(current.is_some(), egui::Button::new("🗑 Delete"))
            .on_hover_text("Move to the Recycle Bin (Del)")
            .clicked()
        {
            self.delete_current();
        }
        if ui
            .add_enabled(
                recycle::UNDO_SUPPORTED && !self.undo_stack.is_empty(),
                egui::Button::new("Undo"),
            )
            .on_hover_text("Undo delete (Ctrl+Z)")
            .clicked()
        {
            self.undo_delete();
        }

        if self.slideshow {
            ui.separator();
            if ui.button("Stop slideshow").clicked() {
                self.toggle_slideshow();
            }
        }
    }

    /// How the list is ordered. Visible in the toolbar so the order is never a surprise.
    fn sort_combo(&mut self, ui: &mut egui::Ui, current: &Option<Media>) {
        let before = self.sort;
        egui::ComboBox::from_id_salt("sort")
            .selected_text(self.sort.short_label())
            .width(110.0)
            .show_ui(ui, |ui| {
                for s in Sort::ALL {
                    ui.selectable_value(&mut self.sort, s, s.label());
                }
            })
            .response
            .on_hover_text("Sort order");
        if self.sort != before {
            self.resort(current.as_ref().map(|m| m.path.clone()));
        }
    }
}
