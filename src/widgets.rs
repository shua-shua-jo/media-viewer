//! Reusable drawing code: the zoomable image area and the heart button.

use eframe::egui;

use crate::config::{HEART_RED, MAX_ZOOM, MIN_ZOOM, SCROLL_ZOOM_SPEED};

/// Draws the image filling the available area with zoom (mouse wheel / pinch), drag-to-pan and
/// double-click-to-reset. Zooming only changes how the already-uploaded texture is drawn on the GPU,
/// so it costs essentially nothing.
pub fn draw_zoomable(
    ui: &mut egui::Ui,
    tex: &egui::TextureHandle,
    zoom: &mut f32,
    prev_zoom: &mut f32,
    pan: &mut egui::Vec2,
) {
    let (rect, resp) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
    let tex_size = tex.size_vec2();
    // Scale that makes the whole image fit (never enlarges small images past 100% when "fit").
    let fit = (rect.width() / tex_size.x)
        .min(rect.height() / tex_size.y)
        .min(1.0);

    // The toolbar slider/buttons changed the zoom: keep the middle of the view where it was.
    if (*zoom - *prev_zoom).abs() > f32::EPSILON && *prev_zoom > 0.0 {
        *pan *= *zoom / *prev_zoom;
    }

    // Mouse wheel / trackpad pinch: zoom towards the cursor.
    if resp.hovered() {
        let (scroll, pinch, pointer) = ui.input(|i| {
            (
                i.smooth_scroll_delta.y,
                i.zoom_delta(),
                i.pointer.hover_pos(),
            )
        });
        let factor = pinch * (scroll * SCROLL_ZOOM_SPEED).exp();
        if (factor - 1.0).abs() > 1e-4 {
            let new_zoom = (*zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
            let ratio = new_zoom / *zoom;
            if let Some(p) = pointer {
                let center = rect.center() + *pan;
                let new_center = p - (p - center) * ratio;
                *pan = new_center - rect.center();
            }
            *zoom = new_zoom;
        }
    }

    if resp.dragged() {
        *pan += resp.drag_delta();
    }
    if resp.double_clicked() {
        *zoom = 1.0;
        *pan = egui::Vec2::ZERO;
    }

    // Keep the image from being dragged away; when it's smaller than the view it stays centered.
    let size = tex_size * fit * *zoom;
    let max_pan = ((size - rect.size()) * 0.5).max(egui::Vec2::ZERO);
    pan.x = pan.x.clamp(-max_pan.x, max_pan.x);
    pan.y = pan.y.clamp(-max_pan.y, max_pan.y);
    *prev_zoom = *zoom;

    if resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if resp.hovered() && (max_pan.x > 0.0 || max_pan.y > 0.0) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }

    let image_rect = egui::Rect::from_center_size(rect.center() + *pan, size);
    let full_uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    ui.painter_at(rect)
        .image(tex.id(), image_rect, full_uv, egui::Color32::WHITE);
}

/// Draws a heart (a rotated square plus two circles) centered on `center`. `d` is the square's half-diagonal.
fn draw_heart(painter: &egui::Painter, center: egui::Pos2, d: f32, color: egui::Color32) {
    let cx = center.x;
    let cy = center.y + 0.1 * d; // shift so the whole heart is vertically centered
    let diamond = vec![
        egui::pos2(cx, cy - d),
        egui::pos2(cx + d, cy),
        egui::pos2(cx, cy + d),
        egui::pos2(cx - d, cy),
    ];
    painter.add(egui::Shape::convex_polygon(
        diamond,
        color,
        egui::Stroke::NONE,
    ));
    let r = d * std::f32::consts::FRAC_1_SQRT_2;
    painter.circle_filled(egui::pos2(cx - d / 2.0, cy - d / 2.0), r, color);
    painter.circle_filled(egui::pos2(cx + d / 2.0, cy - d / 2.0), r, color);
}

/// A clickable heart: hollow outline when `filled` is false, solid light red when true.
pub fn heart_button(ui: &mut egui::Ui, filled: bool) -> egui::Response {
    let d = 7.0_f32;
    let size = egui::vec2(2.414 * d + 10.0, 2.207 * d + 8.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

    if ui.is_rect_visible(rect) {
        let scale = if response.hovered() { 1.1 } else { 1.0 };
        let painter = ui.painter();
        if filled {
            draw_heart(painter, rect.center(), d * scale, HEART_RED);
        } else {
            // Hollow look: outline-colored heart with a smaller background-colored heart on top.
            let outline = ui.visuals().widgets.inactive.fg_stroke.color;
            draw_heart(painter, rect.center(), d * scale, outline);
            draw_heart(
                painter,
                rect.center(),
                d * scale * 0.72,
                ui.visuals().panel_fill,
            );
        }
    }
    response
}

#[derive(Clone, Copy)]
pub enum Arrow {
    Left,
    Right,
}

/// A normal button with a solid triangle drawn on it. The triangle is painted with shapes instead of
/// a text character, so it never turns into a colored emoji and looks the same on every system.
pub fn arrow_button(ui: &mut egui::Ui, arrow: Arrow, tooltip: &str) -> egui::Response {
    let response = ui.add(egui::Button::new("").min_size(egui::vec2(28.0, 0.0)));

    let c = response.rect.center();
    let s = 5.0; // triangle size
    let points = match arrow {
        Arrow::Left => vec![
            egui::pos2(c.x + s * 0.6, c.y - s),
            egui::pos2(c.x + s * 0.6, c.y + s),
            egui::pos2(c.x - s, c.y),
        ],
        Arrow::Right => vec![
            egui::pos2(c.x - s * 0.6, c.y - s),
            egui::pos2(c.x - s * 0.6, c.y + s),
            egui::pos2(c.x + s, c.y),
        ],
    };
    let color = ui.style().interact(&response).fg_stroke.color;
    ui.painter().add(egui::Shape::convex_polygon(
        points,
        color,
        egui::Stroke::NONE,
    ));

    response.on_hover_text(tooltip)
}
