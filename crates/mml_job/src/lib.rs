//! Turns a document into the program streamed to the machine.

use mml_cnc::CncSettings;
use mml_core::document::{Document, Shape, ShapeId, ShapeKind};
use mml_core::geometry::{Path, Point, Rect};
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
fn pause(writer: &mut GcodeWriter, machine: &MachineProfile, message: &str) {
    writer.tool_off();
    writer.air(false);
    if machine.kind == MachineKind::Cnc {
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
                .is_some_and(|layer| layer.output && layer.visible)
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
    if output_shapes.is_empty() {
        warnings.push(JobWarning::NothingToOutput);
        return Job {
            gcode: String::new(),
            bounds,
            warnings,
            pauses: Vec::new(),
            anchor,
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
    writer.preamble();
    if let Some(anchor) = anchor {
        // The head marks `anchor`: declare the current position as it.
        writer.raw(&format!("G92 X{:.3} Y{:.3}", anchor.x, anchor.y));
    }
    writer.raw(&machine.start_gcode);
    let mut head = anchor.unwrap_or_default();

    let mut previous_family: Option<bool> = None;
    for layer in document
        .layers
        .iter()
        .filter(|layer| layer.output && layer.visible)
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
        if previous_family.is_some_and(|previous| previous != is_laser) {
            let reason = PauseReason::HeadChange {
                next_layer: name.clone(),
                to_laser: is_laser,
            };
            pause(
                &mut writer,
                machine,
                &format!("Head change before layer {name}"),
            );
            pauses.push(reason);
        } else if layer.pause_before && previous_family.is_some() {
            pause(&mut writer, machine, &format!("Pause before layer {name}"));
            pauses.push(PauseReason::LayerPause {
                next_layer: name.clone(),
            });
        }
        previous_family = Some(is_laser);
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
        let has_images = shapes.iter().any(|shape| shape.is_image());
        let vectors: Vec<Vec<Path>> = shapes
            .iter()
            .filter_map(|shape| vector_paths(shape))
            .collect();
        if has_images && !matches!(layer.operation, Operation::LaserImage(_)) {
            warnings.push(JobWarning::ImageOnVectorLayer(name.clone()));
        }
        match &layer.operation {
            Operation::LaserLine(params) => {
                mml_laser::emit_line(&mut writer, &mut head, vectors.concat(), params, &laser)
            }
            Operation::LaserFill(params) => {
                if vectors.iter().flatten().any(|path| !path.closed) {
                    warnings.push(JobWarning::OpenPathsNotFilled(name.clone()));
                }
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
                    mml_laser::emit_image(&mut writer, &mut head, shape, params, &laser);
                }
            }
            Operation::CncProfile(params) => {
                mml_cnc::emit_profile(&mut writer, &mut head, vectors, params, &cnc)
            }
            Operation::CncPocket(params) => {
                mml_cnc::emit_pocket(&mut writer, &mut head, vectors, params, &cnc)
            }
        }
    }

    writer.raw(&machine.end_gcode);
    if let Some(anchor) = anchor {
        if machine.kind == MachineKind::Cnc {
            writer.rapid_z(machine.safe_z);
        }
        writer.rapid(anchor.x, anchor.y);
        writer.raw("G92.1");
    } else if machine.kind == MachineKind::Cnc {
        writer.rapid_z(machine.safe_z);
        writer.rapid(0.0, 0.0);
    }
    writer.program_end();
    let gcode = match options.rotary {
        Some(rotary) => wrap_rotary(&writer.finish(), rotary),
        None => writer.finish(),
    };
    Job {
        gcode,
        bounds,
        warnings,
        pauses,
        anchor,
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
/// the material. `power` is a percentage; 0 moves with the laser off.
pub fn frame_job(
    bounds: Rect,
    anchor: Option<Point>,
    machine: &MachineProfile,
    power: f64,
    speed: f64,
) -> String {
    let mut writer = GcodeWriter::new();
    writer.preamble();
    if let Some(anchor) = anchor {
        writer.raw(&format!("G92 X{:.3} Y{:.3}", anchor.x, anchor.y));
    }
    let s_value = (power.clamp(0.0, 100.0) / 100.0 * machine.max_s).round();
    let corners = bounds.corners();
    writer.rapid(corners[0].x, corners[0].y);
    if s_value > 0.0 {
        writer.laser_on(false);
    }
    for corner in corners.iter().skip(1).chain(std::iter::once(&corners[0])) {
        writer.linear(
            corner.x,
            corner.y,
            speed,
            (s_value > 0.0).then_some(s_value),
        );
    }
    if s_value > 0.0 {
        writer.tool_off();
    }
    if let Some(anchor) = anchor {
        writer.rapid(anchor.x, anchor.y);
        writer.raw("G92.1");
    }
    writer.finish()
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
    fn out_of_bounds_design_is_reported() {
        let (document, _) = document_with_square(1000.0);
        let job = build_job(&document, &MachineProfile::generic_laser());
        assert!(job.warnings.contains(&JobWarning::OutOfBounds));
    }

    #[test]
    fn anchored_job_declares_the_head_as_the_anchor_point() {
        let (document, _) = document_with_square(1000.0);
        let options = JobOptions {
            anchor: Some(Point::new(1000.0, 50.0)),
            ..JobOptions::default()
        };
        let job = build_job_with(&document, &MachineProfile::generic_laser(), &options);
        assert!(job.gcode.contains("G92 X1000.000 Y50.000"));
        assert!(job.gcode.contains("G92.1"));
        assert!(!job.warnings.contains(&JobWarning::OutOfBounds));
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
    fn rotary_turns_y_lengths_into_angles_or_scaled_y() {
        let diameter = 360.0 / std::f64::consts::PI;
        let chuck = wrap_rotary(
            "G1X1Y10F600 ; Y in comment\nG0 Y-5",
            Rotary::AxisA { diameter },
        );
        assert_eq!(chuck, "G1X1A10.000F600 ; Y in comment\nG0 A-5.000\n");
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
