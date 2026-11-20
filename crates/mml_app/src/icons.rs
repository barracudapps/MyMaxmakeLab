//! Vector icons painted with egui, so they render identically on every
//! platform without depending on font glyph coverage.

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Select,
    Rectangle,
    Ellipse,
    Polygon,
    Text,
    Import,
    Fit,
    ZoomIn,
    ZoomOut,
    Grid,
    Pen,
}

fn paint(painter: &egui::Painter, icon: Icon, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.6, color);
    let c = rect.center();
    let s = rect.width().min(rect.height()) * 0.36;
    let p = |x: f32, y: f32| Pos2::new(c.x + x * s, c.y + y * s);
    match icon {
        Icon::Select => {
            painter.add(Shape::convex_polygon(
                vec![p(-0.6, -0.9), p(0.7, 0.2), p(0.05, 0.3), p(-0.25, 0.95)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Rectangle => {
            painter.rect_stroke(
                Rect::from_min_max(p(-0.9, -0.65), p(0.9, 0.65)),
                1.0,
                stroke,
                StrokeKind::Middle,
            );
        }
        Icon::Ellipse => {
            painter.circle_stroke(c, s * 0.85, stroke);
        }
        Icon::Polygon => {
            let points = (0..6)
                .map(|index| {
                    let angle = std::f32::consts::TAU * index as f32 / 6.0;
                    p(0.9 * angle.cos(), 0.9 * angle.sin())
                })
                .collect();
            painter.add(Shape::closed_line(points, stroke));
        }
        Icon::Text => {
            painter.line_segment([p(-0.8, -0.8), p(0.8, -0.8)], Stroke::new(2.0, color));
            painter.line_segment([p(0.0, -0.8), p(0.0, 0.9)], Stroke::new(2.0, color));
        }
        Icon::Import => {
            painter.line_segment([p(0.0, -0.9), p(0.0, 0.35)], stroke);
            painter.add(Shape::line(
                vec![p(-0.5, -0.1), p(0.0, 0.4), p(0.5, -0.1)],
                stroke,
            ));
            painter.add(Shape::line(
                vec![p(-0.9, 0.3), p(-0.9, 0.9), p(0.9, 0.9), p(0.9, 0.3)],
                stroke,
            ));
        }
        Icon::Fit => {
            for (x, y) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                painter.add(Shape::line(
                    vec![
                        p(0.9 * x, 0.35 * y),
                        p(0.9 * x, 0.9 * y),
                        p(0.35 * x, 0.9 * y),
                    ],
                    stroke,
                ));
            }
        }
        Icon::ZoomIn | Icon::ZoomOut => {
            painter.circle_stroke(p(-0.15, -0.15), s * 0.6, stroke);
            painter.line_segment([p(0.3, 0.3), p(0.9, 0.9)], Stroke::new(2.2, color));
            painter.line_segment([p(-0.45, -0.15), p(0.15, -0.15)], stroke);
            if icon == Icon::ZoomIn {
                painter.line_segment([p(-0.15, -0.45), p(-0.15, 0.15)], stroke);
            }
        }
        Icon::Pen => {
            painter.add(Shape::line(
                vec![
                    p(-0.9, 0.6),
                    p(-0.5, 0.1),
                    p(-0.1, 0.5),
                    p(0.3, -0.2),
                    p(0.9, -0.8),
                ],
                stroke,
            ));
        }
        Icon::Grid => {
            for t in [-0.45, 0.45] {
                painter.line_segment([p(t, -0.9), p(t, 0.9)], stroke);
                painter.line_segment([p(-0.9, t), p(0.9, t)], stroke);
            }
        }
    }
}

/// Square toggle-style button with an icon; returns the response.
pub fn icon_button(ui: &mut egui::Ui, icon: Icon, selected: bool, tooltip: &str) -> egui::Response {
    let size = Vec2::new(40.0, 36.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let visuals = if selected {
        ui.visuals().selection.bg_fill
    } else {
        Color32::TRANSPARENT
    };
    let widget = ui.style().interact_selectable(&response, selected);
    let fill = if selected {
        visuals
    } else {
        widget.weak_bg_fill
    };
    ui.painter().rect_filled(rect, 6.0, fill);
    let color = if selected {
        ui.visuals().selection.stroke.color
    } else {
        widget.fg_stroke.color
    };
    paint(ui.painter(), icon, rect.shrink(6.0), color);
    response.on_hover_text(tooltip)
}

/// Jog button with a painted arrow pointing at `angle` degrees
/// (0 = right, 90 = up); senses press and hold.
pub fn arrow_button(ui: &mut egui::Ui, angle: f32, size: Vec2) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let widget = ui.style().interact(&response);
    ui.painter().rect_filled(rect, 6.0, widget.weak_bg_fill);
    let color = widget.fg_stroke.color;
    let center = rect.center();
    let radius = rect.width().min(rect.height()) * 0.3;
    let (sin, cos) = angle.to_radians().sin_cos();
    let direction = Vec2::new(cos, -sin);
    let normal = Vec2::new(-direction.y, direction.x);
    let tip = center + direction * radius;
    let base = center - direction * radius * 0.2;
    ui.painter().add(Shape::convex_polygon(
        vec![
            tip,
            base + normal * radius * 0.7,
            base - normal * radius * 0.7,
        ],
        color,
        Stroke::NONE,
    ));
    ui.painter()
        .line_segment([center - direction * radius, base], Stroke::new(2.5, color));
    response
}
