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
}

#[derive(Debug, Clone, Copy)]
enum Drag {
    None,
    Move { start: Point },
    Scale { pivot: Point, start: Point },
    Rotate { center: Point, start_angle: f64 },
    Marquee { start: Point },
    Create { start: Point },
}

/// What the canvas asks the application to do this frame.
pub enum CanvasAction {
    None,
    Select(Vec<ShapeId>),
    /// Commit `transform` to the selected shapes (one undo step).
    Transform(Transform),
    Create(Shape),
    TextAt(Point),
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
}

const HANDLE: f32 = 7.0;

impl Canvas {
    pub fn request_fit(&mut self) {
        self.fit_requested = true;
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
            let area = if input.document.shapes.is_empty() {
                bed
            } else {
                bed.union(input.document.bounds())
            };
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
        if let Some(position) = self.machine_position {
            let center = view.to_screen(position);
            let stroke = Stroke::new(1.5, palette.head);
            painter.circle_stroke(center, 7.0, stroke);
            painter.line_segment([center - Vec2::X * 12.0, center + Vec2::X * 12.0], stroke);
            painter.line_segment([center - Vec2::Y * 12.0, center + Vec2::Y * 12.0], stroke);
        }
        if input.show_preview {
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
                background: Color32::from_rgb(24, 26, 30),
                bed: Color32::from_rgb(36, 39, 45),
                bed_border: Color32::from_rgb(90, 96, 108),
                grid_minor: Color32::from_rgb(44, 48, 55),
                grid_major: Color32::from_rgb(60, 65, 75),
                ruler: Color32::from_rgb(140, 146, 158),
                origin: Color32::from_rgb(230, 80, 70),
                selection: Color32::from_rgb(80, 160, 255),
                travel: Color32::from_rgba_unmultiplied(255, 120, 60, 120),
                head: Color32::from_rgb(255, 90, 60),
            }
        } else {
            Self {
                background: Color32::from_rgb(226, 229, 234),
                bed: Color32::from_rgb(252, 252, 250),
                bed_border: Color32::from_rgb(150, 155, 165),
                grid_minor: Color32::from_rgb(236, 238, 241),
                grid_major: Color32::from_rgb(214, 218, 224),
                ruler: Color32::from_rgb(100, 106, 118),
                origin: Color32::from_rgb(210, 50, 40),
                selection: Color32::from_rgb(20, 110, 230),
                travel: Color32::from_rgba_unmultiplied(230, 90, 30, 110),
                head: Color32::from_rgb(220, 50, 30),
            }
        }
    }
}
