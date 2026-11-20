//! Interactive design canvas: bed, shapes, selection handles, creation
//! tools, pan and zoom.

use crate::i18n::tr;
use eframe::egui::{
    self, Align2, Color32, CursorIcon, FontId, Mesh, PointerButton, Pos2, Rect as ScreenRect,
    Sense, Shape as EShape, Stroke, StrokeKind, TextureHandle, TextureOptions, Vec2,
    epaint::Vertex,
};
use mml_core::document::{Document, Shape, ShapeId, ShapeKind};
use mml_core::geometry::{Point, Rect, Transform};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Select,
    Rectangle,
    Ellipse,
    Polygon,
    Text,
    /// Freehand drawing with the mouse, a stylus or a graphics tablet.
    Pen,
}

#[derive(Debug, Clone, Copy)]
enum Drag {
    None,
    Move { start: Point },
    Scale { pivot: Point, start: Point },
    Rotate { center: Point, start_angle: f64 },
    Marquee { start: Point },
    Create { start: Point },
    Freehand,
}

/// What the canvas asks the application to do this frame.
pub enum CanvasAction {
    None,
    Select(Vec<ShapeId>),
    /// Commit `transform` to the selected shapes (one undo step).
    Transform(Transform),
    Create(Shape),
    TextAt(Point),
    /// A point clicked while picking the job anchor.
    Pick(Point),
}

pub struct Canvas {
    pub zoom: f32,
    pub pan: Vec2,
    pub tool: Tool,
    pub polygon_sides: u32,
    drag: Drag,
    pending: Transform,
    create_preview: Option<Rect>,
    marquee: Option<Rect>,
    textures: HashMap<usize, TextureHandle>,
    preview_texture: Option<(TextureHandle, Rect)>,
    pub cursor_world: Option<Point>,
    fit_requested: bool,
    pub machine_position: Option<Point>,
    pub anchor_marker: Option<Point>,
    pub picking: bool,
    stroke: Vec<Point>,
    /// Rectified camera picture of the bed and its opacity.
    pub camera: Option<(TextureHandle, f32)>,
    /// Head seen by the camera check, in bed millimetres, and whether
    /// it matches the reported position.
    pub vision_marker: Option<(Point, bool)>,
    /// Stock seen by the camera, drawn as a dashed outline.
    pub stock_outline: Option<mml_core::geometry::Rect>,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            zoom: 2.0,
            pan: Vec2::new(40.0, -40.0),
            tool: Tool::Select,
            polygon_sides: 6,
            drag: Drag::None,
            pending: Transform::IDENTITY,
            create_preview: None,
            marquee: None,
            textures: HashMap::new(),
            preview_texture: None,
            cursor_world: None,
            fit_requested: true,
            machine_position: None,
            anchor_marker: None,
            picking: false,
            stroke: Vec::new(),
            camera: None,
            vision_marker: None,
            stock_outline: None,
        }
    }
}

pub struct View {
    rect: ScreenRect,
    zoom: f32,
    pan: Vec2,
}

impl View {
    pub fn to_screen(&self, point: Point) -> Pos2 {
        Pos2::new(
            self.rect.left() + self.pan.x + point.x as f32 * self.zoom,
            self.rect.bottom() + self.pan.y - point.y as f32 * self.zoom,
        )
    }

    pub fn to_world(&self, pos: Pos2) -> Point {
        Point::new(
            ((pos.x - self.rect.left() - self.pan.x) / self.zoom) as f64,
            ((self.rect.bottom() + self.pan.y - pos.y) / self.zoom) as f64,
        )
    }
}

pub struct CanvasInput<'a> {
    pub document: &'a Document,
    pub selection: &'a [ShapeId],
    pub bed: (f64, f64),
    pub grid: f64,
    pub snap: bool,
    pub current_layer: mml_core::LayerId,
    /// Prepare / Run modes show the toolpath preview, not handles.
    pub show_preview: bool,
    pub dim_design: bool,
    pub travel: &'a [(Point, Point)],
    /// Corner where the machine homes, marked on the bed.
    pub home: mml_core::machine::Origin,
    /// Translated "front", "back" and "home" labels.
    pub labels: [&'a str; 3],
}

const HANDLE: f32 = 7.0;

impl Canvas {
    pub fn request_fit(&mut self) {
        self.fit_requested = true;
    }

    pub fn cancel_fit(&mut self) {
        self.fit_requested = false;
    }

