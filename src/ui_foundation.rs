//! Reusable visual primitives for the Nodus desktop shell.
//!
//! Keeping these controls out of `app.rs` makes the interface read as one
//! system instead of a collection of locally-styled egui widgets.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2, pos2, vec2,
};

use crate::theme::{self, Palette};

const WINDOW_RESIZE_BORDER: f32 = 5.0;
const WINDOW_RESIZE_CORNER: f32 = 10.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Sidebar,
    PanelRight,
    Search,
    File,
    Folder,
    Plus,
    ChevronLeft,
    ChevronRight,
    Sync,
    Monitor,
    Sun,
    Moon,
    Copy,
    Device,
    Link,
    Write,
    Split,
    Read,
    Close,
    Minimize,
    Maximize,
    Trash,
    Grip,
}

pub fn icon_button(
    ui: &mut Ui,
    icon: Icon,
    selected: bool,
    tooltip: &str,
    palette: Palette,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(32.0, 30.0), Sense::click());
    let hovered = response.hovered();
    let fill = if selected {
        palette.surface_active
    } else if hovered {
        palette.surface_hover
    } else {
        Color32::TRANSPARENT
    };
    if fill != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, CornerRadius::same(7), fill);
    }
    let color = if selected || hovered {
        palette.ink
    } else {
        palette.muted
    };
    paint_icon(ui, icon, rect.center(), 16.0, color);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), selected, tooltip)
    });
    response.on_hover_text(tooltip)
}

pub fn compact_icon_button(ui: &mut Ui, icon: Icon, tooltip: &str, palette: Palette) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(6), palette.surface_hover);
    }
    paint_icon(
        ui,
        icon,
        rect.center(),
        14.0,
        if response.hovered() {
            palette.ink
        } else {
            palette.muted
        },
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), tooltip)
    });
    response.on_hover_text(tooltip)
}

pub fn segment_button(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    active: bool,
    tooltip: &str,
    palette: Palette,
) -> Response {
    let desired = vec2(34.0 + label.chars().count() as f32 * 6.5, 30.0);
    let (rect, response) = ui.allocate_exact_size(desired, Sense::click());
    let fill = if active {
        palette.surface_active
    } else if response.hovered() {
        palette.surface_hover
    } else {
        Color32::TRANSPARENT
    };
    if fill != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, CornerRadius::same(6), fill);
    }
    let color = if active || response.hovered() {
        palette.ink
    } else {
        palette.muted
    };
    paint_icon(
        ui,
        icon,
        pos2(rect.left() + 15.0, rect.center().y),
        14.0,
        color,
    );
    ui.painter().text(
        pos2(rect.left() + 27.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        theme::ui_medium(11.5),
        color,
    );
    let accessible_label = if label.is_empty() { tooltip } else { label };
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::Button,
            ui.is_enabled(),
            active,
            accessible_label,
        )
    });
    response.on_hover_text(tooltip)
}

pub fn section_label(ui: &mut Ui, label: &str, palette: Palette) {
    ui.label(
        egui::RichText::new(label)
            .font(theme::ui_semibold(11.5))
            .color(palette.muted),
    );
}

pub fn connection_dot(ui: &Ui, center: Pos2, color: Color32) {
    ui.painter().circle_filled(center, 4.0, color);
    ui.painter()
        .circle_stroke(center, 6.0, Stroke::new(1.0, color.gamma_multiply(0.28)));
}

pub fn separator(ui: &mut Ui, palette: Palette) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        Stroke::new(1.0, palette.border),
    );
}

