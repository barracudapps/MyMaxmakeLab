use crate::geometry::{Path, Point, Rect, Transform, arc_segments, bounds_of, contains_even_odd};
use crate::layer::{LAYER_COLORS, Layer, LayerId, LineParams, Operation};
use serde::{Deserialize, Serialize};
use std::f64::consts::TAU;
use std::sync::Arc;
use uuid::Uuid;

/// Flattening tolerance used for primitives, in mm.
pub const CURVE_TOLERANCE: f64 = 0.01;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ShapeId(pub Uuid);

impl ShapeId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ShapeId {
    fn default() -> Self {
        Self::new()
    }
}

/// 8-bit greyscale picture, 0 = black. Shared because undo snapshots and
/// toolpath workers clone documents often.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RasterImage {
    pub width: u32,
    pub height: u32,
    #[serde(with = "base64_bytes")]
    pub pixels: Vec<u8>,
}

impl RasterImage {
    pub fn pixel(&self, x: u32, y: u32) -> u8 {
        self.pixels[(y * self.width + x) as usize]
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ShapeKind {
    /// Rectangle centred on the local origin.
    Rectangle {
        width: f64,
        height: f64,
        corner_radius: f64,
    },
    Ellipse {
        rx: f64,
        ry: f64,
    },
    /// Regular polygon inscribed in a circle of `radius`.
    Polygon {
        sides: u32,
        radius: f64,
    },
    Paths {
        paths: Vec<Path>,
    },
    /// Image centred on the local origin, `width` x `height` mm.
    Image {
        image: Arc<RasterImage>,
        width: f64,
        height: f64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    pub id: ShapeId,
    pub name: String,
    pub layer: LayerId,
    pub kind: ShapeKind,
    pub transform: Transform,
    pub locked: bool,
}

impl Shape {
    pub fn new(name: impl Into<String>, layer: LayerId, kind: ShapeKind) -> Self {
        Self {
            id: ShapeId::new(),
            name: name.into(),
            layer,
            kind,
            transform: Transform::IDENTITY,
            locked: false,
        }
    }

    pub fn is_image(&self) -> bool {
        matches!(self.kind, ShapeKind::Image { .. })
    }

    /// Outline in local coordinates (for images: the picture frame).
    pub fn local_paths(&self) -> Vec<Path> {
        match &self.kind {
            ShapeKind::Rectangle {
                width,
                height,
                corner_radius,
            } => {
                vec![rounded_rectangle(*width, *height, *corner_radius)]
            }
            ShapeKind::Ellipse { rx, ry } => {
                let count = arc_segments(rx.max(*ry), TAU, CURVE_TOLERANCE);
                let points = (0..count)
                    .map(|index| {
                        let angle = TAU * index as f64 / count as f64;
                        Point::new(rx * angle.cos(), ry * angle.sin())
                    })
                    .collect();
                vec![Path::new(points, true)]
            }
            ShapeKind::Polygon { sides, radius } => {
                let sides = (*sides).max(3);
                let points = (0..sides)
                    .map(|index| {
                        let angle = TAU * index as f64 / sides as f64 + TAU / 4.0;
                        Point::new(radius * angle.cos(), radius * angle.sin())
                    })
                    .collect();
                vec![Path::new(points, true)]
            }
            ShapeKind::Paths { paths } => paths.clone(),
            ShapeKind::Image { width, height, .. } => vec![rounded_rectangle(*width, *height, 0.0)],
        }
    }

    pub fn world_paths(&self) -> Vec<Path> {
        self.local_paths()
            .iter()
            .map(|path| path.transformed(&self.transform))
            .collect()
    }

    pub fn bounds(&self) -> Rect {
        bounds_of(&self.world_paths())
    }

    /// True when `point` (world) lies on or inside the shape.
    pub fn hit(&self, point: Point, tolerance: f64) -> bool {
        let Some(inverse) = self.transform.inverse() else {
            return false;
        };
        let local = inverse.apply(point);
        let local_paths = self.local_paths();
        let (sx, sy) = self.transform.scale_factors();
        let local_tolerance = tolerance / sx.abs().min(sy.abs()).max(1e-9);
        self.is_image()
            || contains_even_odd(&local_paths, local)
            || local_paths
                .iter()
                .any(|path| path.distance_to(local) <= local_tolerance)
    }
}

fn rounded_rectangle(width: f64, height: f64, corner_radius: f64) -> Path {
    let (hw, hh) = (width / 2.0, height / 2.0);
    let radius = corner_radius.clamp(0.0, hw.min(hh));
    if radius <= 0.0 {
        return Path::new(
            vec![
                Point::new(-hw, -hh),
                Point::new(hw, -hh),
                Point::new(hw, hh),
                Point::new(-hw, hh),
            ],
            true,
        );
    }
    let per_corner = arc_segments(radius, TAU / 4.0, CURVE_TOLERANCE);
    let centers = [
        (Point::new(hw - radius, -hh + radius), -TAU / 4.0),
        (Point::new(hw - radius, hh - radius), 0.0),
        (Point::new(-hw + radius, hh - radius), TAU / 4.0),
        (Point::new(-hw + radius, -hh + radius), TAU / 2.0),
    ];
    let mut points = Vec::with_capacity(4 * (per_corner + 1));
    for (center, start) in centers {
        for step in 0..=per_corner {
            let angle = start + TAU / 4.0 * step as f64 / per_corner as f64;
            points.push(center + Point::new(angle.cos(), angle.sin()) * radius);
        }
    }
    Path::new(points, true)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub layers: Vec<Layer>,
    pub shapes: Vec<Shape>,
    /// Instructions of a ready-made project, step by step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guide: Option<crate::guide::Guide>,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            layers: vec![Layer::new(
                "Cut",
                LAYER_COLORS[0],
                Operation::LaserLine(LineParams::default()),
            )],
            shapes: Vec::new(),
            guide: None,
        }
    }
}

