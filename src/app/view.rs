//! The per-frame UI: ties the toolbar, status bar and image area together.

use eframe::egui;

use super::App;
use super::preload::ImageState;
use crate::config::{MAX_ZOOM, MIN_ZOOM, ZOOM_STEP};
use crate::media::{Kind, Media};
use crate::widgets::draw_zoomable;

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_shortcuts(&ctx);

        let current = self.items.get(self.index).cloned();

        // Each new photo/video starts at "fit to window".
        let cur_path = current.as_ref().map(|m| m.path.clone());
        if cur_path != self.zoom_path {
            self.reset_zoom();
            self.zoom_path = cur_path;
        }

        let is_fav = current
            .as_ref()
            .map(|m| self.is_favorite(m))
            .unwrap_or(false);
        // Snapshot used at the end of the frame to detect changes made from the toolbar.
        let before = (self.items.len(), self.index, self.favs.len());

        // Background loading: pick up finished images, then (re)plan what to keep and preload.
        let img = self.sync_images(&ctx, &current);

        self.show_toolbar(ui, &current, is_fav);
        self.show_status_bar(ui, &current, img.loading);
        self.show_central(ui, &current, &img);

        // If a toolbar action changed what is shown, draw one more frame right away.
        if before != (self.items.len(), self.index, self.favs.len()) {
            ctx.request_repaint();
        }
    }
}

impl App {
    fn show_status_bar(&mut self, ui: &mut egui::Ui, current: &Option<Media>, loading: bool) {
        egui::Panel::bottom("status").show(ui, |ui| {
            let name = current
                .as_ref()
                .and_then(|m| m.path.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("");

            let loading_note = if loading { "    Loading…" } else { "" };

            let is_image = current
                .as_ref()
                .map(|m| m.kind == Kind::Image)
                .unwrap_or(false);

            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(format!("{name}    {}{loading_note}", self.status));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button("Fit (0)")
                        .on_hover_text("Reset zoom (or double-click the image)")
                        .clicked()
                    {
                        self.reset_zoom();
                    }

                    ui.label(format!("{:.0}%", self.zoom * 100.0))
                        .on_hover_text("Relative to fit-to-window");

                    ui.add_enabled_ui(is_image, |ui| {
                        if ui.button("+").on_hover_text("Zoom in (+)").clicked() {
                            self.zoom_by(ZOOM_STEP);
                        }

                        ui.add(
                            egui::Slider::new(&mut self.zoom, MIN_ZOOM..=MAX_ZOOM)
                                .logarithmic(true)
                                .show_value(false),
                        );

                        if ui.button("-").on_hover_text("Zoom out (-)").clicked() {
                            self.zoom_by(1.0 / ZOOM_STEP);
                        }
                    });
                });
            });
        });
    }

    fn show_central(&mut self, ui: &mut egui::Ui, current: &Option<Media>, img: &ImageState) {
        egui::CentralPanel::default().show(ui, |ui| match current {
            None => {
                let msg = if self.all.is_empty() {
                    "No photos or videos to show."
                } else {
                    "Nothing matches this filter."
                };
                ui.centered_and_justified(|ui| ui.label(msg));
            }
            Some(m) if m.kind == Kind::Image => {
                // The current image if ready; otherwise the previous one (unless this one failed).
                let tex = img.tex.as_ref().or(if img.failed {
                    None
                } else {
                    self.shown.as_ref()
                });
                if let Some(tex) = tex {
                    draw_zoomable(ui, tex, &mut self.zoom, &mut self.prev_zoom, &mut self.pan);
                } else if img.failed {
                    ui.centered_and_justified(|ui| ui.label("Could not load this image."));
                } else {
                    ui.centered_and_justified(|ui| ui.label("Loading…"));
                }
            }
            Some(m) => {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() / 3.0);
                    ui.heading("🎬 Video");
                    if ui.button("▶ Play in default player").clicked() {
                        if let Err(e) = open::that(&m.path) {
                            self.status = format!("Could not open video: {e}");
                        }
                    }
                });
            }
        });
    }
}