    pub fn zoom_by(&mut self, factor: f32) {
        self.zoom = (self.zoom * factor).clamp(0.05, 200.0);
    }

    pub fn set_preview(
        &mut self,
        ctx: &egui::Context,
        preview: Option<crate::preview::PreviewImage>,
    ) {
        self.preview_texture = preview.map(|preview| {
            (
                ctx.load_texture("toolpath-preview", preview.image, TextureOptions::LINEAR),
                preview.bounds,
            )
        });
    }

    pub fn has_preview(&self) -> bool {
        self.preview_texture.is_some()
    }

    fn fit(&mut self, rect: ScreenRect, area: Rect) {
        let margin = 30.0;
        let zoom_x = (rect.width() - 2.0 * margin) / area.width().max(1.0) as f32;
        let zoom_y = (rect.height() - 2.0 * margin) / area.height().max(1.0) as f32;
        self.zoom = zoom_x.min(zoom_y).clamp(0.05, 200.0);
        let center = area.center();
        self.pan = Vec2::new(
            rect.width() / 2.0 - center.x as f32 * self.zoom,
            -(rect.height() / 2.0) + center.y as f32 * self.zoom,
        );
    }

    pub fn show(&mut self, ui: &mut egui::Ui, input: CanvasInput<'_>) -> CanvasAction {
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let rect = response.rect;
        if self.fit_requested {
            let bed = Rect::from_points(Point::default(), Point::new(input.bed.0, input.bed.1));
            let mut area = if input.document.shapes.is_empty() {
                bed
            } else {
                bed.union(input.document.bounds())
            };
            // Prepare and Run focus on the job itself, not the whole bed.
            if input.show_preview
                && let Some((_, bounds)) = &self.preview_texture
            {
                let margin = bounds.width().max(bounds.height()) * 0.15;
                area = Rect::from_points(
                    Point::new(bounds.min.x - margin, bounds.min.y - margin),
                    Point::new(bounds.max.x + margin, bounds.max.y + margin),
                );
            }
            self.fit(rect, area);
            self.fit_requested = false;
        }
        let dark = ui.visuals().dark_mode;
        let palette = Palette::new(dark);
        painter.rect_filled(rect, 0.0, palette.background);

        // Zoom around the cursor; pinch works on touchpads.
        if let Some(hover) = response.hover_pos() {
            let (scroll, pinch, command) = ui.input(|input| {
                (
                    input.smooth_scroll_delta,
                    input.zoom_delta(),
                    input.modifiers.command,
                )
            });
            let mut factor = pinch;
            if command || scroll.x == 0.0 {
                factor *= (scroll.y / 300.0).exp();
            } else {
                self.pan += scroll;
            }
            if (factor - 1.0).abs() > 1e-4 {
                let before = View {
                    rect,
                    zoom: self.zoom,
                    pan: self.pan,
                }
                .to_world(hover);
                self.zoom_by(factor);
                let after = View {
                    rect,
                    zoom: self.zoom,
                    pan: self.pan,
                }
                .to_screen(before);
                self.pan += hover - after;
            }
        }
        let view = View {
            rect,
            zoom: self.zoom,
            pan: self.pan,
        };
        self.cursor_world = response.hover_pos().map(|pos| view.to_world(pos));

        let painter = painter.with_clip_rect(rect);
        self.paint_bed(&painter, &view, input.bed, input.grid, &palette);
        self.paint_orientation(&painter, &view, &input, &palette);
        if let Some((texture, opacity)) = &self.camera {
            let screen = ScreenRect::from_two_pos(
                view.to_screen(Point::new(0.0, input.bed.1)),
                view.to_screen(Point::new(input.bed.0, 0.0)),
            );
            let uv = ScreenRect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
            painter.image(
                texture.id(),
                screen,
                uv,
                Color32::from_white_alpha((opacity * 255.0) as u8),
            );
        }
        if let Some(stock) = self.stock_outline {
            let corners = [
                Point::new(stock.min.x, stock.min.y),
                Point::new(stock.max.x, stock.min.y),
                Point::new(stock.max.x, stock.max.y),
                Point::new(stock.min.x, stock.max.y),
            ];
            for index in 0..4 {
                let (a, b) = (
                    view.to_screen(corners[index]),
                    view.to_screen(corners[(index + 1) % 4]),
                );
                painter.extend(egui::Shape::dashed_line(
                    &[a, b],
                    egui::Stroke::new(1.5, crate::theme::WARNING),
                    6.0,
                    4.0,
                ));
            }
        }
        if let Some((point, consistent)) = self.vision_marker {
            let color = if consistent {
                crate::theme::SUCCESS
            } else {
                crate::theme::DANGER
            };
            let center = view.to_screen(point);
            painter.circle_stroke(center, 9.0, egui::Stroke::new(2.5, color));
            painter.circle_filled(center, 2.5, color);
        }

        if input.show_preview
            && let Some((texture, bounds)) = &self.preview_texture
        {
            let screen = ScreenRect::from_two_pos(
                view.to_screen(Point::new(bounds.min.x, bounds.max.y)),
                view.to_screen(Point::new(bounds.max.x, bounds.min.y)),
            );
            painter.image(
                texture.id(),
                screen,
                ScreenRect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        for (from, to) in input.travel {
            painter.line_segment(
                [view.to_screen(*from), view.to_screen(*to)],
                Stroke::new(0.6, palette.travel),
            );
        }

        self.paint_shapes(ui.ctx(), &painter, &view, &input, &palette);
        if let Some(anchor) = self.anchor_marker {
            let center = view.to_screen(anchor);
            let stroke = Stroke::new(2.0, palette.selection);
            painter.circle_stroke(center, 9.0, stroke);
            painter.circle_filled(center, 2.5, palette.selection);
            painter.text(
                center + Vec2::new(12.0, -12.0),
                Align2::LEFT_BOTTOM,
                "◎",
                FontId::proportional(13.0),
                palette.selection,
            );
        }
        if let Some(position) = self.machine_position {
            let center = view.to_screen(position);
            let stroke = Stroke::new(1.5, palette.head);
            painter.circle_stroke(center, 7.0, stroke);
            painter.line_segment([center - Vec2::X * 12.0, center + Vec2::X * 12.0], stroke);
            painter.line_segment([center - Vec2::Y * 12.0, center + Vec2::Y * 12.0], stroke);
        }
        if input.show_preview {
            if self.picking {
                ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
                if response.clicked()
                    && let Some(pos) = response.interact_pointer_pos()
                {
                    return CanvasAction::Pick(view.to_world(pos));
                }
            }
            return self.handle_navigation_only(&response, &view);
        }
        self.interact(ui, &response, &painter, &view, &input, &palette)
    }

    fn handle_navigation_only(&mut self, response: &egui::Response, _view: &View) -> CanvasAction {
        if response.dragged_by(PointerButton::Primary) || response.dragged_by(PointerButton::Middle)
        {
            self.pan += response.drag_delta();
        }
        CanvasAction::None
    }

    fn paint_bed(
        &self,
        painter: &egui::Painter,
        view: &View,
        bed: (f64, f64),
        grid: f64,
        palette: &Palette,
    ) {
        let bed_rect = ScreenRect::from_two_pos(
            view.to_screen(Point::new(0.0, bed.1)),
            view.to_screen(Point::new(bed.0, 0.0)),
        );
        painter.rect_filled(bed_rect, 2.0, palette.bed);
        // Keep at least 8 px between grid lines whatever the zoom.
        let mut step = grid.max(0.1);
        while step * (self.zoom as f64) < 8.0 {
            step *= if (step / grid).round() as i64 % 5 == 0 {
                2.0
            } else {
                5.0
            };
        }
        let major = step * 10.0;
        let draw_lines = |spacing: f64, color: Color32| {
            let mut x = 0.0;
            while x <= bed.0 + 1e-9 {
                painter.line_segment(
                    [
                        view.to_screen(Point::new(x, 0.0)),
                        view.to_screen(Point::new(x, bed.1)),
                    ],
                    Stroke::new(1.0, color),
                );
                x += spacing;
            }
            let mut y = 0.0;
            while y <= bed.1 + 1e-9 {
                painter.line_segment(
                    [
                        view.to_screen(Point::new(0.0, y)),
                        view.to_screen(Point::new(bed.0, y)),
                    ],
                    Stroke::new(1.0, color),
                );
                y += spacing;
            }
        };
        draw_lines(step, palette.grid_minor);
        draw_lines(major, palette.grid_major);
        painter.rect_stroke(
            bed_rect,
            2.0,
            Stroke::new(1.0, palette.bed_border),
            StrokeKind::Outside,
        );
        let font = FontId::proportional(11.0);
        let mut x = 0.0;
        while x <= bed.0 + 1e-9 {
            painter.text(
                view.to_screen(Point::new(x, 0.0)) + Vec2::new(0.0, 4.0),
                Align2::CENTER_TOP,
                format!("{x:.0}"),
                font.clone(),
                palette.ruler,
            );
            x += major;
        }
        let mut y = major;
        while y <= bed.1 + 1e-9 {
            painter.text(
                view.to_screen(Point::new(0.0, y)) - Vec2::new(4.0, 0.0),
                Align2::RIGHT_CENTER,
                format!("{y:.0}"),
                font.clone(),
                palette.ruler,
            );
            y += major;
        }
        painter.circle_filled(view.to_screen(Point::default()), 3.5, palette.origin);
    }

    /// Front and back of the machine, its home corner and the axes, so
    /// the bed on screen is easy to match with the real one.
    fn paint_orientation(
        &self,
        painter: &egui::Painter,
        view: &View,
        input: &CanvasInput,
        palette: &Palette,
    ) {
        let (width, height) = input.bed;
        let [front, back, home] = input.labels;
        let font = FontId::proportional(13.0);
        let strong = palette.ruler.gamma_multiply(1.4);
        painter.text(
            view.to_screen(Point::new(width / 2.0, 0.0)) + Vec2::new(0.0, 20.0),
            Align2::CENTER_TOP,
            front,
            font.clone(),
            strong,
        );
        painter.text(
            view.to_screen(Point::new(width / 2.0, height)) - Vec2::new(0.0, 8.0),
            Align2::CENTER_BOTTOM,
            back,
            font,
            strong,
        );
        // Axes from the design origin.
        let origin = view.to_screen(Point::default());
        let arrow = |direction: Vec2, color: Color32, name: &str| {
            let tip = origin + direction * 34.0;
            painter.line_segment([origin, tip], Stroke::new(2.0, color));
            let side = Vec2::new(-direction.y, direction.x) * 4.0;
            painter.add(egui::Shape::convex_polygon(
                vec![tip + direction * 6.0, tip + side, tip - side],
                color,
                Stroke::NONE,
            ));
            painter.text(
                tip + direction * 14.0,
                Align2::CENTER_CENTER,
                name,
                FontId::proportional(12.0),
                color,
            );
        };
        arrow(Vec2::new(1.0, 0.0), Color32::from_rgb(220, 70, 70), "X");
        arrow(Vec2::new(0.0, -1.0), Color32::from_rgb(60, 170, 90), "Y");
        // Home corner: a ring and its label outside the bed.
        use mml_core::machine::Origin;
        let (corner, outward) = match input.home {
            Origin::FrontLeft => (Point::new(0.0, 0.0), Vec2::new(-1.0, 1.0)),
            Origin::FrontRight => (Point::new(width, 0.0), Vec2::new(1.0, 1.0)),
            Origin::RearLeft => (Point::new(0.0, height), Vec2::new(-1.0, -1.0)),
            Origin::RearRight => (Point::new(width, height), Vec2::new(1.0, -1.0)),
        };
        let at = view.to_screen(corner);
        painter.circle_stroke(at, 9.0, Stroke::new(2.0, crate::theme::ACCENT));
        painter.circle_filled(at, 3.0, crate::theme::ACCENT);
        // Label inside the bed, on the diagonal between the axes, so it
        // is never clipped by the canvas edge.
        let inward = -outward;
        let align = match (inward.x > 0.0, inward.y > 0.0) {
            (true, true) => Align2::LEFT_TOP,
            (true, false) => Align2::LEFT_BOTTOM,
            (false, true) => Align2::RIGHT_TOP,
            (false, false) => Align2::RIGHT_BOTTOM,
        };
        painter.text(
            at + inward * 14.0,
            align,
            home,
            FontId::proportional(11.0),
            crate::theme::ACCENT,
        );
    }

    fn texture_for(&mut self, ctx: &egui::Context, shape: &Shape) -> Option<TextureHandle> {
        let ShapeKind::Image { image, .. } = &shape.kind else {
            return None;
        };
        let key = std::sync::Arc::as_ptr(image) as usize;
        Some(
            self.textures
                .entry(key)
                .or_insert_with(|| {
                    let picture = egui::ColorImage::from_gray(
                        [image.width as usize, image.height as usize],
                        &image.pixels,
                    );
                    ctx.load_texture(format!("image-{key}"), picture, TextureOptions::LINEAR)
                })
                .clone(),
        )
    }

    fn paint_shapes(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        view: &View,
        input: &CanvasInput<'_>,
        palette: &Palette,
    ) {
        for shape in &input.document.shapes {
            let Some(layer) = input.document.layer(shape.layer) else {
                continue;
            };
            if !layer.visible {
                continue;
            }
            let selected = input.selection.contains(&shape.id);
            let transform = if selected {
                shape.transform.then(self.pending)
            } else {
                shape.transform
            };
            let [r, g, b] = layer.color;
            let alpha = if input.dim_design { 90 } else { 255 };
            let color = Color32::from_rgba_unmultiplied(r, g, b, alpha);
            if let ShapeKind::Image { width, height, .. } = &shape.kind
                && let Some(texture) = self.texture_for(ctx, shape)
            {
                let corners = [
                    (-0.5, 0.5, 0.0, 0.0),
                    (0.5, 0.5, 1.0, 0.0),
                    (0.5, -0.5, 1.0, 1.0),
                    (-0.5, -0.5, 0.0, 1.0),
                ];
                let mut mesh = Mesh::with_texture(texture.id());
                let tint = Color32::from_white_alpha(if input.dim_design { 70 } else { 255 });
                for (x, y, u, v) in corners {
                    let point = transform.apply(Point::new(x * width, y * height));
                    mesh.vertices.push(Vertex {
                        pos: view.to_screen(point),
                        uv: Pos2::new(u, v),
                        color: tint,
                    });
                }
                mesh.indices.extend([0, 1, 2, 0, 2, 3]);
                painter.add(EShape::mesh(mesh));
            }
            let width = if selected { 2.0 } else { 1.2 };
            for path in shape.local_paths() {
                let points: Vec<Pos2> = path
                    .points
                    .iter()
                    .map(|point| view.to_screen(transform.apply(*point)))
                    .collect();
                if points.len() < 2 {
                    continue;
                }
                let stroke = Stroke::new(width, color);
                painter.add(if path.closed {
                    EShape::closed_line(points, stroke)
                } else {
                    EShape::line(points, stroke)
                });
            }
        }
        if !input.selection.is_empty() && !input.show_preview {
            let bounds = self.selection_bounds(input);
            if !bounds.is_empty() {
                let screen = self.screen_rect_of(view, bounds);
                painter.rect_stroke(
                    screen,
                    0.0,
                    Stroke::new(1.0, palette.selection),
                    StrokeKind::Outside,
                );
                for corner in screen_corners(screen) {
                    painter.rect_filled(
                        ScreenRect::from_center_size(corner, Vec2::splat(HANDLE * 2.0)),
                        2.0,
                        palette.selection,
                    );
                    painter.rect_filled(
                        ScreenRect::from_center_size(corner, Vec2::splat(HANDLE * 2.0 - 3.0)),
                        1.5,
                        palette.background,
                    );
                }
                let rotate = rotate_handle(screen);
                painter.line_segment(
                    [Pos2::new(screen.center().x, screen.top()), rotate],
                    Stroke::new(1.0, palette.selection),
                );
                painter.circle_filled(rotate, HANDLE, palette.selection);
                let size = format!("{:.1} × {:.1} mm", bounds.width(), bounds.height());
                painter.text(
                    Pos2::new(screen.center().x, screen.bottom() + 6.0),
                    Align2::CENTER_TOP,
                    size,
                    FontId::proportional(12.0),
                    palette.selection,
                );
            }
        }
        if self.stroke.len() > 1 {
            let points: Vec<Pos2> = self
                .stroke
                .iter()
                .map(|point| view.to_screen(*point))
                .collect();
            painter.add(EShape::line(points, Stroke::new(1.5, palette.selection)));
        }
        if let Some(area) = self.create_preview.or(self.marquee) {
            let screen = self.screen_rect_of(view, area);
            painter.rect_stroke(
                screen,
                0.0,
                Stroke::new(1.0, palette.selection),
                StrokeKind::Inside,
            );
            painter.rect_filled(screen, 0.0, palette.selection.gamma_multiply(0.08));
        }
    }

    fn screen_rect_of(&self, view: &View, area: Rect) -> ScreenRect {
        ScreenRect::from_two_pos(
            view.to_screen(Point::new(area.min.x, area.max.y)),
            view.to_screen(Point::new(area.max.x, area.min.y)),
        )
    }

    fn selection_bounds(&self, input: &CanvasInput<'_>) -> Rect {
        input
            .document
            .shapes
            .iter()
            .filter(|shape| input.selection.contains(&shape.id))
            .fold(Rect::EMPTY, |rect, shape| {
                let transform = shape.transform.then(self.pending);
                shape.local_paths().iter().fold(rect, |rect, path| {
                    rect.union(path.transformed(&transform).bounds())
                })
            })
    }

    fn snap(&self, value: Point, input: &CanvasInput<'_>) -> Point {
        if input.snap && input.grid > 0.0 {
            Point::new(
                (value.x / input.grid).round() * input.grid,
                (value.y / input.grid).round() * input.grid,
            )
        } else {
            value
        }
    }

    fn interact(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        _painter: &egui::Painter,
        view: &View,
        input: &CanvasInput<'_>,
        _palette: &Palette,
    ) -> CanvasAction {
        let shift = ui.input(|state| state.modifiers.shift);
        let space = ui.input(|state| state.key_down(egui::Key::Space));
        if response.dragged_by(PointerButton::Middle)
            || (space && response.dragged_by(PointerButton::Primary))
        {
            self.pan += response.drag_delta();
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
            return CanvasAction::None;
        }
        let bounds = self.selection_bounds(input);
        let screen_bounds = (!bounds.is_empty()).then(|| self.screen_rect_of(view, bounds));
        if let (Some(hover), Some(screen)) = (response.hover_pos(), screen_bounds)
            && self.tool == Tool::Select
        {
            if hover.distance(rotate_handle(screen)) <= HANDLE + 2.0 {
                ui.ctx().set_cursor_icon(CursorIcon::Alias);
            } else if screen_corners(screen)
                .iter()
                .any(|corner| hover.distance(*corner) <= HANDLE + 2.0)
            {
                ui.ctx().set_cursor_icon(CursorIcon::ResizeNwSe);
            } else if screen.contains(hover) {
                ui.ctx().set_cursor_icon(CursorIcon::Move);
            }
        } else if self.tool != Tool::Select && response.hovered() {
            // Pen and creation tools use a crosshair.
            ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
        }

        let tolerance = 4.0 / self.zoom as f64;
        if response.drag_started_by(PointerButton::Primary)
            && let Some(press) = response.interact_pointer_pos()
        {
            let world = view.to_world(press);
            self.drag = match self.tool {
                Tool::Select => {
                    if let Some(screen) = screen_bounds {
                        if press.distance(rotate_handle(screen)) <= HANDLE + 2.0 {
                            let center = bounds.center();
                            Drag::Rotate {
                                center,
                                start_angle: (world.y - center.y).atan2(world.x - center.x),
                            }
                        } else if let Some(index) = screen_corners(screen)
                            .iter()
                            .position(|corner| press.distance(*corner) <= HANDLE + 2.0)
                        {
                            let opposite = screen_corners(screen)[(index + 2) % 4];
                            Drag::Scale {
                                pivot: view.to_world(opposite),
                                start: world,
                            }
                        } else if screen.contains(press) {
                            Drag::Move { start: world }
                        } else {
                            self.start_select_drag(world, tolerance, input, shift)
                        }
                    } else {
                        self.start_select_drag(world, tolerance, input, shift)
                    }
                }
                Tool::Text => Drag::None,
                Tool::Pen => {
                    self.stroke = vec![world];
                    Drag::Freehand
                }
                _ => Drag::Create {
                    start: self.snap(world, input),
                },
            };
            if let Drag::Move { .. } = self.drag
                && let Some(hit) = input.document.hit_test(world, tolerance)
                && !input.selection.contains(&hit)
            {
                self.drag = Drag::Move { start: world };
                return CanvasAction::Select(vec![hit]);
            }
        }

        if response.dragged_by(PointerButton::Primary)
            && let Some(pos) = response.interact_pointer_pos()
        {
            let world = view.to_world(pos);
            match self.drag {
                Drag::Move { start } => {
                    let delta = world - start;
                    let delta = if input.snap {
                        self.snap(delta, input)
                    } else {
                        delta
                    };
                    self.pending = Transform::translate(delta.x, delta.y);
                }
                Drag::Scale { pivot, start } => {
                    let (dx0, dy0) = (start.x - pivot.x, start.y - pivot.y);
                    let (dx1, dy1) = (world.x - pivot.x, world.y - pivot.y);
                    let (sx, sy) = if shift {
                        (safe_ratio(dx1, dx0), safe_ratio(dy1, dy0))
                    } else {
                        let ratio = safe_ratio(
                            (dx1 * dx1 + dy1 * dy1).sqrt(),
                            (dx0 * dx0 + dy0 * dy0).sqrt(),
                        );
                        (ratio, ratio)
                    };
                    self.pending = Transform::scale(sx, sy).around(pivot);
                }
                Drag::Rotate {
                    center,
                    start_angle,
                } => {
                    let mut angle =
                        ((world.y - center.y).atan2(world.x - center.x) - start_angle).to_degrees();
                    if shift {
                        angle = (angle / 15.0).round() * 15.0;
                    }
                    self.pending = Transform::rotate_degrees(angle).around(center);
                }
                Drag::Marquee { start } => self.marquee = Some(Rect::from_points(start, world)),
                Drag::Freehand => {
                    let spacing = 1.5 / self.zoom as f64;
                    if self
                        .stroke
                        .last()
                        .is_none_or(|last| last.distance(world) >= spacing)
                    {
                        self.stroke.push(world);
                    }
                }
                Drag::Create { start } => {
                    let end = self.snap(world, input);
                    let end = if shift {
                        let side = (end.x - start.x).abs().max((end.y - start.y).abs());
                        Point::new(
                            start.x + side * (end.x - start.x).signum(),
                            start.y + side * (end.y - start.y).signum(),
                        )
                    } else {
                        end
                    };
                    self.create_preview = Some(Rect::from_points(start, end));
                }
                Drag::None => {}
            }
        }

        if response.drag_stopped() {
            let drag = std::mem::replace(&mut self.drag, Drag::None);
            let pending = std::mem::replace(&mut self.pending, Transform::IDENTITY);
            match drag {
                Drag::Move { .. } | Drag::Scale { .. } | Drag::Rotate { .. } => {
                    if pending != Transform::IDENTITY {
                        return CanvasAction::Transform(pending);
                    }
                }
                Drag::Marquee { .. } => {
                    if let Some(area) = self.marquee.take() {
                        let mut ids: Vec<ShapeId> = if shift {
                            input.selection.to_vec()
                        } else {
                            Vec::new()
                        };
                        ids.extend(
                            input
                                .document
                                .shapes
                                .iter()
                                .filter(|shape| !shape.locked)
                                .filter(|shape| {
                                    input
                                        .document
                                        .layer(shape.layer)
                                        .is_some_and(|layer| layer.visible)
                                })
                                .filter(|shape| {
                                    let shape_bounds = shape.bounds();
                                    area.contains(shape_bounds.min)
                                        && area.contains(shape_bounds.max)
                                })
                                .map(|shape| shape.id),
                        );
                        ids.dedup();
                        return CanvasAction::Select(ids);
                    }
                }
                Drag::Freehand => {
                    let stroke = std::mem::take(&mut self.stroke);
                    if let Some(shape) = freehand_shape(&stroke, self.zoom, input.current_layer) {
                        return CanvasAction::Create(shape);
                    }
                }
                Drag::Create { .. } => {
                    if let Some(area) = self.create_preview.take()
                        && area.width() > 0.2
                        && area.height() > 0.2
                    {
                        let kind = match self.tool {
                            Tool::Rectangle => ShapeKind::Rectangle {
                                width: area.width(),
                                height: area.height(),
                                corner_radius: 0.0,
                            },
                            Tool::Ellipse => ShapeKind::Ellipse {
                                rx: area.width() / 2.0,
                                ry: area.height() / 2.0,
                            },
                            _ => ShapeKind::Polygon {
                                sides: self.polygon_sides,
                                radius: area.width().max(area.height()) / 2.0,
                            },
                        };
                        let name = match self.tool {
                            Tool::Rectangle => tr("shape.rectangle"),
                            Tool::Ellipse => tr("shape.ellipse"),
                            _ => tr("shape.polygon"),
                        };
                        let mut shape = Shape::new(name, input.current_layer, kind);
                        let center = area.center();
                        shape.transform = Transform::translate(center.x, center.y);
                        self.tool = Tool::Select;
                        return CanvasAction::Create(shape);
                    }
                }
                Drag::None => {}
            }
        }

        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            let world = view.to_world(pos);
            return match self.tool {
                Tool::Text => {
                    self.tool = Tool::Select;
                    CanvasAction::TextAt(self.snap(world, input))
                }
                Tool::Select => match input.document.hit_test(world, tolerance) {
                    Some(hit) if shift => {
                        let mut ids = input.selection.to_vec();
                        if let Some(index) = ids.iter().position(|id| *id == hit) {
                            ids.remove(index);
                        } else {
                            ids.push(hit);
                        }
                        CanvasAction::Select(ids)
                    }
                    Some(hit) => CanvasAction::Select(vec![hit]),
                    None if shift => CanvasAction::None,
                    None => CanvasAction::Select(Vec::new()),
                },
                _ => CanvasAction::None,
            };
        }
        CanvasAction::None
    }

    fn start_select_drag(
        &mut self,
        world: Point,
        tolerance: f64,
        input: &CanvasInput<'_>,
        shift: bool,
    ) -> Drag {
        match input.document.hit_test(world, tolerance) {
            Some(_) if !shift => Drag::Move { start: world },
            _ => Drag::Marquee { start: world },
        }
    }
}

/// Smooths and simplifies a stroke; a stroke ending near its start
/// becomes a closed outline that can be filled.
fn freehand_shape(stroke: &[Point], zoom: f32, layer: mml_core::LayerId) -> Option<Shape> {
    if stroke.len() < 2 {
        return None;
    }
    let smoothed = mml_core::geometry::smooth_polyline(stroke, 5);
    let tolerance = (0.6 / zoom as f64).max(0.02);
    let mut points = mml_core::geometry::simplify_polyline(&smoothed, tolerance);
    let closing = (4.0 / zoom as f64).max(1.0);
    let closed = points.len() > 3 && points[0].distance(*points.last().unwrap()) <= closing;
    if closed {
        points.pop();
    }
    let path = mml_core::geometry::Path::new(points, closed);
    let bounds = path.bounds();
    if bounds.width().max(bounds.height()) < 0.3 {
        return None;
    }
    let center = bounds.center();
    let local = mml_core::geometry::Path::new(
        path.points.iter().map(|point| *point - center).collect(),
        closed,
    );
    let mut shape = Shape::new(
        tr("shape.freehand"),
        layer,
        ShapeKind::Paths { paths: vec![local] },
    );
    shape.transform = Transform::translate(center.x, center.y);
    Some(shape)
}

fn safe_ratio(value: f64, base: f64) -> f64 {
    if base.abs() < 1e-9 {
        1.0
    } else {
        (value / base).clamp(-1000.0, 1000.0)
    }
}

fn screen_corners(rect: ScreenRect) -> [Pos2; 4] {
    [
        rect.left_bottom(),
        rect.right_bottom(),
        rect.right_top(),
        rect.left_top(),
    ]
}

fn rotate_handle(rect: ScreenRect) -> Pos2 {
    Pos2::new(rect.center().x, rect.top() - 26.0)
}

struct Palette {
    background: Color32,
    bed: Color32,
    bed_border: Color32,
    grid_minor: Color32,
    grid_major: Color32,
    ruler: Color32,
    origin: Color32,
    selection: Color32,
    travel: Color32,
    head: Color32,
}

impl Palette {
    fn new(dark: bool) -> Self {
        if dark {
            Self {
                background: Color32::from_rgb(26, 27, 32),
                bed: Color32::from_rgb(35, 36, 43),
                bed_border: Color32::from_rgb(78, 80, 96),
                grid_minor: Color32::from_rgb(41, 42, 50),
                grid_major: Color32::from_rgb(52, 54, 64),
                ruler: Color32::from_rgb(140, 138, 152),
                origin: crate::theme::DANGER,
                selection: crate::theme::ACCENT,
                travel: Color32::from_rgba_unmultiplied(255, 140, 90, 110),
                head: Color32::from_rgb(255, 110, 80),
            }
        } else {
            Self {
                background: Color32::from_rgb(236, 233, 227),
                bed: Color32::from_rgb(255, 255, 253),
                bed_border: Color32::from_rgb(200, 195, 186),
                grid_minor: Color32::from_rgb(242, 239, 234),
                grid_major: Color32::from_rgb(224, 219, 211),
                ruler: Color32::from_rgb(120, 115, 108),
                origin: crate::theme::DANGER,
                selection: crate::theme::ACCENT,
                travel: Color32::from_rgba_unmultiplied(230, 110, 60, 110),
                head: Color32::from_rgb(220, 70, 40),
            }
        }
    }
}
