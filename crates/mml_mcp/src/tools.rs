//! The tools offered to assistants and their implementation, on top of
//! the libraries of the application.

use mml_core::document::{Document, Shape, ShapeKind};
use mml_core::geometry::Path;
use mml_core::layer::{
    CncPocketParams, CncProfileParams, FillParams, LineParams, Operation, ProfileSide,
};
use mml_core::machine::MachineProfile;
use mml_core::makers::{boxes, jigsaw, maze};
use mml_core::ornament::{OrnamentLayout, OrnamentParams, OrnamentStyle};
use serde_json::{Value, json};

fn number(arguments: &Value, key: &str, default: f64) -> f64 {
    arguments[key].as_f64().unwrap_or(default)
}

fn count(arguments: &Value, key: &str, default: u64) -> u64 {
    arguments[key].as_u64().unwrap_or(default)
}

fn flag(arguments: &Value, key: &str, default: bool) -> bool {
    arguments[key].as_bool().unwrap_or(default)
}

fn text<'a>(arguments: &'a Value, key: &str) -> Option<&'a str> {
    arguments[key].as_str().filter(|value| !value.is_empty())
}

/// Paths as an SVG in millimetres, Y up turned into SVG's Y down.
pub fn svg(paths: &[Path]) -> String {
    let bounds = paths
        .iter()
        .fold(mml_core::geometry::Rect::EMPTY, |rect, path| {
            rect.union(path.bounds())
        });
    let (width, height) = (bounds.width().max(1.0), bounds.height().max(1.0));
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width:.2}mm\" height=\"{height:.2}mm\" viewBox=\"0 0 {width:.3} {height:.3}\">\n"
    );
    for path in paths.iter().filter(|path| path.points.len() >= 2) {
        let points: Vec<String> = path
            .points
            .iter()
            .map(|point| {
                format!(
                    "{:.3},{:.3}",
                    point.x - bounds.min.x,
                    bounds.max.y - point.y
                )
            })
            .collect();
        let tag = if path.closed { "polygon" } else { "polyline" };
        out.push_str(&format!(
            "<{tag} points=\"{}\" fill=\"none\" stroke=\"black\" stroke-width=\"0.2\"/>\n",
            points.join(" ")
        ));
    }
    out.push_str("</svg>\n");
    out
}

/// Writes `content` to `path`, refusing to replace a file unless asked.
fn save(arguments: &Value, content: &str) -> Result<Option<String>, String> {
    let Some(path) = text(arguments, "output_path") else {
        return Ok(None);
    };
    let target = std::path::Path::new(path);
    if target.exists() && !flag(arguments, "overwrite", false) {
        return Err(format!("{path} exists; pass overwrite: true to replace it"));
    }
    std::fs::write(target, content).map_err(|error| format!("cannot write {path}: {error}"))?;
    Ok(Some(path.to_owned()))
}

/// The SVG itself, or where it was saved.
fn deliver_svg(arguments: &Value, paths: &[Path]) -> Result<String, String> {
    let content = svg(paths);
    Ok(match save(arguments, &content)? {
        Some(path) => format!("SVG saved to {path} ({} paths).", paths.len()),
        None => content,
    })
}

fn source(arguments: &Value, inline: &str, file: &str) -> Result<Vec<u8>, String> {
    if let Some(content) = text(arguments, inline) {
        return Ok(content.as_bytes().to_vec());
    }
    let path = text(arguments, file).ok_or(format!("give {inline} or {file}"))?;
    std::fs::read(path).map_err(|error| format!("cannot read {path}: {error}"))
}

fn machine(arguments: &Value, cnc: bool) -> MachineProfile {
    let wanted = text(arguments, "machine").map(str::to_lowercase);
    wanted
        .and_then(|wanted| {
            MachineProfile::presets()
                .into_iter()
                .find(|preset| preset.name.to_lowercase().contains(&wanted))
        })
        .unwrap_or_else(|| {
            if cnc {
                MachineProfile::generic_cnc()
            } else {
                MachineProfile::generic_laser()
            }
        })
}

