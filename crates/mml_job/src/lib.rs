//! Turns a document into the program streamed to the machine.

mod relief;
mod vcarve;

use mml_cnc::CncSettings;
use mml_core::document::{Document, Shape, ShapeId, ShapeKind};
use mml_core::geometry::{Path, Point, Rect, Transform};
use mml_core::layer::Operation;
use mml_core::machine::{MachineKind, MachineProfile};
use mml_gcode::GcodeWriter;
use mml_laser::LaserSettings;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobWarning {
    NothingToOutput,
    OutOfBounds,
    CncOnLaser(String),
    LaserOnCnc(String),
    ImageOnVectorLayer(String),
    VectorOnImageLayer(String),
    OpenPathsNotFilled(String),
}

/// Why the job stops and waits for the operator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PauseReason {
    /// The next layer needs the other head (laser after CNC or the
    /// opposite).
    HeadChange { next_layer: String, to_laser: bool },
    /// The user asked for a pause before this layer.
    LayerPause { next_layer: String },
    /// Next file of a G-code sequence.
    NextFile { name: String },
}

/// Engraving around a cylinder. The design is drawn flat: its height
/// is the unrolled circumference, `π × diameter`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Rotary {
    /// Chuck on a real A axis configured in degrees (grblHAL, 4-axis
    /// boards): Y lengths become A angles.
    AxisA { diameter: f64 },
    /// Rotary plugged in place of the Y motor (rollers or chuck): Y
    /// lengths are scaled so that one turn of the object equals
    /// `y_per_rotation` controller millimetres (found by calibration).
    ReplaceY { diameter: f64, y_per_rotation: f64 },
}

impl Rotary {
    pub fn circumference(&self) -> f64 {
        let diameter = match self {
            Self::AxisA { diameter } | Self::ReplaceY { diameter, .. } => *diameter,
        };
        std::f64::consts::PI * diameter.max(0.1)
    }
}

/// Rewrites the Y words of a program for a rotary attachment.
pub fn wrap_rotary(gcode: &str, rotary: Rotary) -> String {
    let circumference = rotary.circumference();
    let (letter, factor) = match rotary {
        Rotary::AxisA { .. } => ('A', 360.0 / circumference),
        Rotary::ReplaceY { y_per_rotation, .. } => ('Y', y_per_rotation / circumference),
    };
    let mut out = String::with_capacity(gcode.len() + gcode.len() / 8);
    if let Rotary::AxisA { diameter } = rotary {
        // Lets previews wrap the program around the axis.
        out.push_str(&format!("; Rotary stock diameter {diameter:.3} mm\n"));
    }
    for line in gcode.lines() {
        let (code, comment) = match line.find(';') {
            Some(index) => line.split_at(index),
            None => (line, ""),
        };
        let mut chars = code.chars().peekable();
        while let Some(character) = chars.next() {
            if character == 'Y' || character == 'y' {
                let mut number = String::new();
                while let Some(&next) = chars.peek() {
                    if next.is_ascii_digit() || next == '.' || next == '-' || next == '+' {
                        number.push(next);
                        chars.next();
                    } else {
                        break;
                    }
                }
                match number.parse::<f64>() {
                    Ok(value) => out.push_str(&format!("{letter}{:.3}", value * factor)),
                    Err(_) => {
                        out.push(character);
                        out.push_str(&number);
                    }
                }
            } else {
                out.push(character);
            }
        }
        out.push_str(comment);
        out.push('\n');
    }
    out
}

#[derive(Debug, Clone, Default)]
pub struct JobOptions {
    /// Only these shapes are output (partial engraving of large pieces).
    pub only: Option<Vec<ShapeId>>,
    /// Design point that the head currently marks; the job then runs
    /// relative to the head position. Overrides the machine setting.
    pub anchor: Option<Point>,
    /// With an anchor, the current head height is Z0 too: the whole job
    /// runs relative to the head position in X, Y and Z.
    pub anchor_z: bool,
    pub rotary: Option<Rotary>,
}