pub fn paint_icon(ui: &Ui, icon: Icon, center: Pos2, size: f32, color: Color32) {
    let painter = ui.painter();
    let stroke = Stroke::new(1.55, color);
    let h = size * 0.5;
    let left = center.x - h;
    let right = center.x + h;
    let top = center.y - h;
    let bottom = center.y + h;
    match icon {
        Icon::Sidebar | Icon::PanelRight => {
            let rect =
                Rect::from_min_max(pos2(left + 1.0, top + 1.0), pos2(right - 1.0, bottom - 1.0));
            painter.rect_stroke(
                rect,
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Middle,
            );
            let x = if icon == Icon::Sidebar {
                left + size * 0.34
            } else {
                right - size * 0.34
            };
            painter.vline(x, top + 1.0..=bottom - 1.0, stroke);
        }
        Icon::Search => {
            painter.circle_stroke(pos2(center.x - 2.0, center.y - 2.0), size * 0.28, stroke);
            painter.line_segment(
                [
                    pos2(center.x + 2.0, center.y + 2.0),
                    pos2(right - 1.0, bottom - 1.0),
                ],
                stroke,
            );
        }
        Icon::File => {
            let rect =
                Rect::from_min_max(pos2(left + 3.0, top + 1.0), pos2(right - 2.0, bottom - 1.0));
            painter.rect_stroke(
                rect,
                CornerRadius::same(1),
                stroke,
                egui::StrokeKind::Middle,
            );
            painter.line_segment(
                [
                    pos2(left + 6.0, center.y - 1.0),
                    pos2(right - 5.0, center.y - 1.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    pos2(left + 6.0, center.y + 3.0),
                    pos2(right - 6.0, center.y + 3.0),
                ],
                stroke,
            );
        }
        Icon::Folder => {
            let points = [
                pos2(left + 1.0, top + 4.0),
                pos2(center.x - 1.0, top + 4.0),
                pos2(center.x + 1.0, top + 7.0),
                pos2(right - 1.0, top + 7.0),
                pos2(right - 1.0, bottom - 2.0),
                pos2(left + 1.0, bottom - 2.0),
            ];
            painter.add(egui::Shape::closed_line(points.to_vec(), stroke));
        }
        Icon::Plus => {
            painter.hline(center.x - 5.0..=center.x + 5.0, center.y, stroke);
            painter.vline(center.x, center.y - 5.0..=center.y + 5.0, stroke);
        }
        Icon::ChevronLeft => {
            painter.line_segment(
                [
                    pos2(center.x + 2.0, center.y - 5.0),
                    pos2(center.x - 2.0, center.y),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    pos2(center.x - 2.0, center.y),
                    pos2(center.x + 2.0, center.y + 5.0),
                ],
                stroke,
            );
        }
        Icon::ChevronRight => {
            painter.line_segment(
                [
                    pos2(center.x - 2.0, center.y - 5.0),
                    pos2(center.x + 2.0, center.y),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    pos2(center.x + 2.0, center.y),
                    pos2(center.x - 2.0, center.y + 5.0),
                ],
                stroke,
            );
        }
        Icon::Sync => {
            painter.arc(center, 5.0, 0.45, 2.45, stroke);
            painter.arc(center, 5.0, 3.6, 2.35, stroke);
            painter.line_segment(
                [pos2(right - 4.0, top + 3.0), pos2(right - 1.0, top + 5.0)],
                stroke,
            );
            painter.line_segment(
                [
                    pos2(left + 4.0, bottom - 3.0),
                    pos2(left + 1.0, bottom - 5.0),
                ],
                stroke,
            );
        }
        Icon::Monitor => {
            let rect =
                Rect::from_min_max(pos2(left + 1.0, top + 2.0), pos2(right - 1.0, bottom - 3.0));
            painter.rect_stroke(
                rect,
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Middle,
            );
            painter.vline(center.x, bottom - 3.0..=bottom, stroke);
            painter.hline(center.x - 4.0..=center.x + 4.0, bottom, stroke);
        }
        Icon::Sun => {
            painter.circle_stroke(center, 3.0, stroke);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let dir = vec2(a.cos(), a.sin());
                painter.line_segment([center + dir * 5.0, center + dir * 7.0], stroke);
            }
        }
        Icon::Moon => {
            painter.circle_stroke(center, 6.0, stroke);
            painter.circle_filled(
                pos2(center.x + 3.0, center.y - 2.0),
                5.0,
                ui.visuals().panel_fill,
            );
        }
        Icon::Copy => {
            let a =
                Rect::from_min_max(pos2(left + 4.0, top + 1.0), pos2(right - 1.0, bottom - 4.0));
            let b = a.translate(vec2(-3.0, 3.0));
            painter.rect_stroke(b, CornerRadius::same(1), stroke, egui::StrokeKind::Middle);
            painter.rect_stroke(a, CornerRadius::same(1), stroke, egui::StrokeKind::Middle);
        }
        Icon::Device => {
            let rect =
                Rect::from_min_max(pos2(left + 2.0, top + 1.0), pos2(right - 2.0, bottom - 1.0));
            painter.rect_stroke(
                rect,
                CornerRadius::same(3),
                stroke,
                egui::StrokeKind::Middle,
            );
            painter.circle_filled(pos2(center.x, bottom - 4.0), 1.0, color);
        }
        Icon::Link => {
            painter.circle_stroke(pos2(center.x - 3.0, center.y), 4.0, stroke);
            painter.circle_stroke(pos2(center.x + 3.0, center.y), 4.0, stroke);
            painter.hline(center.x - 2.0..=center.x + 2.0, center.y, stroke);
        }
        Icon::Write => {
            painter.line_segment(
                [pos2(left + 2.0, bottom - 2.0), pos2(right - 2.0, top + 2.0)],
                stroke,
            );
            painter.line_segment(
                [
                    pos2(left + 2.0, bottom - 2.0),
                    pos2(left + 6.0, bottom - 1.0),
                ],
                stroke,
            );
        }
        Icon::Split => {
            let rect =
                Rect::from_min_max(pos2(left + 1.0, top + 1.0), pos2(right - 1.0, bottom - 1.0));
            painter.rect_stroke(
                rect,
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Middle,
            );
            painter.vline(center.x, top + 1.0..=bottom - 1.0, stroke);
        }
        Icon::Read => {
            painter.line_segment(
                [pos2(center.x, top + 3.0), pos2(center.x, bottom - 1.0)],
                stroke,
            );
            painter.add(egui::Shape::line(
                vec![
                    pos2(center.x, top + 3.0),
                    pos2(left + 1.0, top + 1.0),
                    pos2(left + 1.0, bottom - 3.0),
                    pos2(center.x, bottom - 1.0),
                ],
                stroke,
            ));
            painter.add(egui::Shape::line(
                vec![
                    pos2(center.x, top + 3.0),
                    pos2(right - 1.0, top + 1.0),
                    pos2(right - 1.0, bottom - 3.0),
                    pos2(center.x, bottom - 1.0),
                ],
                stroke,
            ));
        }
        Icon::Close => {
            painter.line_segment(
                [pos2(left + 3.0, top + 3.0), pos2(right - 3.0, bottom - 3.0)],
                stroke,
            );
            painter.line_segment(
                [pos2(right - 3.0, top + 3.0), pos2(left + 3.0, bottom - 3.0)],
                stroke,
            );
        }
        Icon::Minimize => {
            painter.hline(left + 3.0..=right - 3.0, center.y + 3.0, stroke);
        }
        Icon::Maximize => {
            painter.rect_stroke(
                Rect::from_min_max(pos2(left + 3.0, top + 3.0), pos2(right - 3.0, bottom - 3.0)),
                CornerRadius::ZERO,
                stroke,
                egui::StrokeKind::Middle,
            );
        }
        Icon::Trash => {
            painter.rect_stroke(
                Rect::from_min_max(pos2(left + 4.0, top + 5.0), pos2(right - 4.0, bottom - 2.0)),
                CornerRadius::same(1),
                stroke,
                egui::StrokeKind::Middle,
            );
            painter.hline(left + 3.0..=right - 3.0, top + 3.0, stroke);
            painter.hline(center.x - 2.0..=center.x + 2.0, top + 1.0, stroke);
        }
        Icon::Grip => {
            for x in [-2.0, 2.0] {
                for y in [-4.0, 0.0, 4.0] {
                    painter.circle_filled(pos2(center.x + x, center.y + y), 1.0, color);
                }
            }
        }
    }
}

