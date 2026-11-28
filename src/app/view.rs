//! The per-frame UI: ties the toolbar, status bar and image area together.

use eframe::egui;

use super::App;
use super::preload::ImageState;
use crate::config::{MAX_ZOOM, MIN_ZOOM, ZOOM_STEP};
use crate::media::{Kind, Media};
use crate::widgets::draw_zoomable;

const KEYBINDS: &[(&str, &str)] = &[
    ("← / →", "Previous / next item"),
    ("F", "Add / remove favorite"),
    ("L / R", "Rotate view left / right"),
    ("Delete", "Move item to Recycle Bin / Trash"),
    ("Ctrl+Z", "Undo delete (Windows and Linux only)"),
    ("+ / =", "Zoom in"),
    ("-", "Zoom out"),
    ("0", "Fit image to window"),
    ("Space", "Play / pause video or animated GIF"),
    ("S", "Start / stop slideshow"),
    ("F11", "Toggle fullscreen"),
    ("Esc", "Close this window, or exit fullscreen"),
    ("F1", "Show / hide this window"),
];

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.fullscreen {
            self.goto_focused = false;
        }
        self.handle_shortcuts(&ctx);
        self.tick_slideshow(&ctx);
        self.handle_dropped_files(&ctx);

        let current = self.items.get(self.index).cloned();

        // Stop the embedded player when we're no longer on a video.
        if !current
            .as_ref()
            .map(|m| m.kind == Kind::Video)
            .unwrap_or(false)
        {
            self.player = None;
            self.player_failed = None;
        }

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
        self.update_animation(&ctx, &current);

        self.show_keybinds_window(&ctx);
        if !self.fullscreen {
            self.show_toolbar(ui, &current, is_fav);
            self.show_status_bar(ui, &current, img.loading);
        }
        self.show_central(ui, &current, &img);
        self.draw_fullscreen_hint(ui);

        // If a toolbar action changed what is shown, draw one more frame right away.
        if before != (self.items.len(), self.index, self.favs.len()) {
            ctx.request_repaint();
        }
        self.persist_settings(&ctx);
    }
}

impl App {
    fn show_keybinds_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("Keybinds")
            .open(&mut self.show_keybinds)
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(280.0)
                    .show(ui, |ui| {
                        egui::Grid::new("keybinds_grid")
                            .num_columns(2)
                            .spacing([24.0, 6.0])
                            .striped(true)
                            .show(ui, |ui| {
                                for (key, action) in KEYBINDS {
                                    ui.label(egui::RichText::new(*key).strong());
                                    ui.label(*action);
                                    ui.end_row();
                                }
                            });
                    });
                ui.separator();
                ui.label("Mouse: scroll or pinch to zoom, drag to pan, double-click to fit.");
                ui.label("Drag a file or folder onto the window to open it.");
                ui.label("Shortcuts are ignored while the jump-to-number box is focused.");
            });
    }

    fn show_status_bar(&mut self, ui: &mut egui::Ui, current: &Option<Media>, loading: bool) {
        egui::Panel::bottom("status").show(ui, |ui| {
            let name = current
                .as_ref()
                .map(|m| self.display_name(m))
                .unwrap_or_default();

            let loading_note = if loading { "  Loading…" } else { "" };

            let is_image = current
                .as_ref()
                .map(|m| m.kind == Kind::Image)
                .unwrap_or(false);

            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(name.to_string());
                if !self.status.is_empty() {
                    ui.separator();
                    ui.label(format!("{}{loading_note}", self.status));
                } else if loading {
                    ui.separator();
                    ui.label("Loading…");
                }

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
                let anim_tex = self.anim.as_ref().and_then(|a| a.texture());
                let tex = anim_tex.or(img.tex.as_ref()).or(if img.failed {
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
                self.show_video(ui, m);
            }
        });
    }
}