#[derive(Debug, Clone)]
pub struct Job {
    pub gcode: String,
    pub bounds: Rect,
    pub warnings: Vec<JobWarning>,
    /// Pauses in program order; each is an `M0` line in the program.
    pub pauses: Vec<PauseReason>,
    /// Design point placed at the head position, when relative.
    pub anchor: Option<Point>,
    /// The head height is Z0 as well (see [`JobOptions::anchor_z`]).
    pub anchor_z: bool,
}

fn vector_paths(shape: &Shape) -> Option<Vec<Path>> {
    (!shape.is_image()).then(|| shape.world_paths())
}

pub fn build_job(document: &Document, machine: &MachineProfile) -> Job {
    build_job_with(document, machine, &JobOptions::default())
}

/// Stops the tool, lifts it and waits for the operator (`M0`). GRBL
/// resumes on cycle start, which the machine button sends; with the
/// safety door enabled ($ door input) it refuses to resume while open.
fn pause(writer: &mut GcodeWriter, machine: &MachineProfile, mills: bool, message: &str) {
    writer.tool_off();
    writer.air(false);
    if machine.kind == MachineKind::Cnc && mills {
        writer.rapid_z(machine.safe_z);
    }
    writer.comment(message);
    writer.raw("M0");
}

pub fn build_job_with(document: &Document, machine: &MachineProfile, options: &JobOptions) -> Job {
    let mut warnings = Vec::new();
    let output_shapes: Vec<&Shape> = document
        .shapes
        .iter()
        .filter(|shape| {
            document
                .layer(shape.layer)
                .is_some_and(|layer| layer.output && layer.visible && layer.obstacle.is_none())
        })
        .filter(|shape| {
            options
                .only
                .as_ref()
                .is_none_or(|only| only.contains(&shape.id))
        })
        .collect();
    let bounds = output_shapes
        .iter()
        .fold(Rect::EMPTY, |rect, shape| rect.union(shape.bounds()));
    let anchor = options
        .anchor
        .or(machine.relative_to_current.then_some(bounds.min));
    let anchor_z = anchor.is_some() && options.anchor_z;
    if output_shapes.is_empty() {
        warnings.push(JobWarning::NothingToOutput);
        return Job {
            gcode: String::new(),
            bounds,
            warnings,
            pauses: Vec::new(),
            anchor,
            anchor_z,
        };
    }
    let height = options.rotary.map_or(machine.height, |rotary| {
        rotary.circumference().max(bounds.max.y)
    });
    let machine_area = Rect::from_points(Point::default(), Point::new(machine.width, height));
    if anchor.is_none() && !(machine_area.contains(bounds.min) && machine_area.contains(bounds.max))
    {
        warnings.push(JobWarning::OutOfBounds);
    }

    let laser = LaserSettings {
        max_s: machine.max_s,
        dynamic_power: machine.laser_mode,
        scan_offset: 0.0,
    };
    let cnc = CncSettings {
        safe_z: machine.safe_z,
    };
    let mut writer = GcodeWriter::new();
    let mut pauses = Vec::new();
    writer.comment(&format!(
        "MyMaxmakeLab {} - {}",
        env!("CARGO_PKG_VERSION"),
        machine.name
    ));
    writer.comment(&format!(
        "Bounds X{:.2} Y{:.2} to X{:.2} Y{:.2}",
        bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y
    ));
    // The start G-code comes first, as MaxmakeLAB does on the HiMill
    // (its `[STREAM_START]` opens the program).
    writer.raw(&machine.start_gcode);
    writer.preamble();
    // A job placed at the head is written relative to it (G91, see
    // `mml_gcode::relative`), like the "current position" jobs of
    // MaxmakeLAB: no work offset is touched. Rotary jobs still declare
    // the head position with G92.
    let relative = anchor.is_some() && options.rotary.is_none();
    if let Some(anchor) = anchor.filter(|_| !relative) {
        let z = if anchor_z { " Z0" } else { "" };
        writer.raw(&format!("G92 X{:.3} Y{:.3}{z}", anchor.x, anchor.y));
    }
    let mut head = anchor.unwrap_or_default();

    // A job of laser layers only never moves Z, even on a milling
    // machine with a laser module: the head stays at its focus height.
    let mills = document.layers.iter().any(|layer| {
        !layer.operation.is_laser()
            && output_shapes.iter().any(|shape| shape.layer == layer.id)
            && layer.output
            && layer.visible
            && layer.obstacle.is_none()
    });
    let lifts = machine.kind == MachineKind::Cnc && mills;
    let mut previous_family: Option<bool> = None;
    let mut loaded_tool: Option<u32> = None;
    for layer in document
        .layers
        .iter()
        .filter(|layer| layer.output && layer.visible && layer.obstacle.is_none())
    {
        let shapes: Vec<&Shape> = output_shapes
            .iter()
            .copied()
            .filter(|shape| shape.layer == layer.id)
            .collect();
        if shapes.is_empty() {
            continue;
        }
        let name = layer.name.clone();
        let is_laser = layer.operation.is_laser();
        // Laser module beside the spindle: burn where the design is.
        let [dx, dy] = machine.laser_offset;
        let moved: Vec<Shape>;
        let shapes: Vec<&Shape> =
            if is_laser && machine.kind == MachineKind::Cnc && (dx != 0.0 || dy != 0.0) {
                moved = shapes
                    .iter()
                    .map(|shape| {
                        let mut copy = (*shape).clone();
                        copy.transform = copy.transform.then(Transform::translate(-dx, -dy));
                        copy
                    })
                    .collect();
                moved.iter().collect()
            } else {
                shapes
            };
        if previous_family.is_some_and(|previous| previous != is_laser) {
            let reason = PauseReason::HeadChange {
                next_layer: name.clone(),
                to_laser: is_laser,
            };
            pause(
                &mut writer,
                machine,
                mills,
                &format!("Head change before layer {name}"),
            );
            pauses.push(reason);
        } else if layer.pause_before && previous_family.is_some() {
            pause(
                &mut writer,
                machine,
                mills,
                &format!("Pause before layer {name}"),
            );
            pauses.push(PauseReason::LayerPause {
                next_layer: name.clone(),
            });
        }
        previous_family = Some(is_laser);
        if machine.tool_changer
            && !is_laser
            && let Some(tool) = layer.tool
            && loaded_tool != Some(tool)
        {
            // The firmware fetches and measures the tool, then returns.
            writer.tool_off();
            writer.rapid_z(machine.safe_z);
            writer.comment(&format!("Tool {tool} for layer {name}"));
            writer.raw(&format!("M6 T{tool}"));
            loaded_tool = Some(tool);
        }
        match (&layer.operation, machine.kind) {
            (operation, MachineKind::Laser) if !operation.is_laser() => {
                warnings.push(JobWarning::CncOnLaser(name.clone()))
            }
            (operation, MachineKind::Cnc) if operation.is_laser() && !machine.laser_mode => {
                warnings.push(JobWarning::LaserOnCnc(name.clone()))
            }
            _ => {}
        }
        writer.comment(&format!("Layer {name}"));
        // Lets job records add the time of each tool.
        writer.comment(&format!("Tool {}", layer.operation.tool_label()));
        let has_images = shapes.iter().any(|shape| shape.is_image());
        let vectors: Vec<Vec<Path>> = shapes
            .iter()
            .filter_map(|shape| vector_paths(shape))
            .collect();
        let takes_images = matches!(
            layer.operation,
            Operation::LaserImage(_) | Operation::CncVCarve(_) | Operation::CncRelief(_)
        );
        if has_images && !takes_images {
            warnings.push(JobWarning::ImageOnVectorLayer(name.clone()));
        }
        match &layer.operation {
            Operation::LaserLine(params) => {
                let vectors = if params.kerf > 0.0 {
                    vectors
                        .into_iter()
                        .map(|paths| compensate_kerf(paths, params.kerf))
                        .collect()
                } else {
                    vectors
                };
                mml_laser::emit_line_shapes(&mut writer, &mut head, vectors, params, &laser)
            }
            Operation::LaserFill(params) => {
                if vectors.iter().flatten().any(|path| !path.closed) {
                    warnings.push(JobWarning::OpenPathsNotFilled(name.clone()));
                }
                let laser = LaserSettings {
                    scan_offset: machine.scan_offset(params.speed),
                    ..laser
                };
                mml_laser::emit_fill(&mut writer, &mut head, vectors, params, &laser)
            }
            Operation::LaserImage(params) => {
                if !vectors.is_empty() {
                    warnings.push(JobWarning::VectorOnImageLayer(name.clone()));
                }
                for shape in shapes
                    .iter()
                    .filter(|shape| matches!(shape.kind, ShapeKind::Image { .. }))
                {
                    let laser = LaserSettings {
                        scan_offset: machine.scan_offset(params.speed),
                        ..laser
                    };
                    mml_laser::emit_image(&mut writer, &mut head, shape, params, &laser);
                }
            }
            Operation::CncProfile(params) => {
                mml_cnc::emit_profile(&mut writer, &mut head, vectors, params, &cnc)
            }
            Operation::CncPocket(params) => {
                mml_cnc::emit_pocket(&mut writer, &mut head, vectors, params, &cnc)
            }
            Operation::CncVCarve(params) => {
                vcarve::emit_vcarve(&mut writer, &shapes, params, machine.safe_z)
            }
            Operation::CncRelief(params) => {
                if vectors.iter().flatten().any(|path| !path.closed) {
                    warnings.push(JobWarning::OpenPathsNotFilled(name.clone()));
                }
                relief::emit_relief(&mut writer, &shapes, params, machine.safe_z)
            }
        }
    }

    // Spindle and coolant are already off: the head goes back, then the
    // end G-code of the machine runs last (it may present the part, or
    // close a stream like the HiMill), and the program ends.
    if let Some(anchor) = anchor {
        if lifts {
            writer.rapid_z(machine.safe_z);
        }
        writer.rapid(anchor.x, anchor.y);
        if !relative {
            writer.raw("G92.1");
        }
    } else if lifts {
        writer.rapid_z(machine.safe_z);
        writer.rapid(0.0, 0.0);
    }
    writer.raw(&machine.end_gcode);
    writer.program_end();
    let gcode = match (options.rotary, anchor) {
        (Some(rotary), _) => wrap_rotary(&writer.finish(), rotary),
        (None, Some(anchor)) => {
            mml_gcode::relative::to_relative(&writer.finish(), [anchor.x, anchor.y], anchor_z)
        }
        (None, None) => writer.finish(),
    };
    Job {
        gcode,
        bounds,
        warnings,
        pauses,
        anchor,
        anchor_z,
    }
}