/// Restores native-feeling resize edges for the borderless Windows shell.
pub fn window_resize(ui: &mut Ui) {
    let (fullscreen, maximized) = ui.ctx().input(|input| {
        (
            input.viewport().fullscreen.unwrap_or(false),
            input.viewport().maximized.unwrap_or(false),
        )
    });
    if !cfg!(windows) || fullscreen || maximized {
        return;
    }
    let Some(position) = ui.input(|input| input.pointer.hover_pos()) else {
        return;
    };
    let Some((direction, cursor)) = resize_direction(ui.ctx().content_rect(), position) else {
        return;
    };
    ui.ctx().set_cursor_icon(cursor);
    if ui.input(|input| input.pointer.primary_pressed()) {
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
    }
}

fn resize_direction(
    window: Rect,
    position: Pos2,
) -> Option<(egui::ResizeDirection, egui::CursorIcon)> {
    if !window.contains(position) {
        return None;
    }
    let [left, right, top, bottom] = [
        position.x - window.left(),
        window.right() - position.x,
        position.y - window.top(),
        window.bottom() - position.y,
    ];
    let mut x = i8::from(right <= WINDOW_RESIZE_BORDER) - i8::from(left <= WINDOW_RESIZE_BORDER);
    let mut y = i8::from(bottom <= WINDOW_RESIZE_BORDER) - i8::from(top <= WINDOW_RESIZE_BORDER);
    if y != 0 {
        x = i8::from(right <= WINDOW_RESIZE_CORNER) - i8::from(left <= WINDOW_RESIZE_CORNER);
    }
    if x != 0 {
        y = i8::from(bottom <= WINDOW_RESIZE_CORNER) - i8::from(top <= WINDOW_RESIZE_CORNER);
    }
    use egui::{CursorIcon as C, ResizeDirection as D};
    match (x, y) {
        (-1, -1) => Some((D::NorthWest, C::ResizeNwSe)),
        (1, -1) => Some((D::NorthEast, C::ResizeNeSw)),
        (-1, 1) => Some((D::SouthWest, C::ResizeNeSw)),
        (1, 1) => Some((D::SouthEast, C::ResizeNwSe)),
        (-1, 0) => Some((D::West, C::ResizeHorizontal)),
        (1, 0) => Some((D::East, C::ResizeHorizontal)),
        (0, -1) => Some((D::North, C::ResizeVertical)),
        (0, 1) => Some((D::South, C::ResizeVertical)),
        _ => None,
    }
}

