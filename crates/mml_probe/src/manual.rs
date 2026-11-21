//! Manual measurement, for machines without a 3D probe: the operator
//! jogs the tool (or a flexible rod held in the collet) until it just
//! touches the stock, and records the machine position with one click.
//! Each stock shape asks for the fewest touches that define it.

use mml_core::geometry::{Point, Rect};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StockShape {
    /// Only the top face: sets Z zero.
    Surface,
    /// Top face measured in 3 points or more: Z zero and tilt.
    TiltedSurface,
    Cube,
    /// Rectangular block of any proportions.
    Block,
    /// Cylinder standing on its end.
    CylinderUpright,
    /// Cylinder lying along X (in a vise or V blocks).
    CylinderLying,
    /// Prism along X whose section narrows from the base to the top
    /// (both sides sloping by the same angle).
    Trapezoid,
}

impl StockShape {
    pub const ALL: [StockShape; 7] = [
        StockShape::Surface,
        StockShape::TiltedSurface,
        StockShape::Cube,
        StockShape::Block,
        StockShape::CylinderUpright,
        StockShape::CylinderLying,
        StockShape::Trapezoid,
    ];

    /// Translation key of the shape name.
    pub fn key(self) -> &'static str {
        match self {
            StockShape::Surface => "manual.shape_surface",
            StockShape::TiltedSurface => "manual.shape_tilted",
            StockShape::Cube => "manual.shape_cube",
            StockShape::Block => "manual.shape_block",
            StockShape::CylinderUpright => "manual.shape_cylinder_upright",
            StockShape::CylinderLying => "manual.shape_cylinder_lying",
            StockShape::Trapezoid => "manual.shape_trapezoid",
        }
    }

    /// Touches in the order they are asked; the optional ones only add
    /// information (size, squareness, height).
    pub fn steps(self) -> Vec<Step> {
        use Touch::*;
        let required = |touch| Step {
            touch,
            optional: false,
        };
        let optional = |touch| Step {
            touch,
            optional: true,
        };
        match self {
            StockShape::Surface => vec![required(Top)],
            StockShape::TiltedSurface => vec![
                required(TopAt(0)),
                required(TopAt(1)),
                required(TopAt(2)),
                optional(TopAt(3)),
            ],
            StockShape::Cube => vec![
                required(Top),
                required(Left),
                required(Front),
                required(Right),
                optional(FrontSecond),
            ],
            StockShape::Block => vec![
                required(Top),
                required(Left),
                required(Front),
                optional(Right),
                optional(Back),
                optional(Bed),
                optional(FrontSecond),
            ],
            StockShape::CylinderUpright => vec![
                required(Top),
                required(Rim(0)),
                required(Rim(1)),
                required(Rim(2)),
                optional(Bed),
            ],
            StockShape::CylinderLying => vec![
                required(Front),
                required(Back),
                required(Top),
                required(Left),
                optional(Right),
            ],
            StockShape::Trapezoid => vec![
                required(Top),
                required(Left),
                required(FrontLow),
                required(FrontHigh),
                required(BackLow),
                optional(Right),
                optional(Bed),
            ],
        }
    }

    /// Default origin for the shape: the centre for cylinders, the
    /// front-left corner otherwise.
    pub fn default_origin(self) -> Origin {
        match self {
            StockShape::CylinderUpright => Origin::Center,
            _ => Origin::FrontLeft,
        }
    }
}

/// Where the tool touches, which says how the contact is compensated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Touch {
    /// Tool tip on the top face.
    Top,
    /// Tool tip on the top face, n-th point of a tilted surface.
    TopAt(u8),
    /// Tool side against the left face (tool moving towards +X).
    Left,
    Right,
    /// Tool side against the front face (tool moving towards +Y).
    Front,
    /// A second point on the front face, further right: squareness.
    FrontSecond,
    Back,
    /// Tool side against the cylinder, n-th of 3 points spread around.
    Rim(u8),
    /// Tool tip against the sloping front side, low then high.
    FrontLow,
    FrontHigh,
    BackLow,
    /// Tool tip on the table (or vise jaw) beside the stock: height.
    Bed,
}

