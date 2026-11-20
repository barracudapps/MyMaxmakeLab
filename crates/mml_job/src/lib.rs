//! Turns a document into the program streamed to the machine.

use mml_cnc::CncSettings;
use mml_core::document::{Document, Shape, ShapeKind};
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

#[derive(Debug, Clone)]
pub struct Job {
    pub gcode: String,
    pub bounds: Rect,
    pub warnings: Vec<JobWarning>,
}

fn vector_paths(shape: &Shape) -> Option<Vec<Path>> {
    (!shape.is_image()).then(|| shape.world_paths())
}

/// Builds the job from every visible output layer, in layer order.
pub fn build_job(document: &Document, machine: &MachineProfile) -> Job {
    let mut warnings = Vec::new();
    let output_shapes: Vec<&Shape> = document
        .shapes
        .iter()
        .filter(|shape| {
            document
                .layer(shape.layer)
                .is_some_and(|layer| layer.output && layer.visible)
        })
        .collect();
    let bounds = output_shapes
        .iter()
        .fold(Rect::EMPTY, |rect, shape| rect.union(shape.bounds()));
    if output_shapes.is_empty() {
        warnings.push(JobWarning::NothingToOutput);
        return Job {
            gcode: String::new(),
            bounds,
            warnings,
        };
    }
    let machine_area =
        Rect::from_points(Point::default(), Point::new(machine.width, machine.height));
    if !machine.relative_to_current
        && !(machine_area.contains(bounds.min) && machine_area.contains(bounds.max))
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
    if machine.relative_to_current {
        // The head marks the bottom-left corner of the design.
        writer.raw(&format!("G92 X{:.3} Y{:.3}", bounds.min.x, bounds.min.y));
    }
    writer.raw(&machine.start_gcode);
    let mut head = if machine.relative_to_current {
        bounds.min
    } else {
        Point::default()
    };

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
    if machine.relative_to_current {
        writer.rapid(bounds.min.x, bounds.min.y);
        writer.raw("G92.1");
    } else if machine.kind == MachineKind::Cnc {
        writer.rapid_z(machine.safe_z);
        writer.rapid(0.0, 0.0);
    }
    writer.program_end();
    Job {
        gcode: writer.finish(),
        bounds,
        warnings,
    }
}

/// Traces the job bounding box so the user can check the placement on
/// the material. `power` is a percentage; 0 moves with the laser off.
pub fn frame_job(bounds: Rect, machine: &MachineProfile, power: f64, speed: f64) -> String {
    let mut writer = GcodeWriter::new();
    writer.preamble();
    if machine.relative_to_current {
        writer.raw(&format!("G92 X{:.3} Y{:.3}", bounds.min.x, bounds.min.y));
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
    if machine.relative_to_current {
        writer.raw("G92.1");
    }
    writer.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mml_core::geometry::Transform;
    use mml_core::layer::{FillParams, Layer, LayerId};

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
        copy.id = mml_core::ShapeId::new();
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
}