trait PainterArc {
    fn arc(&self, center: Pos2, radius: f32, start: f32, sweep: f32, stroke: Stroke);
}

impl PainterArc for egui::Painter {
    fn arc(&self, center: Pos2, radius: f32, start: f32, sweep: f32, stroke: Stroke) {
        let points = 12;
        let path = (0..=points)
            .map(|i| {
                let angle = start + sweep * i as f32 / points as f32;
                center + Vec2::angled(angle) * radius
            })
            .collect();
        self.add(egui::Shape::line(path, stroke));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_hit_test_covers_edges_and_corners() {
        use egui::ResizeDirection as D;

        let window = Rect::from_min_max(pos2(20.0, 30.0), pos2(120.0, 110.0));
        for (position, expected) in [
            (pos2(21.0, 31.0), Some(D::NorthWest)),
            (pos2(70.0, 31.0), Some(D::North)),
            (pos2(119.0, 31.0), Some(D::NorthEast)),
            (pos2(21.0, 70.0), Some(D::West)),
            (pos2(70.0, 70.0), None),
            (pos2(119.0, 70.0), Some(D::East)),
            (pos2(21.0, 109.0), Some(D::SouthWest)),
            (pos2(70.0, 109.0), Some(D::South)),
            (pos2(119.0, 109.0), Some(D::SouthEast)),
        ] {
            assert_eq!(
                resize_direction(window, position).map(|hit| hit.0),
                expected
            );
        }
    }
}