impl Document {
    pub fn layer(&self, id: LayerId) -> Option<&Layer> {
        self.layers.iter().find(|layer| layer.id == id)
    }

    pub fn layer_mut(&mut self, id: LayerId) -> Option<&mut Layer> {
        self.layers.iter_mut().find(|layer| layer.id == id)
    }

    pub fn shape(&self, id: ShapeId) -> Option<&Shape> {
        self.shapes.iter().find(|shape| shape.id == id)
    }

    pub fn shape_mut(&mut self, id: ShapeId) -> Option<&mut Shape> {
        self.shapes.iter_mut().find(|shape| shape.id == id)
    }

    /// Palette colour farthest from the colours already in use, so a new
    /// layer never looks like an existing one (imported SVG colours are
    /// rarely exactly the palette ones).
    pub fn next_layer_color(&self) -> [u8; 3] {
        let distance = |a: [u8; 3], b: [u8; 3]| {
            (0..3)
                .map(|channel| (a[channel] as i32 - b[channel] as i32).pow(2))
                .sum::<i32>()
        };
        LAYER_COLORS
            .iter()
            .copied()
            .max_by_key(|candidate| {
                self.layers
                    .iter()
                    .map(|layer| distance(*candidate, layer.color))
                    .min()
                    .unwrap_or(i32::MAX)
            })
            .unwrap_or(LAYER_COLORS[0])
    }

    pub fn shapes_on(&self, layer: LayerId) -> impl Iterator<Item = &Shape> {
        self.shapes.iter().filter(move |shape| shape.layer == layer)
    }

    /// Removes a layer and moves its shapes to the first remaining layer.
    pub fn remove_layer(&mut self, id: LayerId) {
        if self.layers.len() <= 1 {
            return;
        }
        self.layers.retain(|layer| layer.id != id);
        let fallback = self.layers[0].id;
        for shape in self.shapes.iter_mut().filter(|shape| shape.layer == id) {
            shape.layer = fallback;
        }
    }

    pub fn bounds(&self) -> Rect {
        self.shapes
            .iter()
            .fold(Rect::EMPTY, |rect, shape| rect.union(shape.bounds()))
    }

    pub fn bounds_of(&self, ids: &[ShapeId]) -> Rect {
        self.shapes
            .iter()
            .filter(|shape| ids.contains(&shape.id))
            .fold(Rect::EMPTY, |rect, shape| rect.union(shape.bounds()))
    }

    /// Topmost visible, unlocked shape under `point`.
    pub fn hit_test(&self, point: Point, tolerance: f64) -> Option<ShapeId> {
        self.shapes
            .iter()
            .rev()
            .filter(|shape| !shape.locked)
            .filter(|shape| self.layer(shape.layer).is_some_and(|layer| layer.visible))
            .find(|shape| shape.hit(point, tolerance))
            .map(|shape| shape.id)
    }
}

mod base64_bytes {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(deserializer)?;
        STANDARD.decode(text).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_layer_colour_differs_from_close_existing_colours() {
        let mut document = Document::default();
        document.layers[0].color = [31, 119, 180];
        let color = document.next_layer_color();
        let distance: i32 = (0..3)
            .map(|channel| (color[channel] as i32 - [31, 119, 180][channel]).pow(2))
            .sum();
        assert!(distance > 100 * 100);
    }

    #[test]
    fn rotated_rectangle_bounds_and_hit() {
        let document = Document::default();
        let mut shape = Shape::new(
            "r",
            document.layers[0].id,
            ShapeKind::Rectangle {
                width: 20.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );
        shape.transform = Transform::rotate_degrees(90.0).then(Transform::translate(50.0, 50.0));
        let bounds = shape.bounds();
        assert!((bounds.width() - 10.0).abs() < 1e-9);
        assert!((bounds.height() - 20.0).abs() < 1e-9);
        assert!(shape.hit(Point::new(50.0, 58.0), 0.1));
        assert!(!shape.hit(Point::new(58.0, 50.0), 0.1));
    }
}
