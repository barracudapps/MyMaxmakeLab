use serde::{Deserialize, Serialize};
use std::ops::{Add, Mul, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn distance(self, other: Point) -> f64 {
        (self - other).length()
    }

    pub fn length(self) -> f64 {
        self.x.hypot(self.y)
    }

    pub fn dot(self, other: Point) -> f64 {
        self.x * other.x + self.y * other.y
    }

    pub fn cross(self, other: Point) -> f64 {
        self.x * other.y - self.y * other.x
    }

    pub fn normalized(self) -> Point {
        let length = self.length();
        if length <= f64::EPSILON {
            Point::default()
        } else {
            self * (1.0 / length)
        }
    }

    pub fn perpendicular(self) -> Point {
        Point::new(-self.y, self.x)
    }

    pub fn lerp(self, other: Point, t: f64) -> Point {
        self + (other - self) * t
    }
}

impl Add for Point {
    type Output = Point;
    fn add(self, rhs: Point) -> Point {
        Point::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Point {
    type Output = Point;
    fn sub(self, rhs: Point) -> Point {
        Point::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Mul<f64> for Point {
    type Output = Point;
    fn mul(self, rhs: f64) -> Point {
        Point::new(self.x * rhs, self.y * rhs)
    }
}

/// Axis-aligned rectangle; an empty rectangle has `min > max`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub min: Point,
    pub max: Point,
}

impl Default for Rect {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl Rect {
    pub const EMPTY: Rect = Rect {
        min: Point::new(f64::INFINITY, f64::INFINITY),
        max: Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY),
    };

    pub fn from_points(a: Point, b: Point) -> Self {
        Rect {
            min: Point::new(a.x.min(b.x), a.y.min(b.y)),
            max: Point::new(a.x.max(b.x), a.y.max(b.y)),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.min.x > self.max.x || self.min.y > self.max.y
    }

    pub fn width(&self) -> f64 {
        if self.is_empty() {
            0.0
        } else {
            self.max.x - self.min.x
        }
    }

    pub fn height(&self) -> f64 {
        if self.is_empty() {
            0.0
        } else {
            self.max.y - self.min.y
        }
    }

    pub fn center(&self) -> Point {
        self.min.lerp(self.max, 0.5)
    }

    pub fn include(&mut self, point: Point) {
        self.min.x = self.min.x.min(point.x);
        self.min.y = self.min.y.min(point.y);
        self.max.x = self.max.x.max(point.x);
        self.max.y = self.max.y.max(point.y);
    }

    pub fn union(mut self, other: Rect) -> Rect {
        if !other.is_empty() {
            self.include(other.min);
            self.include(other.max);
        }
        self
    }

    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        !(other.min.x > self.max.x
            || other.max.x < self.min.x
            || other.min.y > self.max.y
            || other.max.y < self.min.y)
    }

    pub fn corners(&self) -> [Point; 4] {
        [
            self.min,
            Point::new(self.max.x, self.min.y),
            self.max,
            Point::new(self.min.x, self.max.y),
        ]
    }
}

/// 2D affine transform `[a c e; b d f; 0 0 1]`, same layout as SVG.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    pub const IDENTITY: Transform = Transform {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    pub fn translate(dx: f64, dy: f64) -> Self {
        Transform {
            e: dx,
            f: dy,
            ..Self::IDENTITY
        }
    }

    pub fn scale(sx: f64, sy: f64) -> Self {
        Transform {
            a: sx,
            d: sy,
            ..Self::IDENTITY
        }
    }

    pub fn rotate_degrees(angle: f64) -> Self {
        let (sin, cos) = angle.to_radians().sin_cos();
        Transform {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            e: 0.0,
            f: 0.0,
        }
    }

    /// Transform that applies `self` first, then `next`.
    pub fn then(self, next: Transform) -> Transform {
        Transform {
            a: next.a * self.a + next.c * self.b,
            b: next.b * self.a + next.d * self.b,
            c: next.a * self.c + next.c * self.d,
            d: next.b * self.c + next.d * self.d,
            e: next.a * self.e + next.c * self.f + next.e,
            f: next.b * self.e + next.d * self.f + next.f,
        }
    }

    /// Wraps `self` so that it acts around `pivot` instead of the origin.
    pub fn around(self, pivot: Point) -> Transform {
        Transform::translate(-pivot.x, -pivot.y)
            .then(self)
            .then(Transform::translate(pivot.x, pivot.y))
    }

    pub fn apply(&self, point: Point) -> Point {
        Point::new(
            self.a * point.x + self.c * point.y + self.e,
            self.b * point.x + self.d * point.y + self.f,
        )
    }

    pub fn determinant(&self) -> f64 {
        self.a * self.d - self.b * self.c
    }

    pub fn inverse(&self) -> Option<Transform> {
        let det = self.determinant();
        if det.abs() < 1e-12 {
            return None;
        }
        let a = self.d / det;
        let b = -self.b / det;
        let c = -self.c / det;
        let d = self.a / det;
        Some(Transform {
            a,
            b,
            c,
            d,
            e: -(a * self.e + c * self.f),
            f: -(b * self.e + d * self.f),
        })
    }

    pub fn rotation_degrees(&self) -> f64 {
        self.b.atan2(self.a).to_degrees()
    }

    pub fn scale_factors(&self) -> (f64, f64) {
        let sx = self.a.hypot(self.b);
        let sy = if sx > 0.0 {
            self.determinant() / sx
        } else {
            self.c.hypot(self.d)
        };
        (sx, sy)
    }
}

/// Polyline; curves are flattened when shapes are built or imported.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Path {
    pub points: Vec<Point>,
    pub closed: bool,
}