/// Joins G-code files into one program with an operator pause between
/// them (quick tool or head change between prepared files).
pub fn sequence(files: &[(String, String)]) -> (String, Vec<PauseReason>) {
    let mut program = String::new();
    let mut pauses = Vec::new();
    for (index, (name, gcode)) in files.iter().enumerate() {
        if index > 0 {
            program.push_str(&format!("M5\nM9\n; Next file {name}\nM0\n"));
            pauses.push(PauseReason::NextFile { name: name.clone() });
        }
        for line in gcode.lines() {
            let trimmed = line.trim();
            // Program end words would stop the sequence early.
            let code = trimmed
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_uppercase();
            if code == "M2" || code == "M30" || code == "%" {
                continue;
            }
            program.push_str(trimmed);
            program.push('\n');
        }
    }
    program.push_str("M5\nM2\n");
    (program, pauses)
}

/// Traces the job bounding box so the user can check the placement on
/// the material, with jog commands (`$J=`) like MaxmakeLAB: the
/// controller takes them whenever it is idle and they change no modal
/// state. `power` is a percentage: above 0 the laser is on at that
/// constant power (`M3`) during the trace, so it shows on the material;
/// at 0 the head moves with the laser off.
///
/// With an `anchor` the trace is relative to the head (which marks that
/// design point) and ends back where it started; without, it follows
/// the work coordinates.
pub fn frame_job(
    bounds: Rect,
    anchor: Option<Point>,
    machine: &MachineProfile,
    power: f64,
    speed: f64,
) -> String {
    let mut lines = Vec::new();
    let s_value = (power.clamp(0.0, 100.0) / 100.0 * machine.max_s).round();
    let corners = bounds.corners();
    let feed = speed.round();
    let number = |value: f64| {
        let text = format!("{:.3}", if value.abs() < 0.0005 { 0.0 } else { value });
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    };
    // Path: to the first corner (laser off), around, back to the first
    // corner, then (relative) back to the start.
    let mut path: Vec<Point> = corners.to_vec();
    path.push(corners[0]);
    match anchor {
        Some(anchor) => {
            let mut at = anchor;
            let mut jog = |to: Point, lines: &mut Vec<String>| {
                lines.push(format!(
                    "$J=G21G91X{}Y{}F{feed}",
                    number(to.x - at.x),
                    number(to.y - at.y)
                ));
                at = to;
            };
            jog(path[0], &mut lines);
            if s_value > 0.0 {
                lines.push(format!("M3 S{s_value}"));
            }
            for corner in &path[1..] {
                jog(*corner, &mut lines);
            }
            if s_value > 0.0 {
                lines.push("M5".into());
            }
            jog(anchor, &mut lines);
        }
        None => {
            let jog = |to: Point| format!("$J=G21G90X{}Y{}F{feed}", number(to.x), number(to.y));
            lines.push(jog(path[0]));
            if s_value > 0.0 {
                lines.push(format!("M3 S{s_value}"));
            }
            for corner in &path[1..] {
                lines.push(jog(*corner));
            }
            if s_value > 0.0 {
                lines.push("M5".into());
            }
        }
    }
    let mut program = lines.join("\n");
    program.push('\n');
    program
}