fn summary(gcode: &str) -> Value {
    let program = mml_gcode::parse(gcode);
    let stats = mml_gcode::analyze(&program, mml_gcode::MotionSettings::default());
    let bounds = stats.cut_bounds;
    json!({
        "lines": program.line_count,
        "estimated_seconds": stats.duration.as_secs(),
        "cut_length_mm": (stats.cut_length * 10.0).round() / 10.0,
        "travel_length_mm": (stats.travel_length * 10.0).round() / 10.0,
        "bounds_mm": if bounds.is_empty() { Value::Null } else {
            json!([[bounds.min.x, bounds.min.y], [bounds.max.x, bounds.max.y]])
        },
        "z_range_mm": [program.z_range.0, program.z_range.1],
        "uses_rotary_axis": program.uses_rotary,
        "warnings": program.warnings.iter().take(10).collect::<Vec<_>>(),
    })
}

/// G-code itself (small programs) or saved with its summary.
fn deliver_gcode(arguments: &Value, gcode: &str) -> Result<String, String> {
    let summary = summary(gcode);
    match save(arguments, gcode)? {
        Some(path) => Ok(format!(
            "G-code saved to {path}. Open it in MyMaxmakeLab to preview, frame and run it.\n{summary:#}"
        )),
        None if gcode.len() > 100_000 => Err(format!(
            "The program is large ({} lines): give output_path to save it.\n{summary:#}",
            gcode.lines().count()
        )),
        None => Ok(format!("{summary:#}\n\n{gcode}")),
    }
}

pub fn definitions() -> Vec<Value> {
    let output = json!({"type": "string", "description": "Optional file to write the result to"});
    let overwrite = json!({"type": "boolean", "description": "Replace output_path if it exists"});
    vec![
        json!({"name": "list_machine_presets", "description": "Machine profiles known to MyMaxmakeLab (bed size, laser or CNC, tool changer).",
            "inputSchema": {"type": "object", "properties": {}}}),
        json!({"name": "list_fonts", "description": "Fonts available for text: bundled fonts and single-line (Hershey) fonts for pen, V-bit and fast marking.",
            "inputSchema": {"type": "object", "properties": {}}}),
        json!({"name": "generate_box", "description": "Finger-jointed box panels (SVG) for an object: inside size, sheet thickness, lid, kerf compensation.",
            "inputSchema": {"type": "object", "required": ["width", "depth", "height"], "properties": {
                "width": {"type": "number", "description": "Inside width, mm"}, "depth": {"type": "number"}, "height": {"type": "number"},
                "thickness": {"type": "number", "default": 3}, "finger": {"type": "number", "default": 10},
                "lid": {"type": "boolean", "default": true}, "kerf": {"type": "number", "default": 0.1},
                "output_path": output, "overwrite": overwrite}}}),
        json!({"name": "generate_maze", "description": "Perfect maze (SVG): walls to engrave or cut, or corridor centre lines to mill.",
            "inputSchema": {"type": "object", "properties": {
                "columns": {"type": "integer", "default": 15}, "rows": {"type": "integer", "default": 10},
                "cell": {"type": "number", "default": 8}, "output": {"type": "string", "enum": ["walls", "corridors"]},
                "solution": {"type": "boolean", "default": false}, "seed": {"type": "integer", "default": 1},
                "output_path": output, "overwrite": overwrite}}}),
        json!({"name": "generate_jigsaw", "description": "Jigsaw puzzle cut lines (SVG) with classic knobs.",
            "inputSchema": {"type": "object", "properties": {
                "width": {"type": "number", "default": 200}, "height": {"type": "number", "default": 150},
                "columns": {"type": "integer", "default": 8}, "rows": {"type": "integer", "default": 6},
                "knob": {"type": "number", "default": 0.2}, "seed": {"type": "integer", "default": 1},
                "output_path": output, "overwrite": overwrite}}}),
        json!({"name": "generate_ornament", "description": "Random ornament by cultural style (SVG): friezes or tiling.",
            "inputSchema": {"type": "object", "properties": {
                "style": {"type": "string", "enum": ["viking", "celtic", "african", "maya", "greek", "japanese", "art_deco", "polynesian", "islamic"]},
                "layout": {"type": "string", "enum": ["bands", "tiles"]},
                "width": {"type": "number", "default": 120}, "height": {"type": "number", "default": 60},
                "cell": {"type": "number", "default": 12}, "seed": {"type": "integer", "default": 1},
                "output_path": output, "overwrite": overwrite}}}),
        json!({"name": "text_to_svg", "description": "Text turned into vector outlines (SVG) with a bundled or single-line font.",
            "inputSchema": {"type": "object", "required": ["text"], "properties": {
                "text": {"type": "string"}, "font": {"type": "string", "default": "sans-serif"},
                "size": {"type": "number", "default": 10, "description": "mm"},
                "output_path": output, "overwrite": overwrite}}}),
        json!({"name": "svg_to_gcode", "description": "Turns an SVG into a G-code program with one operation. Review the result in MyMaxmakeLab before running it.",
            "inputSchema": {"type": "object", "properties": {
                "svg": {"type": "string", "description": "SVG content"}, "svg_path": {"type": "string"},
                "operation": {"type": "string", "enum": ["laser_line", "laser_fill", "cnc_profile_outside", "cnc_profile_inside", "cnc_pocket"]},
                "power": {"type": "number", "description": "Laser power, percent", "default": 80},
                "speed": {"type": "number", "description": "Laser speed or CNC feed, mm/min"},
                "passes": {"type": "integer", "default": 1},
                "depth": {"type": "number", "description": "CNC depth, mm", "default": 3},
                "tool_diameter": {"type": "number", "description": "CNC tool, mm", "default": 3.175},
                "machine": {"type": "string", "description": "Preset name, e.g. 'GRBL laser', 'Makera Carvera'"},
                "output_path": output, "overwrite": overwrite}}}),
        json!({"name": "analyze_gcode", "description": "Duration estimate, size, cut and travel lengths, Z range and warnings of a G-code program.",
            "inputSchema": {"type": "object", "properties": {"gcode": {"type": "string"}, "gcode_path": {"type": "string"}}}}),
        json!({"name": "optimize_gcode", "description": "Repairs and lightens a G-code program (comments, repeated words, zero moves, colinear segments) keeping the same path.",
            "inputSchema": {"type": "object", "properties": {"gcode": {"type": "string"}, "gcode_path": {"type": "string"},
                "output_path": output, "overwrite": overwrite}}}),
        json!({"name": "stl_relief_to_gcode", "description": "3D relief carving program from an STL or OBJ model (roughing and finishing by drop-cutter).",
            "inputSchema": {"type": "object", "required": ["model_path", "output_path"], "properties": {
                "model_path": {"type": "string"}, "width": {"type": "number", "default": 80}, "depth": {"type": "number", "default": 5},
                "tool": {"type": "string", "enum": ["ball", "flat"]}, "tool_diameter": {"type": "number", "default": 3.175},
                "output_path": output, "overwrite": overwrite}}}),
    ]
}