impl Path {
    pub fn new(points: Vec<Point>, closed: bool) -> Self {
        Self { points, closed }
    }

    pub fn bounds(&self) -> Rect {
        let mut rect = Rect::EMPTY;
        for point in &self.points {
            rect.include(*point);
        }
        rect
    }

    pub fn transformed(&self, transform: &Transform) -> Path {
        Path {
            points: self
                .points
                .iter()
                .map(|point| transform.apply(*point))
                .collect(),
            closed: self.closed,
        }
    }

    pub fn segments(&self) -> impl Iterator<Item = (Point, Point)> + '_ {
        let closing = (self.closed && self.points.len() > 2)
            .then(|| (*self.points.last().unwrap(), self.points[0]));
        self.points
            .windows(2)
            .map(|pair| (pair[0], pair[1]))
            .chain(closing)
    }

    pub fn length(&self) -> f64 {
        self.segments().map(|(a, b)| a.distance(b)).sum()
    }

    /// Signed area, positive when the path runs counter-clockwise.
    pub fn signed_area(&self) -> f64 {
        if self.points.len() < 3 {
            return 0.0;
        }
        let mut sum = 0.0;
        for (index, point) in self.points.iter().enumerate() {
            let next = self.points[(index + 1) % self.points.len()];
            sum += point.cross(next);
        }
        sum / 2.0
    }

    pub fn reversed(&self) -> Path {
        let mut points = self.points.clone();
        points.reverse();
        Path {
            points,
            closed: self.closed,
        }
    }

    pub fn distance_to(&self, point: Point) -> f64 {
        self.segments()
            .map(|(a, b)| distance_to_segment(point, a, b))
            .fold(f64::INFINITY, f64::min)
    }

    /// Removes consecutive duplicates and the closing duplicate point.
    pub fn cleaned(mut self, tolerance: f64) -> Path {
        self.points.dedup_by(|a, b| a.distance(*b) <= tolerance);
        if self.closed && self.points.len() > 1 {
            let first = self.points[0];
            if self
                .points
                .last()
                .is_some_and(|last| last.distance(first) <= tolerance)
            {
                self.points.pop();
            }
        }
        self
    }
}

pub fn distance_to_segment(point: Point, a: Point, b: Point) -> f64 {
    let ab = b - a;
    let length_squared = ab.dot(ab);
    if length_squared <= f64::EPSILON {
        return point.distance(a);
    }
    let t = ((point - a).dot(ab) / length_squared).clamp(0.0, 1.0);
    point.distance(a + ab * t)
}

/// Even-odd point-in-polygon test over several closed paths.
pub fn contains_even_odd(paths: &[Path], point: Point) -> bool {
    let mut inside = false;
    for path in paths.iter().filter(|path| path.closed) {
        let count = path.points.len();
        for index in 0..count {
            let a = path.points[index];
            let b = path.points[(index + 1) % count];
            if (a.y > point.y) != (b.y > point.y) {
                let x = a.x + (point.y - a.y) / (b.y - a.y) * (b.x - a.x);
                if x > point.x {
                    inside = !inside;
                }
            }
        }
    }
    inside
}

pub fn bounds_of(paths: &[Path]) -> Rect {
    paths
        .iter()
        .fold(Rect::EMPTY, |rect, path| rect.union(path.bounds()))
}

/// Number of segments so that the chord error stays below `tolerance`.
pub fn arc_segments(radius: f64, sweep_radians: f64, tolerance: f64) -> usize {
    if radius <= tolerance {
        return 4;
    }
    let step = 2.0 * (1.0 - tolerance / radius).clamp(-1.0, 1.0).acos();
    ((sweep_radians.abs() / step).ceil() as usize).clamp(4, 4096)
}

pub fn flatten_cubic(p0: Point, p1: Point, p2: Point, p3: Point, tolerance: f64) -> Vec<Point> {
    let hull = p0.distance(p1) + p1.distance(p2) + p2.distance(p3);
    let steps = ((hull / tolerance.max(1e-4)).sqrt().ceil() as usize).clamp(2, 512);
    (1..=steps)
        .map(|step| {
            let t = step as f64 / steps as f64;
            let u = 1.0 - t;
            p0 * (u * u * u) + p1 * (3.0 * u * u * t) + p2 * (3.0 * u * t * t) + p3 * (t * t * t)
        })
        .collect()
}

pub fn flatten_quadratic(p0: Point, p1: Point, p2: Point, tolerance: f64) -> Vec<Point> {
    flatten_cubic(
        p0,
        p0.lerp(p1, 2.0 / 3.0),
        p2.lerp(p1, 2.0 / 3.0),
        p2,
        tolerance,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transform_inverse_round_trips() {
        let transform = Transform::rotate_degrees(30.0)
            .then(Transform::scale(2.0, 0.5))
            .then(Transform::translate(10.0, -4.0));
        let point = Point::new(3.0, 7.0);
        let back = transform.inverse().unwrap().apply(transform.apply(point));
        assert!(back.distance(point) < 1e-9);
    }

    #[test]
    fn even_odd_handles_holes() {
        let square = |size: f64| {
            Path::new(
                vec![
                    Point::new(-size, -size),
                    Point::new(size, -size),
                    Point::new(size, size),
                    Point::new(-size, size),
                ],
                true,
            )
        };
        let paths = [square(10.0), square(5.0)];
        assert!(!contains_even_odd(&paths, Point::new(0.0, 0.0)));
        assert!(contains_even_odd(&paths, Point::new(7.0, 0.0)));
    }
}