impl Touch {
    /// Translation key of the instruction shown for this touch.
    pub fn key(self) -> &'static str {
        match self {
            Touch::Top | Touch::TopAt(_) => "manual.touch_top",
            Touch::Left => "manual.touch_left",
            Touch::Right => "manual.touch_right",
            Touch::Front => "manual.touch_front",
            Touch::FrontSecond => "manual.touch_front_second",
            Touch::Back => "manual.touch_back",
            Touch::Rim(_) => "manual.touch_rim",
            Touch::FrontLow => "manual.touch_front_low",
            Touch::FrontHigh => "manual.touch_front_high",
            Touch::BackLow => "manual.touch_back_low",
            Touch::Bed => "manual.touch_bed",
        }
    }

    /// Number shown after the instruction for repeated touches.
    pub fn index(self) -> Option<u8> {
        match self {
            Touch::TopAt(n) | Touch::Rim(n) => Some(n + 1),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    pub touch: Touch,
    pub optional: bool,
}

/// What touches the stock and what sits between them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Contact {
    /// Diameter of the tool or rod touching the sides, mm.
    pub diameter: f64,
    /// Thickness of the shim slid between tool and stock (a sheet of
    /// paper is about 0.1 mm), 0 for a direct touch, mm.
    pub shim: f64,
}

impl Default for Contact {
    fn default() -> Self {
        Self {
            diameter: 6.0,
            shim: 0.1,
        }
    }
}

impl Contact {
    /// Distance from the tool axis to a face it touches with its side.
    fn side(&self) -> f64 {
        self.diameter / 2.0 + self.shim
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Origin {
    /// Front-left corner (left end and axis for a lying cylinder), top.
    FrontLeft,
    /// Centre of the top face (axis for cylinders), top.
    Center,
}

/// Result of a measurement, in machine coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measure {
    pub top: f64,
    /// Lowest point of the stock when known (table touched, or deduced
    /// from a cube or a lying cylinder).
    pub bottom: Option<f64>,
    pub left: Option<f64>,
    pub right: Option<f64>,
    pub front: Option<f64>,
    pub back: Option<f64>,
    /// Upright cylinder: centre and diameter; lying cylinder: diameter
    /// (the axis is half-way between front and back).
    pub center: Option<Point>,
    pub diameter: Option<f64>,
    /// Trapezoid: width of the top face and slope of the sides from
    /// the vertical, degrees.
    pub top_width: Option<f64>,
    pub side_angle: Option<f64>,
    /// How far the front face turns from the X axis, degrees, when a
    /// second front point was taken.
    pub skew: Option<f64>,
    /// Tilted surface: `z = a·x + b·y + c`, and its largest residual.
    pub plane: Option<(crate::Plane, f64)>,
}

impl Measure {
    fn new(top: f64) -> Self {
        Self {
            top,
            bottom: None,
            left: None,
            right: None,
            front: None,
            back: None,
            center: None,
            diameter: None,
            top_width: None,
            side_angle: None,
            skew: None,
            plane: None,
        }
    }

    pub fn width(&self) -> Option<f64> {
        Some(self.right? - self.left?)
    }

    pub fn depth(&self) -> Option<f64> {
        Some(self.back? - self.front?)
    }

    pub fn height(&self) -> Option<f64> {
        Some(self.top - self.bottom?)
    }

    /// Footprint in machine XY, when fully known.
    pub fn footprint(&self) -> Option<Rect> {
        Some(Rect::from_points(
            Point::new(self.left?, self.front?),
            Point::new(self.right?, self.back?),
        ))
    }

    /// Machine coordinates of the chosen origin; `None` when the
    /// measurement does not define it (centre without the far sides).
    pub fn origin(&self, origin: Origin) -> Option<[f64; 3]> {
        if let Some(center) = self.center {
            return Some([center.x, center.y, self.top]);
        }
        match origin {
            Origin::FrontLeft => {
                let y = match self.diameter {
                    // Lying cylinder: the axis is the natural Y zero.
                    Some(_) => (self.front? + self.back?) / 2.0,
                    None => self.front?,
                };
                Some([self.left?, y, self.top])
            }
            Origin::Center => Some([
                (self.left? + self.right?) / 2.0,
                (self.front? + self.back?) / 2.0,
                self.top,
            ]),
        }
    }

    /// Whether the touches located the stock in X and Y, not only its
    /// top.
    pub fn locates_xy(&self) -> bool {
        self.left.is_some() || self.center.is_some()
    }

