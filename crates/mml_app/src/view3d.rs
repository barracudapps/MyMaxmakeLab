//! Orbit 3D view of the toolpath and of the probed surface, drawn with
//! the egui painter (orthographic projection, no GPU code needed).

use eframe::egui::{self, Color32, Pos2, Sense, Stroke, Vec2};
use mml_gcode::{MoveKind, Program};
use mml_probe::HeightMap;

pub struct View3d {
    pub enabled: bool,
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
    pub pan: Vec2,
    /// Z is exaggerated so shallow reliefs and maps stay readable.
    pub z_scale: f32,
    /// Turntable: the view turns slowly around the part.
    pub orbit: bool,
    /// 1 for rotary programs, drawn at true scale around the axis.
    effective_z_scale: f32,
}

impl Default for View3d {
    fn default() -> Self {
        Self {
            enabled: false,
            yaw: -0.6,
            pitch: 1.0,
            zoom: 0.0,
            pan: Vec2::ZERO,
            z_scale: 3.0,
            orbit: false,
            effective_z_scale: 3.0,
        }
    }
}

const MAX_SEGMENTS: usize = 120_000;

impl View3d {
    fn project(&self, point: [f64; 3], center: [f64; 3], scale: f32, origin: Pos2) -> Pos2 {
        let x = (point[0] - center[0]) as f32;
        let y = (point[1] - center[1]) as f32;
        let z = (point[2] - center[2]) as f32 * self.effective_z_scale;
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        let rx = x * cos_yaw - y * sin_yaw;
        let ry = x * sin_yaw + y * cos_yaw;
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        // Pitch 0 looks straight down, 90° from the side: +Z goes up.
        let screen_y = ry * cos_pitch + z * sin_pitch;
        origin + self.pan + Vec2::new(rx, -screen_y) * scale
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        program: Option<&Program>,
        map: Option<&HeightMap>,
        bed: (f64, f64),
    ) {
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let rect = response.rect;
        let dark = ui.visuals().dark_mode;
        painter.rect_filled(
            rect,
            0.0,
            if dark {
                Color32::from_rgb(22, 24, 28)
            } else {
                Color32::from_rgb(232, 235, 240)
            },
        );
        if self.orbit {
            if response.dragged() {
                // Grabbing the view stops the turntable.
                self.orbit = false;
            } else {
                let dt = ui.input(|input| input.stable_dt).min(0.1);
                self.yaw -= dt * 0.5;
                ui.ctx().request_repaint();
            }
        }
        if response.dragged_by(egui::PointerButton::Primary)
            && !ui.input(|input| input.modifiers.shift)
        {
            let delta = response.drag_delta();
            self.yaw -= delta.x * 0.01;
            self.pitch = (self.pitch - delta.y * 0.01).clamp(0.0, std::f32::consts::FRAC_PI_2);
        } else if response.dragged() {
            self.pan += response.drag_delta();
        }
        if response.hovered() {
            let (scroll, pinch) =
                ui.input(|input| (input.smooth_scroll_delta.y, input.zoom_delta()));
            self.zoom = (self.zoom + scroll / 300.0).clamp(-3.0, 5.0) + pinch.ln();
        }
        if response.double_clicked() {
            *self = Self {
                enabled: true,
                ..Self::default()
            };
        }
        // 5-axis programs are shown in part space: every side lands on
        // the part where it was cut.
        let trunnion = program
            .and_then(|program| program.trunnion_table)
            .map(|table_height| mml_core::machine::Trunnion {
                table_height,
                ..Default::default()
            });
        let rotary_radius = program
            .filter(|program| program.uses_rotary && trunnion.is_none())
            .map(|program| {
                program
                    .rotary_diameter
                    .map_or((-program.z_range.0).max(5.0), |diameter| diameter / 2.0)
            });
        self.effective_z_scale = if rotary_radius.is_some() || trunnion.is_some() {
            1.0
        } else {
            self.z_scale
        };
        let bounds = program
            .map(|program| program.bounds())
            .filter(|bounds| !bounds.is_empty());
        let (min_x, min_y, max_x, max_y) = match bounds {
            Some(bounds) => (bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y),
            None => (0.0, 0.0, bed.0, bed.1),
        };
        let (center, extent) = match rotary_radius {
            // Part space, origin at the centre of the table.
            _ if trunnion.is_some() => {
                ([0.0; 3], (max_x - min_x).max(max_y - min_y).max(1.0) as f32)
            }
            // The stock turns around X: centre on the axis.
            Some(radius) => (
                [(min_x + max_x) / 2.0, 0.0, 0.0],
                (max_x - min_x).max(2.0 * radius).max(1.0) as f32,
            ),
            None => (
                [(min_x + max_x) / 2.0, (min_y + max_y) / 2.0, 0.0],
                (max_x - min_x).max(max_y - min_y).max(1.0) as f32,
            ),
        };
        let scale = rect.width().min(rect.height()) * 0.8 / extent * self.zoom.exp();
        let origin = rect.center();
        let painter = painter.with_clip_rect(rect);
        let bed_color = if dark {
            Color32::from_rgb(70, 76, 88)
        } else {
            Color32::from_rgb(170, 176, 188)
        };
        let corners = [
            [0.0, 0.0, 0.0],
            [bed.0, 0.0, 0.0],
            [bed.0, bed.1, 0.0],
            [0.0, bed.1, 0.0],
        ];
        for index in (0..4).filter(|_| rotary_radius.is_none() && trunnion.is_none()) {
            let a = self.project(corners[index], center, scale, origin);
            let b = self.project(corners[(index + 1) % 4], center, scale, origin);
            painter.line_segment([a, b], Stroke::new(1.0, bed_color));
        }
        let axis = |end: [f64; 3], color: Color32| {
            painter.line_segment(
                [
                    self.project([0.0; 3], center, scale, origin),
                    self.project(end, center, scale, origin),
                ],
                Stroke::new(2.0, color),
            );
        };
        axis([20.0, 0.0, 0.0], Color32::from_rgb(220, 60, 60));
        axis([0.0, 20.0, 0.0], Color32::from_rgb(60, 180, 80));
        axis(
            [0.0, 0.0, 20.0 / self.z_scale as f64],
            Color32::from_rgb(60, 120, 230),
        );

        if let Some(map) = map {
            let color = Color32::from_rgba_unmultiplied(230, 160, 40, 160);
            for row in 0..map.rows {
                for column in 0..map.columns {
                    let Some(z) = map.heights[row * map.columns + column] else {
                        continue;
                    };
                    let node = map.node(column, row);
                    let here = self.project([node.x, node.y, z], center, scale, origin);
                    for (next_column, next_row) in [(column + 1, row), (column, row + 1)] {
                        if next_column < map.columns
                            && next_row < map.rows
                            && let Some(next_z) = map.heights[next_row * map.columns + next_column]
                        {
                            let next = map.node(next_column, next_row);
                            let there =
                                self.project([next.x, next.y, next_z], center, scale, origin);
                            painter.line_segment([here, there], Stroke::new(1.0, color));
                        }
                    }
                }
            }
        }

        let Some(program) = program else { return };
        let (low, high) = program.z_range;
        let span = (high - low).max(1e-6);
        let stride = (program.motions.len() / MAX_SEGMENTS).max(1);
        for (index, motion) in program.motions.iter().enumerate() {
            let rapid = motion.kind == MoveKind::Rapid;
            if !rapid && !motion.is_cutting() || (stride > 1 && index % stride != 0 && !rapid) {
                continue;
            }
            let color = if rapid {
                Color32::from_rgba_unmultiplied(255, 120, 60, 60)
            } else {
                let depth = ((high - motion.to[2]) / span) as f32;
                Color32::from_rgb(
                    (40.0 + 60.0 * (1.0 - depth)) as u8,
                    (140.0 + 80.0 * (1.0 - depth)) as u8,
                    (255.0 - 90.0 * depth) as u8,
                )
            };
            let stroke = Stroke::new(if rapid { 0.6 } else { 1.0 }, color);
            if let Some(trunnion) = trunnion {
                let a = trunnion.to_part(motion.from, motion.a[0], motion.c[0]);
                let b = trunnion.to_part(motion.to, motion.a[1], motion.c[1]);
                painter.line_segment(
                    [
                        self.project(a, center, scale, origin),
                        self.project(b, center, scale, origin),
                    ],
                    stroke,
                );
                continue;
            }
            let Some(radius) = rotary_radius else {
                let a = self.project(motion.from, center, scale, origin);
                let b = self.project(motion.to, center, scale, origin);
                painter.line_segment([a, b], stroke);
                continue;
            };
            // Large rotations are split so they are drawn as arcs.
            let pieces = ((motion.a[1] - motion.a[0]).abs() / 5.0).ceil().max(1.0) as usize;
            let at = |t: f64| {
                let point = [0, 1, 2]
                    .map(|axis| motion.from[axis] + (motion.to[axis] - motion.from[axis]) * t);
                let angle = motion.a[0] + (motion.a[1] - motion.a[0]) * t;
                self.project(
                    mml_gcode::unwrap_rotary(point, angle, radius),
                    center,
                    scale,
                    origin,
                )
            };
            let mut previous = at(0.0);
            for piece in 1..=pieces {
                let next = at(piece as f64 / pieces as f64);
                painter.line_segment([previous, next], stroke);
                previous = next;
            }
        }
    }
}