/// Closed outlines moved out by half the kerf, holes in, so the part
/// keeps its drawn size once the beam has burnt its width; open lines
/// stay where they are.
fn compensate_kerf(paths: Vec<Path>, kerf: f64) -> Vec<Path> {
    let (closed, open): (Vec<Path>, Vec<Path>) = paths.into_iter().partition(|path| path.closed);
    let mut result = mml_core::region::offset_paths(&closed, kerf / 2.0);
    if result.is_empty() {
        result = closed;
    }
    result.extend(open);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use mml_core::geometry::Transform;
    use mml_core::layer::{CncProfileParams, FillParams, Layer, LayerId};

    fn document_with_square(x: f64) -> (Document, LayerId) {
        let mut document = Document::default();
        let layer = document.layers[0].id;
        let mut shape = Shape::new(
            "square",
            layer,
            ShapeKind::Rectangle {
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );
        shape.transform = Transform::translate(x, 50.0);
        document.shapes.push(shape);
        (document, layer)
    }

    #[test]
    fn job_runs_layers_in_order_and_skips_hidden_ones() {
        let (mut document, _) = document_with_square(50.0);
        let fill = Layer::new(
            "Fill",
            [0, 0, 0],
            Operation::LaserFill(FillParams::default()),
        );
        let fill_id = fill.id;
        document.layers.insert(0, fill);
        let mut copy = document.shapes[0].clone();
        copy.id = ShapeId::new();
        copy.layer = fill_id;
        document.shapes.push(copy);
        let job = build_job(&document, &MachineProfile::generic_laser());
        let fill_at = job.gcode.find("; Layer Fill").unwrap();
        let cut_at = job.gcode.find("; Layer Cut").unwrap();
        assert!(fill_at < cut_at);
        assert!(job.warnings.is_empty());

        document.layers[0].output = false;
        let job = build_job(&document, &MachineProfile::generic_laser());
        assert!(!job.gcode.contains("; Layer Fill"));
    }

    #[test]
    fn laser_layers_move_by_the_laser_offset_on_a_hybrid_machine() {
        let (document, _) = document_with_square(20.0);
        let laser = |profile: &MachineProfile| {
            let job = build_job(&document, profile);
            mml_gcode::parse(&job.gcode).cut_bounds()
        };
        let mut hybrid = MachineProfile::generic_hybrid();
        let plain = laser(&hybrid);
        hybrid.laser_offset = [30.0, -12.5];
        let moved = laser(&hybrid);
        assert!((moved.min.x - (plain.min.x - 30.0)).abs() < 1e-6);
        assert!((moved.min.y - (plain.min.y + 12.5)).abs() < 1e-6);
        // A laser-only machine has no spindle to be offset from.
        let mut laser_only = MachineProfile::generic_laser();
        laser_only.laser_offset = [30.0, 0.0];
        let same = laser(&laser_only);
        let reference = laser(&MachineProfile::generic_laser());
        assert!((same.min.x - reference.min.x).abs() < 1e-6);
    }

    #[test]
    fn himill_programs_are_framed_and_still_read_correctly() {
        let (document, _) = document_with_square(20.0);
        let himill = MachineProfile::maxmake_himill_d1s();
        let job = build_job(&document, &himill);
        let lines: Vec<&str> = job
            .gcode
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();
        assert!(lines.contains(&"[STREAM_START]"), "{lines:?}");
        // End G-code goes just before the closing M2, like on any machine.
        assert_eq!(lines[lines.len() - 2..], ["[STREAM_END]", "M2"]);
        // The markers move nothing: same cut as without them.
        let plain = MachineProfile {
            start_gcode: String::new(),
            end_gcode: String::new(),
            ..himill.clone()
        };
        let framed = mml_gcode::parse(&job.gcode).cut_bounds();
        let bare = mml_gcode::parse(&build_job(&document, &plain).gcode).cut_bounds();
        assert_eq!(framed, bare);
    }

    #[test]
    fn kerf_compensation_grows_parts_and_shrinks_holes() {
        let (mut document, _) = document_with_square(20.0);
        let width = |document: &Document| {
            mml_gcode::parse(&build_job(document, &MachineProfile::generic_laser()).gcode)
                .cut_bounds()
                .width()
        };
        let plain = width(&document);
        if let Operation::LaserLine(params) = &mut document.layers[0].operation {
            params.kerf = 0.2;
        }
        // Cut 0.1 mm outside on every side.
        assert!((width(&document) - (plain + 0.2)).abs() < 1e-3);
        let hole = compensate_kerf(
            vec![
                Path::new(
                    vec![
                        Point::new(0.0, 0.0),
                        Point::new(10.0, 0.0),
                        Point::new(10.0, 10.0),
                        Point::new(0.0, 10.0),
                    ],
                    true,
                ),
                Path::new(
                    vec![
                        Point::new(4.0, 4.0),
                        Point::new(6.0, 4.0),
                        Point::new(6.0, 6.0),
                        Point::new(4.0, 6.0),
                    ],
                    true,
                ),
            ],
            0.2,
        );
        let inner = hole
            .iter()
            .map(|path| path.bounds())
            .min_by(|a, b| a.width().total_cmp(&b.width()))
            .unwrap();
        assert!((inner.width() - 1.8).abs() < 1e-6);
    }

    #[test]
    fn out_of_bounds_design_is_reported() {
        let (document, _) = document_with_square(1000.0);
        let job = build_job(&document, &MachineProfile::generic_laser());
        assert!(job.warnings.contains(&JobWarning::OutOfBounds));
    }

    #[test]
    fn anchored_job_runs_relative_to_the_head() {
        let (document, _) = document_with_square(1000.0);
        let options = JobOptions {
            anchor: Some(Point::new(1000.0, 50.0)),
            ..JobOptions::default()
        };
        let job = build_job_with(&document, &MachineProfile::generic_laser(), &options);
        assert!(!job.gcode.contains("G92"), "{}", job.gcode);
        assert!(!job.gcode.contains("G90"), "{}", job.gcode);
        assert!(job.gcode.contains("G91"));
        assert!(!job.warnings.contains(&JobWarning::OutOfBounds));
        // Parsed back, it is in design coordinates and ends at the head.
        let program = mml_gcode::parse(&job.gcode);
        assert_eq!(program.relative_start, Some([1000.0, 50.0, 0.0]));
        let absolute = build_job_with(
            &document,
            &MachineProfile::generic_laser(),
            &JobOptions::default(),
        );
        let expected = mml_gcode::parse(&absolute.gcode).cut_bounds();
        let bounds = program.cut_bounds();
        for (got, want) in [
            (bounds.min.x, expected.min.x),
            (bounds.min.y, expected.min.y),
            (bounds.max.x, expected.max.x),
            (bounds.max.y, expected.max.y),
        ] {
            assert!((got - want).abs() < 1e-3, "{got} vs {want}");
        }
        let last = program.motions.last().unwrap();
        assert!((last.to[0] - 1000.0).abs() < 1e-6 && (last.to[1] - 50.0).abs() < 1e-6);
    }

    #[test]
    fn a_himill_job_opens_its_stream_first() {
        let (document, _) = document_with_square(50.0);
        let mut machine = MachineProfile::maxmake_himill_d1s();
        machine.laser_mode = true;
        let options = JobOptions {
            anchor: Some(Point::new(45.0, 45.0)),
            ..JobOptions::default()
        };
        let job = build_job_with(&document, &machine, &options);
        let code: Vec<&str> = job
            .gcode
            .lines()
            .filter(|line| !line.starts_with(';') && !line.is_empty())
            .collect();
        assert_eq!(code[0], "[STREAM_START]");
        assert!(code[1].starts_with("G91"), "{}", code[1]);
        assert_eq!(code[code.len() - 2], "[STREAM_END]");
    }

    #[test]
    fn framing_jogs_around_the_job_and_back_to_the_head() {
        let bounds = Rect::from_points(Point::new(10.0, 10.0), Point::new(60.0, 40.0));
        let machine = MachineProfile::maxmake_himill_d1s();
        let frame = frame_job(bounds, Some(Point::new(10.0, 10.0)), &machine, 1.0, 3000.0);
        let lines: Vec<&str> = frame.lines().collect();
        assert!(
            lines
                .iter()
                .all(|line| line.starts_with("$J=") || line.starts_with('M'))
        );
        assert_eq!(lines[1], "M3 S130");
        assert_eq!(lines.last(), Some(&"$J=G21G91X0Y0F3000"));
        assert!(lines.contains(&"M5"));
        let off = frame_job(bounds, None, &machine, 0.0, 3000.0);
        assert!(!off.contains('M'));
        assert!(off.lines().all(|line| line.starts_with("$J=G21G90")));
    }

    #[test]
    fn partial_job_outputs_only_the_chosen_shapes() {
        let (mut document, layer) = document_with_square(50.0);
        let mut far = Shape::new("far", layer, ShapeKind::Ellipse { rx: 5.0, ry: 5.0 });
        far.transform = Transform::translate(200.0, 200.0);
        let keep = document.shapes[0].id;
        document.shapes.push(far);
        let options = JobOptions {
            only: Some(vec![keep]),
            ..JobOptions::default()
        };
        let job = build_job_with(&document, &MachineProfile::generic_laser(), &options);
        assert!(job.bounds.max.x < 100.0);
    }

    #[test]
    fn switching_between_cnc_and_laser_pauses_for_the_head_change() {
        let (mut document, laser_layer) = document_with_square(50.0);
        let cnc = Layer::new(
            "Mill",
            [0, 0, 0],
            Operation::CncProfile(CncProfileParams::default()),
        );
        let cnc_id = cnc.id;
        document.layers.insert(0, cnc);
        let mut milled = document.shapes[0].clone();
        milled.id = ShapeId::new();
        milled.layer = cnc_id;
        document.shapes.push(milled);
        let mut machine = MachineProfile::generic_cnc();
        machine.laser_mode = true;
        let job = build_job(&document, &machine);
        assert_eq!(
            job.pauses,
            vec![PauseReason::HeadChange {
                next_layer: "Cut".into(),
                to_laser: true
            }]
        );
        let pause_at = job.gcode.find("M0").unwrap();
        assert!(job.gcode.find("; Layer Mill").unwrap() < pause_at);
        assert!(pause_at < job.gcode.find("; Layer Cut").unwrap());
        let _ = laser_layer;
    }

    #[test]
    fn a_laser_job_on_a_milling_machine_never_moves_z() {
        let (document, _) = document_with_square(50.0);
        let mut machine = MachineProfile::maxmake_himill_d1s();
        machine.laser_mode = true;
        let job = build_job(&document, &machine);
        let program = mml_gcode::parse(&job.gcode);
        assert!(!program.moves_z, "{}", job.gcode);
        assert!(program.motions.iter().any(|motion| motion.is_cutting()));
    }

    #[test]
    fn fixtures_are_never_cut() {
        let (mut document, _) = document_with_square(50.0);
        document.layers[0].operation = Operation::CncProfile(CncProfileParams::default());
        document.layers[0].obstacle = Some(12.0);
        let job = build_job(&document, &MachineProfile::generic_cnc());
        assert!(!job.gcode.contains("G1"), "{}", job.gcode);
    }

    #[test]
    fn a_tool_changer_loads_each_layer_tool_without_pausing() {
        let (mut document, first) = document_with_square(50.0);
        document.layers[0].operation = Operation::CncProfile(CncProfileParams::default());
        document.layers[0].tool = Some(2);
        let mut second = Layer::new(
            "Finish",
            [0, 0, 0],
            Operation::CncProfile(CncProfileParams::default()),
        );
        second.tool = Some(5);
        let second_id = second.id;
        document.layers.push(second);
        let mut copy = document.shapes[0].clone();
        copy.id = ShapeId::new();
        copy.layer = second_id;
        document.shapes.push(copy);
        let job = build_job(&document, &MachineProfile::makera_carvera());
        assert!(job.pauses.is_empty());
        let t2 = job.gcode.find("M6 T2").unwrap();
        let t5 = job.gcode.find("M6 T5").unwrap();
        assert!(t2 < job.gcode.find("; Layer Finish").unwrap() && t5 > t2);
        assert!(!job.gcode.contains("M0\n"));
        let after = &job.gcode[t5..];
        let spindle = after.find("M3").unwrap();
        assert!(
            spindle < after.find("G1").unwrap(),
            "spindle restarts after the change"
        );
        let manual = build_job(&document, &MachineProfile::generic_cnc());
        assert!(!manual.gcode.contains("M6"));
        let _ = first;
    }

    #[test]
    fn rotary_turns_y_lengths_into_angles_or_scaled_y() {
        let diameter = 360.0 / std::f64::consts::PI;
        let chuck = wrap_rotary(
            "G1X1Y10F600 ; Y in comment\nG0 Y-5",
            Rotary::AxisA { diameter },
        );
        assert_eq!(
            chuck,
            "; Rotary stock diameter 114.592 mm\nG1X1A10.000F600 ; Y in comment\nG0 A-5.000\n"
        );
        assert_eq!(mml_gcode::parse(&chuck).rotary_diameter, Some(114.592));
        let rollers = wrap_rotary(
            "G1 Y180",
            Rotary::ReplaceY {
                diameter,
                y_per_rotation: 100.0,
            },
        );
        assert_eq!(rollers, "G1 Y50.000\n");
    }

    #[test]
    fn v_carved_letter_stays_within_the_depth_limit() {
        let (mut document, _) = document_with_square(50.0);
        document.layers[0].operation = Operation::CncVCarve(mml_core::layer::VCarveParams {
            max_depth: 1.0,
            line_spacing: 0.5,
            ..Default::default()
        });
        let job = build_job(&document, &MachineProfile::generic_cnc());
        let program = mml_gcode::parse(&job.gcode);
        assert!(program.z_range.0 >= -1.0 - 1e-6);
        assert!(
            program.z_range.0 < -0.9,
            "a 10 mm square must reach the depth limit"
        );
        let bounds = program.cut_bounds();
        assert!(bounds.min.x >= 44.0 && bounds.max.x <= 56.0);
    }

    #[test]
    fn file_sequence_drops_inner_program_ends() {
        let (program, pauses) = sequence(&[
            ("a".into(), "G0 X1\nM2\n".into()),
            ("b".into(), "G0 X2\nM30".into()),
        ]);
        assert_eq!(pauses.len(), 1);
        assert_eq!(program.matches("M2").count(), 1);
        assert!(program.find("G0 X1").unwrap() < program.find("M0").unwrap());
    }
}
