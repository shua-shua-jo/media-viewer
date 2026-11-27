//! The top toolbar: open folder, filter, sort, navigation, jump-to-number, favorite, rotate, zoom, delete.

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
            ui.horizontal_wrapped(|ui| {
                if ui.button("📂 Open folder").clicked()
                    && let Some(dir) = rfd::FileDialog::new().pick_folder()
                {
                    self.load_folder(dir);
                }
                ui.separator();
                let subfolders_before = self.include_subfolders;
                ui.checkbox(&mut self.include_subfolders, "Subfolders")
                    .on_hover_text("Also show photos and videos in subfolders");
                if self.include_subfolders != subfolders_before {
                    self.rescan(current.as_ref().map(|m| m.path.clone()));
                }
                ui.separator();
                if ui.button("Open file").clicked() {
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
                ui.separator();
                let before = self.filter;
                ui.selectable_value(&mut self.filter, Filter::All, "All");
                ui.selectable_value(&mut self.filter, Filter::Favorites, "Favorites");
                ui.selectable_value(&mut self.filter, Filter::NotFavorites, "Not favorites");
                if self.filter != before {
                    self.apply_filter(current.as_ref().map(|m| m.path.clone()));
                }
                ui.separator();
                let sort_before = self.sort;
                egui::ComboBox::from_id_salt("sort")
                    .selected_text(self.sort.label())
                    .show_ui(ui, |ui| {
                        for s in Sort::ALL {
                            ui.selectable_value(&mut self.sort, s, s.label());
                        }
                    });
                if self.sort != sort_before {
                    self.resort(current.as_ref().map(|m| m.path.clone()));
                }
                ui.separator();
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
                                self.status =
                                    format!("Enter a number from 1 to {}.", self.items.len());
                            }
                        }
                        ui.ctx().request_repaint();
                    }
                    ui.label(format!("/ {}", self.items.len()));
                }
                ui.separator();
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
                ui.separator();
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
                ui.separator();
                if ui.button("Fullscreen (F11)").clicked() {
                    self.toggle_fullscreen(ui.ctx());
                }
                let slideshow_label = if self.slideshow {
                    "Stop slideshow (S)"
                } else {
                    "Slideshow (S)"
                };
                if ui.button(slideshow_label).clicked() {
                    self.toggle_slideshow();
                }
                let secs_before = self.slideshow_secs;
                egui::ComboBox::from_id_salt("slideshow_secs")
                    .selected_text(format!("{} s", self.slideshow_secs))
                    .show_ui(ui, |ui| {
                        for secs in SLIDESHOW_CHOICES {
                            ui.selectable_value(
                                &mut self.slideshow_secs,
                                secs,
                                format!("{secs} s"),
                            );
                        }
                    });
                if self.slideshow_secs != secs_before {
                    self.restart_slideshow_timer();
                }
                ui.separator();
                if ui
                    .add_enabled(current.is_some(), egui::Button::new("🗑 Delete (Del)"))
                    .clicked()
                {
                    self.delete_current();
                }
                ui.separator();
                if ui
                    .add_enabled(
                        recycle::UNDO_SUPPORTED && !self.undo_stack.is_empty(),
                        egui::Button::new("Undo delete (Ctrl+Z)"),
                    )
                    .clicked()
                {
                    self.undo_delete();
                }
            });
        });
    }
}