pub fn call(name: &str, arguments: &Value) -> Result<String, String> {
    match name {
        "list_machine_presets" => Ok(serde_json::to_string_pretty(
            &MachineProfile::presets()
                .iter()
                .map(|preset| {
                    json!({"name": preset.name, "kind": format!("{:?}", preset.kind), "bed_mm": [preset.width, preset.height],
                        "z_travel_mm": preset.z_travel, "laser_mode": preset.laser_mode, "tool_changer": preset.tool_changer})
                })
                .collect::<Vec<_>>(),
        )
        .unwrap_or_default()),
        "list_fonts" => Ok(json!({
            "fonts": mml_io::bundled_families(),
            "single_line_fonts": mml_io::hershey_families(),
        })
        .to_string()),
        "generate_box" => {
            let params = boxes::BoxParams {
                inner: [number(arguments, "width", 80.0), number(arguments, "depth", 60.0), number(arguments, "height", 40.0)],
                thickness: number(arguments, "thickness", 3.0),
                finger: number(arguments, "finger", 10.0),
                lid: flag(arguments, "lid", true),
                kerf: number(arguments, "kerf", 0.1),
                spacing: 4.0,
            };
            let panels: Vec<Path> = boxes::layout(&params, 600.0).into_iter().map(|panel| panel.outline).collect();
            deliver_svg(arguments, &panels)
        }
        "generate_maze" => {
            let params = maze::MazeParams {
                columns: count(arguments, "columns", 15) as usize,
                rows: count(arguments, "rows", 10) as usize,
                cell: number(arguments, "cell", 8.0),
                output: if text(arguments, "output") == Some("corridors") {
                    maze::MazeOutput::Corridors
                } else {
                    maze::MazeOutput::Walls
                },
                seed: count(arguments, "seed", 1),
            };
            let generated = maze::generate(&params);
            let mut paths = generated.paths;
            if flag(arguments, "solution", false) {
                paths.push(generated.solution);
            }
            deliver_svg(arguments, &paths)
        }
        "generate_jigsaw" => {
            let params = jigsaw::JigsawParams {
                width: number(arguments, "width", 200.0),
                height: number(arguments, "height", 150.0),
                columns: count(arguments, "columns", 8) as usize,
                rows: count(arguments, "rows", 6) as usize,
                knob: number(arguments, "knob", 0.2),
                seed: count(arguments, "seed", 1),
            };
            deliver_svg(arguments, &jigsaw::generate(&params))
        }
        "generate_ornament" => {
            let wanted = text(arguments, "style").unwrap_or("viking").to_lowercase();
            let style = OrnamentStyle::ALL
                .into_iter()
                .find(|style| style.key().ends_with(&wanted))
                .ok_or(format!("unknown style {wanted}"))?;
            let params = OrnamentParams {
                style,
                layout: if text(arguments, "layout") == Some("tiles") { OrnamentLayout::Tiles } else { OrnamentLayout::Bands },
                width: number(arguments, "width", 120.0),
                height: number(arguments, "height", 60.0),
                cell: number(arguments, "cell", 12.0),
                seed: count(arguments, "seed", 1),
            };
            deliver_svg(arguments, &mml_core::ornament::generate(&params))
        }
        "text_to_svg" => {
            let content = text(arguments, "text").ok_or("text is required")?;
            let style = mml_io::TextStyle {
                family: text(arguments, "font").unwrap_or("sans-serif").to_owned(),
                size: number(arguments, "size", 10.0),
                ..mml_io::TextStyle::default()
            };
            let paths = mml_io::text_to_paths(content, &style);
            if paths.is_empty() {
                return Err("no outline produced: check the font name with list_fonts".into());
            }
            deliver_svg(arguments, &paths)
        }
        "svg_to_gcode" => {
            let data = source(arguments, "svg", "svg_path")?;
            let groups = mml_io::import_svg(&data).map_err(|error| error.to_string())?;
            let operation_name = text(arguments, "operation").unwrap_or("laser_line");
            let cnc = operation_name.starts_with("cnc");
            let speed = arguments["speed"].as_f64();
            let passes = count(arguments, "passes", 1) as u32;
            let power = number(arguments, "power", 80.0);
            let depth = number(arguments, "depth", 3.0);
            let tool_diameter = number(arguments, "tool_diameter", 3.175);
            let operation = match operation_name {
                "laser_line" => Operation::LaserLine(LineParams {
                    power,
                    passes,
                    speed: speed.unwrap_or(600.0),
                    ..LineParams::default()
                }),
                "laser_fill" => Operation::LaserFill(FillParams {
                    power,
                    passes,
                    speed: speed.unwrap_or(6000.0),
                    ..FillParams::default()
                }),
                "cnc_profile_outside" | "cnc_profile_inside" => Operation::CncProfile(CncProfileParams {
                    side: if operation_name.ends_with("inside") { ProfileSide::Inside } else { ProfileSide::Outside },
                    depth,
                    tool_diameter,
                    feed: speed.unwrap_or(CncProfileParams::default().feed),
                    ..CncProfileParams::default()
                }),
                "cnc_pocket" => Operation::CncPocket(CncPocketParams {
                    depth,
                    tool_diameter,
                    feed: speed.unwrap_or(CncPocketParams::default().feed),
                    ..CncPocketParams::default()
                }),
                other => return Err(format!("unknown operation {other}")),
            };
            let mut document = Document::default();
            document.layers[0].operation = operation;
            let layer = document.layers[0].id;
            let paths: Vec<Path> = groups.into_iter().flat_map(|group| group.paths).collect();
            if paths.is_empty() {
                return Err("the SVG contains no drawable path".into());
            }
            document.shapes.push(Shape::new("svg", layer, ShapeKind::Paths { paths }));
            let profile = machine(arguments, cnc);
            let job = mml_job::build_job(&document, &profile);
            let mut text = deliver_gcode(arguments, &job.gcode)?;
            if !job.warnings.is_empty() {
                text.push_str(&format!("\nWarnings: {:?}", job.warnings));
            }
            Ok(text)
        }
        "analyze_gcode" => {
            let data = source(arguments, "gcode", "gcode_path")?;
            Ok(format!("{:#}", summary(&String::from_utf8_lossy(&data))))
        }
        "optimize_gcode" => {
            let data = source(arguments, "gcode", "gcode_path")?;
            let (optimized, report) =
                mml_gcode::optimize(&String::from_utf8_lossy(&data), mml_gcode::OptimizeOptions::default());
            let report = format!(
                "{} lines -> {} lines, {} -> {} bytes; {} comments, {} repeated words, {} zero moves removed, {} segments merged{}",
                report.lines_before,
                report.lines_after,
                report.bytes_before,
                report.bytes_after,
                report.comments_removed,
                report.words_removed,
                report.zero_moves_removed,
                report.segments_merged,
                if report.warnings.is_empty() { String::new() } else { format!("; warnings: {:?}", report.warnings) }
            );
            Ok(format!("{report}\n{}", deliver_gcode(arguments, &optimized)?))
        }
        "stl_relief_to_gcode" => {
            let path = text(arguments, "model_path").ok_or("model_path is required")?;
            if text(arguments, "output_path").is_none() {
                return Err("output_path is required for relief programs".into());
            }
            let data = std::fs::read(path).map_err(|error| format!("cannot read {path}: {error}"))?;
            let triangles = mml_relief::load_mesh(&data).map_err(|error| error.to_string())?;
            let params = mml_relief::ReliefParams {
                width: number(arguments, "width", 80.0),
                depth: number(arguments, "depth", 5.0),
                tool: if text(arguments, "tool") == Some("flat") { mml_relief::ToolShape::Flat } else { mml_relief::ToolShape::Ball },
                tool_diameter: number(arguments, "tool_diameter", 3.175),
                ..mml_relief::ReliefParams::default()
            };
            deliver_gcode(arguments, &mml_relief::relief_gcode(&triangles, &params))
        }
        other => Err(format!("unknown tool {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_generator_answers_with_an_svg() {
        for (name, arguments) in [
            (
                "generate_box",
                json!({"width": 50, "depth": 40, "height": 30}),
            ),
            (
                "generate_maze",
                json!({"columns": 6, "rows": 5, "solution": true}),
            ),
            ("generate_jigsaw", json!({"columns": 3, "rows": 2})),
            ("generate_ornament", json!({"style": "art_deco"})),
        ] {
            let result = call(name, &arguments).unwrap();
            assert!(
                result.starts_with("<svg") && result.contains("<poly"),
                "{name}"
            );
        }
        assert!(call("generate_ornament", &json!({"style": "klingon"})).is_err());
    }

    #[test]
    fn an_svg_becomes_a_program_with_its_summary() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="40mm" height="20mm" viewBox="0 0 40 20"><rect x="5" y="5" width="30" height="10" fill="none" stroke="black"/></svg>"#;
        let result = call(
            "svg_to_gcode",
            &json!({"svg": svg, "operation": "laser_line", "power": 60}),
        )
        .unwrap();
        assert!(
            result.contains("\"lines\"") && result.contains("G1"),
            "{result}"
        );
        let analysis = call(
            "analyze_gcode",
            &json!({"gcode": "G0 X0 Y0\nM3 S100\nG1 X10 F600\nM5"}),
        )
        .unwrap();
        assert!(analysis.contains("cut_length_mm"));
        let optimized = call(
            "optimize_gcode",
            &json!({"gcode": "G1 X1 F600\nG1 X2\nG1 X3\n"}),
        )
        .unwrap();
        assert!(optimized.contains("segments merged"));
    }

    #[test]
    fn existing_files_are_not_replaced_without_consent() {
        let path = std::env::temp_dir().join(format!("mml-mcp-{}.svg", std::process::id()));
        std::fs::write(&path, "keep").unwrap();
        let arguments = json!({"columns": 4, "rows": 4, "output_path": path.to_string_lossy()});
        assert!(call("generate_maze", &arguments).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep");
        let mut replace = arguments.clone();
        replace["overwrite"] = json!(true);
        assert!(call("generate_maze", &replace).unwrap().contains("saved"));
        std::fs::remove_file(&path).unwrap();
    }
}