    /// G-code setting the work zero (G54) on the chosen origin, or only
    /// Z when the sides were not touched. The offset is given in machine
    /// coordinates, so it does not depend on where the head is now.
    pub fn origin_program(&self, origin: Origin) -> Option<Vec<String>> {
        if !self.locates_xy() {
            return Some(vec![format!("G10 L2 P1 Z{:.3}", self.top)]);
        }
        let [x, y, z] = self.origin(origin)?;
        Some(vec![format!("G10 L2 P1 X{x:.3} Y{y:.3} Z{z:.3}")])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolveError {
    /// A required touch has not been recorded yet.
    Missing(Touch),
    /// The points contradict the shape (sides crossed, points aligned).
    Inconsistent,
}

/// Turns recorded tool positions (machine coordinates of the tool tip,
/// on its axis) into stock faces, compensating the tool radius and
/// the shim.
pub fn solve(
    shape: StockShape,
    touches: &[(Touch, [f64; 3])],
    contact: Contact,
) -> Result<Measure, SolveError> {
    let get = |touch: Touch| touches.iter().find(|(t, _)| *t == touch).map(|(_, p)| *p);
    let need = |touch: Touch| get(touch).ok_or(SolveError::Missing(touch));
    let side = contact.side();
    let tip = |p: [f64; 3]| p[2] - contact.shim;
    let mut measure = match shape {
        StockShape::TiltedSurface => {
            let points: Vec<[f64; 3]> = (0..4)
                .filter_map(|n| get(Touch::TopAt(n)))
                .map(|p| [p[0], p[1], tip(p)])
                .collect();
            for n in 0..3 {
                need(Touch::TopAt(n))?;
            }
            let plane = crate::Plane::fit(&points).ok_or(SolveError::Inconsistent)?;
            let residual = plane.residual(&points);
            let top = points
                .iter()
                .map(|p| p[2])
                .fold(f64::NEG_INFINITY, f64::max);
            let mut measure = Measure::new(top);
            measure.plane = Some((plane, residual));
            return Ok(measure);
        }
        _ => Measure::new(tip(need(Touch::Top)?)),
    };
    measure.bottom = get(Touch::Bed).map(tip);
    measure.left = get(Touch::Left).map(|p| p[0] + side);
    measure.right = get(Touch::Right).map(|p| p[0] - side);
    measure.front = get(Touch::Front).map(|p| p[1] + side);
    measure.back = get(Touch::Back).map(|p| p[1] - side);
    match shape {
        StockShape::Surface | StockShape::TiltedSurface => {}
        StockShape::Cube => {
            let (left, right, front) = (
                measure.left.ok_or(SolveError::Missing(Touch::Left))?,
                measure.right.ok_or(SolveError::Missing(Touch::Right))?,
                measure.front.ok_or(SolveError::Missing(Touch::Front))?,
            );
            let size = right - left;
            if size <= 0.0 {
                return Err(SolveError::Inconsistent);
            }
            measure.back = Some(front + size);
            measure.bottom = Some(measure.top - size);
        }
        StockShape::Block => {
            need(Touch::Left)?;
            need(Touch::Front)?;
        }
        StockShape::CylinderUpright => {
            let rim: Vec<Point> = (0..3)
                .map(|n| need(Touch::Rim(n)).map(|p| Point::new(p[0], p[1])))
                .collect::<Result<_, _>>()?;
            let (center, reach) =
                circle_through(rim[0], rim[1], rim[2]).ok_or(SolveError::Inconsistent)?;
            let radius = reach - side;
            if radius <= 0.0 {
                return Err(SolveError::Inconsistent);
            }
            measure.center = Some(center);
            measure.diameter = Some(radius * 2.0);
            measure.left = Some(center.x - radius);
            measure.right = Some(center.x + radius);
            measure.front = Some(center.y - radius);
            measure.back = Some(center.y + radius);
        }
        StockShape::CylinderLying => {
            let (front, back) = (
                measure.front.ok_or(SolveError::Missing(Touch::Front))?,
                measure.back.ok_or(SolveError::Missing(Touch::Back))?,
            );
            need(Touch::Left)?;
            let diameter = back - front;
            if diameter <= 0.0 {
                return Err(SolveError::Inconsistent);
            }
            measure.diameter = Some(diameter);
            measure.bottom = Some(measure.top - diameter);
        }
        StockShape::Trapezoid => {
            need(Touch::Left)?;
            let (low, high, back_low) = (
                need(Touch::FrontLow)?,
                need(Touch::FrontHigh)?,
                need(Touch::BackLow)?,
            );
            // The flat tool end touches a side narrowing upwards with its
            // lower edge, so each touch gives the face at the tip height.
            let (z_low, z_high) = (tip(low), tip(high));
            if z_high - z_low < 0.5 {
                return Err(SolveError::Inconsistent);
            }
            let (y_low, y_high) = (low[1] + side, high[1] + side);
            let slope = (y_high - y_low) / (z_high - z_low);
            let front_at = |z: f64| y_low + slope * (z - z_low);
            let back_y = back_low[1] - side;
            let back_at = |z: f64| back_y - slope * (z - tip(back_low));
            let top_width = back_at(measure.top) - front_at(measure.top);
            if top_width <= 0.0 || slope < -1e-6 {
                return Err(SolveError::Inconsistent);
            }
            let base = measure.bottom.unwrap_or(z_low.min(tip(back_low)));
            measure.front = Some(front_at(base));
            measure.back = Some(back_at(base));
            measure.top_width = Some(top_width);
            measure.side_angle = Some(slope.atan().to_degrees());
        }
    }
    if let (Some(first), Some(second)) = (get(Touch::Front), get(Touch::FrontSecond)) {
        let run = second[0] - first[0];
        if run.abs() > 1.0 {
            measure.skew = Some((second[1] - first[1]).atan2(run).to_degrees());
        }
    }
    Ok(measure)
}

/// Circle through three points: centre and radius.
fn circle_through(a: Point, b: Point, c: Point) -> Option<(Point, f64)> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() < 1e-9 {
        return None;
    }
    let (a2, b2, c2) = (
        a.x * a.x + a.y * a.y,
        b.x * b.x + b.y * b.y,
        c.x * c.x + c.y * c.y,
    );
    let center = Point::new(
        (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d,
        (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d,
    );
    Some((center, (a - center).length()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTACT: Contact = Contact {
        diameter: 6.0,
        shim: 0.1,
    };

    #[test]
    fn block_faces_compensate_tool_and_shim() {
        // Block from (10, 20, -50) to (110, 70, -30) in machine space.
        let touches = [
            (Touch::Top, [50.0, 40.0, -30.0 + 0.1]),
            (Touch::Left, [10.0 - 3.1, 40.0, -35.0]),
            (Touch::Front, [50.0, 20.0 - 3.1, -35.0]),
            (Touch::Right, [110.0 + 3.1, 40.0, -35.0]),
            (Touch::Back, [50.0, 70.0 + 3.1, -35.0]),
            (Touch::Bed, [0.0, 0.0, -50.0 + 0.1]),
        ];
        let m = solve(StockShape::Block, &touches, CONTACT).unwrap();
        assert!((m.width().unwrap() - 100.0).abs() < 1e-9);
        assert!((m.depth().unwrap() - 50.0).abs() < 1e-9);
        assert!((m.height().unwrap() - 20.0).abs() < 1e-9);
        let [x, y, z] = m.origin(Origin::FrontLeft).unwrap();
        assert!((x - 10.0).abs() < 1e-9 && (y - 20.0).abs() < 1e-9 && (z + 30.0).abs() < 1e-9);
        let [cx, cy, _] = m.origin(Origin::Center).unwrap();
        assert!((cx - 60.0).abs() < 1e-9 && (cy - 45.0).abs() < 1e-9);
        assert_eq!(
            m.origin_program(Origin::FrontLeft).unwrap(),
            ["G10 L2 P1 X10.000 Y20.000 Z-30.000"]
        );
    }

    #[test]
    fn block_origin_needs_only_three_touches() {
        let touches = [
            (Touch::Top, [0.0, 0.0, -10.0]),
            (Touch::Left, [0.0, 0.0, -12.0]),
            (Touch::Front, [0.0, 0.0, -12.0]),
        ];
        let m = solve(StockShape::Block, &touches, CONTACT).unwrap();
        assert!(m.origin(Origin::FrontLeft).is_some());
        assert!(m.origin(Origin::Center).is_none());
        assert_eq!(
            solve(StockShape::Block, &touches[..2], CONTACT),
            Err(SolveError::Missing(Touch::Front))
        );
    }

    #[test]
    fn cube_is_deduced_from_four_touches_and_skew_measured() {
        let touches = [
            (Touch::Top, [0.0, 0.0, 0.1]),
            (Touch::Left, [-3.1, 10.0, -5.0]),
            (Touch::Front, [10.0, -3.1, -5.0]),
            (Touch::Right, [40.0 + 3.1, 10.0, -5.0]),
            (Touch::FrontSecond, [30.0, -3.1 + 0.2, -5.0]),
        ];
        let m = solve(StockShape::Cube, &touches, CONTACT).unwrap();
        assert!((m.depth().unwrap() - 40.0).abs() < 1e-9);
        assert!((m.height().unwrap() - 40.0).abs() < 1e-9);
        assert!((m.skew.unwrap() - 0.2f64.atan2(20.0).to_degrees()).abs() < 1e-9);
    }

    #[test]
    fn upright_cylinder_centre_and_diameter() {
        let (cx, cy, r) = (100.0, 80.0, 25.0);
        let reach = r + 3.1;
        let rim = |deg: f64| {
            let a = deg.to_radians();
            [cx + reach * a.cos(), cy + reach * a.sin(), -20.0]
        };
        let touches = [
            (Touch::Top, [cx, cy, 0.1]),
            (Touch::Rim(0), rim(200.0)),
            (Touch::Rim(1), rim(270.0)),
            (Touch::Rim(2), rim(330.0)),
        ];
        let m = solve(StockShape::CylinderUpright, &touches, CONTACT).unwrap();
        assert!((m.diameter.unwrap() - 50.0).abs() < 1e-9);
        let [x, y, z] = m.origin(Origin::FrontLeft).unwrap();
        assert!((x - cx).abs() < 1e-9 && (y - cy).abs() < 1e-9 && z.abs() < 1e-9);
    }

    #[test]
    fn lying_cylinder_axis_from_its_sides() {
        let touches = [
            (Touch::Front, [20.0, 30.0 - 3.1, -40.0]),
            (Touch::Back, [20.0, 70.0 + 3.1, -40.0]),
            (Touch::Top, [20.0, 50.0, -10.0 + 0.1]),
            (Touch::Left, [5.0 - 3.1, 50.0, -20.0]),
        ];
        let m = solve(StockShape::CylinderLying, &touches, CONTACT).unwrap();
        assert!((m.diameter.unwrap() - 40.0).abs() < 1e-9);
        assert!((m.bottom.unwrap() + 50.0).abs() < 1e-9);
        let [x, y, _] = m.origin(Origin::FrontLeft).unwrap();
        assert!((x - 5.0).abs() < 1e-9 && (y - 50.0).abs() < 1e-9);
    }

    #[test]
    fn trapezoid_top_width_and_side_angle() {
        // Base 60 wide from y=0 to y=60 at z=0, top 40 wide at z=20:
        // each side moves in by 10 over 20 of height.
        let face = |z: f64| 10.0 * z / 20.0;
        let touches = [
            (Touch::Top, [30.0, 30.0, 20.1]),
            (Touch::Left, [-3.1, 30.0, 10.0]),
            (Touch::FrontLow, [10.0, face(4.0) - 3.1, 4.1]),
            (Touch::FrontHigh, [10.0, face(16.0) - 3.1, 16.1]),
            (Touch::BackLow, [10.0, 60.0 - face(4.0) + 3.1, 4.1]),
            (Touch::Bed, [-20.0, 0.0, 0.1]),
        ];
        let m = solve(StockShape::Trapezoid, &touches, CONTACT).unwrap();
        assert!((m.top_width.unwrap() - 40.0).abs() < 1e-9);
        assert!((m.side_angle.unwrap() - 0.5f64.atan().to_degrees()).abs() < 1e-9);
        assert!(m.front.unwrap().abs() < 1e-9 && (m.back.unwrap() - 60.0).abs() < 1e-9);
    }

    #[test]
    fn tilted_surface_from_three_points() {
        let touches = [
            (Touch::TopAt(0), [0.0, 0.0, 0.1]),
            (Touch::TopAt(1), [100.0, 0.0, 1.1]),
            (Touch::TopAt(2), [0.0, 100.0, 0.1]),
        ];
        let m = solve(StockShape::TiltedSurface, &touches, CONTACT).unwrap();
        let (plane, residual) = m.plane.unwrap();
        assert!((plane.a - 0.01).abs() < 1e-9 && plane.b.abs() < 1e-9);
        assert!(residual < 1e-9);
        // Without sides only Z is set, on the highest point.
        assert_eq!(
            m.origin_program(Origin::Center).unwrap(),
            ["G10 L2 P1 Z1.000"]
        );
    }

    #[test]
    fn every_shape_has_required_touches() {
        for shape in StockShape::ALL {
            assert!(shape.steps().iter().any(|step| !step.optional));
        }
    }
}
