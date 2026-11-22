//! Sample projects shipped with MyMaxmakeLab: machine tests and things
//! to make, each with a step-by-step guide (stock, tools, one step per
//! tool and material, assembly). They are generated, not stored, so
//! they always match the current file format and makers.

mod clock;
mod deco;
mod esoteric;
mod mixed;
mod puzzles;
mod puzzles3d;
mod science;
pub mod shapes;
mod tests_projects;
pub mod text;

use mml_core::guide::{Guide, GuideStep};
use mml_core::layer::{
    CncPocketParams, CncProfileParams, FillParams, Layer, LayerId, LineParams, Operation,
    ProfileSide, VCarveParams,
};
use mml_core::{Document, MachineKind, Path, Shape, ShapeKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// Check and tune the machine.
    MachineTest,
    Puzzle,
    Decoration,
    Science,
    Esoteric,
    Mechanism,
}

impl Category {
    pub const ALL: [Category; 6] = [
        Self::MachineTest,
        Self::Puzzle,
        Self::Decoration,
        Self::Science,
        Self::Esoteric,
        Self::Mechanism,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::MachineTest => "category.test",
            Self::Puzzle => "category.puzzle",
            Self::Decoration => "category.deco",
            Self::Science => "category.science",
            Self::Esoteric => "category.esoteric",
            Self::Mechanism => "category.mechanism",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Sample {
    pub id: &'static str,
    pub category: Category,
    pub machine: MachineKind,
    /// Needs a router with a laser head (`machine` is then `Cnc`).
    pub hybrid: bool,
    /// 1 easy to 3 demanding.
    pub difficulty: u8,
    build: fn(&str) -> Document,
}

impl Sample {
    pub fn title(&self, language: &str) -> String {
        text::get(language, &format!("{}.title", self.id))
    }

    pub fn summary(&self, language: &str) -> String {
        text::get(language, &format!("{}.summary", self.id))
    }

    /// The project, texts in `language`.
    pub fn build(&self, language: &str) -> Document {
        (self.build)(text::supported(language))
    }
}

pub fn catalog() -> &'static [Sample] {
    use MachineKind::{Cnc, Laser};
    &[
        Sample {
            id: "cnc_calibration",
            category: Category::MachineTest,
            machine: Cnc,
            hybrid: false,
            difficulty: 1,
            build: tests_projects::cnc_calibration,
        },
        Sample {
            id: "laser_tests",
            category: Category::MachineTest,
            machine: Laser,
            hybrid: false,
            difficulty: 1,
            build: tests_projects::laser_tests,
        },
        Sample {
            id: "hybrid_alignment",
            category: Category::MachineTest,
            machine: Cnc,
            hybrid: true,
            difficulty: 2,
            build: mixed::hybrid_alignment,
        },
        Sample {
            id: "fit_test",
            category: Category::MachineTest,
            machine: Cnc,
            hybrid: false,
            difficulty: 1,
            build: tests_projects::fit_test,
        },
        Sample {
            id: "marble_maze",
            category: Category::Puzzle,
            machine: Cnc,
            hybrid: false,
            difficulty: 2,
            build: puzzles::marble_maze,
        },
        Sample {
            id: "hanoi",
            category: Category::Puzzle,
            machine: Cnc,
            hybrid: false,
            difficulty: 1,
            build: puzzles::hanoi,
        },
        Sample {
            id: "dragon",
            category: Category::Puzzle,
            machine: Laser,
            hybrid: false,
            difficulty: 2,
            build: puzzles3d::dragon,
        },
        Sample {
            id: "globe",
            category: Category::Puzzle,
            machine: Laser,
            hybrid: false,
            difficulty: 2,
            build: puzzles3d::slotted_globe,
        },
        Sample {
            id: "awale",
            category: Category::Puzzle,
            machine: Cnc,
            hybrid: true,
            difficulty: 2,
            build: mixed::awale,
        },
        Sample {
            id: "pentominoes",
            category: Category::Puzzle,
            machine: Laser,
            hybrid: false,
            difficulty: 1,
            build: puzzles::pentominoes,
        },
        Sample {
            id: "sliding_puzzle",
            category: Category::Puzzle,
            machine: Laser,
            hybrid: false,
            difficulty: 1,
            build: puzzles::sliding_puzzle,
        },
        Sample {
            id: "coasters",
            category: Category::Decoration,
            machine: Laser,
            hybrid: false,
            difficulty: 1,
            build: deco::coasters,
        },
        Sample {
            id: "christmas",
            category: Category::Decoration,
            machine: Laser,
            hybrid: false,
            difficulty: 1,
            build: deco::christmas,
        },
        Sample {
            id: "door_sign",
            category: Category::Decoration,
            machine: Cnc,
            hybrid: false,
            difficulty: 2,
            build: deco::door_sign,
        },
        Sample {
            id: "valet_tray",
            category: Category::Decoration,
            machine: Cnc,
            hybrid: true,
            difficulty: 2,
            build: mixed::valet_tray,
        },
        Sample {
            id: "benham_tops",
            category: Category::Science,
            machine: Cnc,
            hybrid: true,
            difficulty: 1,
            build: science::benham_tops,
        },
        Sample {
            id: "sundial",
            category: Category::Science,
            machine: Cnc,
            hybrid: true,
            difficulty: 2,
            build: science::sundial,
        },
        Sample {
            id: "sacred_geometry",
            category: Category::Esoteric,
            machine: Laser,
            hybrid: false,
            difficulty: 1,
            build: esoteric::sacred_geometry,
        },
        Sample {
            id: "runes",
            category: Category::Esoteric,
            machine: Cnc,
            hybrid: false,
            difficulty: 1,
            build: esoteric::runes,
        },
        Sample {
            id: "spirit_board",
            category: Category::Esoteric,
            machine: Laser,
            hybrid: false,
            difficulty: 1,
            build: esoteric::spirit_board,
        },
        Sample {
            id: "pendulum_clock",
            category: Category::Mechanism,
            machine: Cnc,
            hybrid: false,
            difficulty: 3,
            build: clock::pendulum_clock,
        },
    ]
}

pub fn find(id: &str) -> Option<&'static Sample> {
    catalog().iter().find(|sample| sample.id == id)
}

/// Starting settings, tuned for a desktop router (HiMill class) in
/// plywood and for a 10 W class diode laser; the guides say so.
pub(crate) mod settings {
    use super::*;

    pub fn profile(tool: f64, depth: f64, side: ProfileSide, tabs: u32) -> Operation {
        let (pass_depth, feed, plunge) = cutter(tool);
        Operation::CncProfile(CncProfileParams {
            side,
            tool_diameter: tool,
            depth,
            pass_depth,
            feed,
            plunge,
            spindle: 12000.0,
            climb: false,
            tabs,
            tab_width: 5.0,
            tab_height: (depth * 0.25).clamp(1.0, 2.5),
        })
    }

    pub fn pocket(tool: f64, depth: f64) -> Operation {
        let (pass_depth, feed, plunge) = cutter(tool);
        Operation::CncPocket(CncPocketParams {
            tool_diameter: tool,
            depth,
            pass_depth,
            stepover: 0.4,
            feed,
            plunge,
            spindle: 12000.0,
            climb: false,
        })
    }

    /// Pass depth, feed and plunge for a flat end mill in plywood.
    fn cutter(tool: f64) -> (f64, f64, f64) {
        if tool <= 2.0 {
            (0.6, 500.0, 150.0)
        } else if tool <= 3.5 {
            (1.0, 800.0, 200.0)
        } else {
            (1.5, 900.0, 250.0)
        }
    }

    /// A 60° V-bit following lines `depth` deep.
    pub fn v_line(depth: f64) -> Operation {
        Operation::CncProfile(CncProfileParams {
            side: ProfileSide::On,
            tool_diameter: 2.0 * depth * 30f64.to_radians().tan(),
            depth,
            pass_depth: depth,
            feed: 600.0,
            plunge: 150.0,
            spindle: 12000.0,
            climb: false,
            tabs: 0,
            tab_width: 4.0,
            tab_height: 1.0,
        })
    }

    pub fn v_carve(max_depth: f64) -> Operation {
        Operation::CncVCarve(VCarveParams {
            angle: 60.0,
            tool_diameter: 6.0,
            max_depth,
            line_spacing: 0.15,
            feed: 600.0,
            plunge: 150.0,
            spindle: 12000.0,
        })
    }

    /// Cutting 3 mm plywood.
    pub fn laser_cut() -> Operation {
        Operation::LaserLine(LineParams {
            speed: 300.0,
            power: 100.0,
            passes: 2,
            air_assist: true,
            kerf: 0.15,
            ..LineParams::default()
        })
    }

    pub fn laser_line() -> Operation {
        Operation::LaserLine(LineParams {
            speed: 1500.0,
            power: 30.0,
            passes: 1,
            air_assist: false,
            ..LineParams::default()
        })
    }

    pub fn laser_fill() -> Operation {
        Operation::LaserFill(FillParams {
            speed: 3000.0,
            power: 35.0,
            interval: 0.1,
            ..FillParams::default()
        })
    }
}

/// How the parts of a board are machined.
pub(crate) struct BoardOps {
    pub engrave: Option<Operation>,
    /// Engrave in a step of its own (another tool on a router).
    pub engrave_alone: bool,
    pub holes: Operation,
    pub outline: Operation,
}

/// Builds a sample document: layers, shapes and the guide, texts read
/// under the sample's own key prefix.
pub(crate) struct Builder {
    language: &'static str,
    prefix: &'static str,
    document: Document,
    steps: Vec<GuideStep>,
}

impl Builder {
    pub fn new(language: &'static str, prefix: &'static str) -> Self {
        Self {
            language,
            prefix,
            document: Document {
                layers: Vec::new(),
                shapes: Vec::new(),
                guide: None,
            },
            steps: Vec::new(),
        }
    }

    /// Text of `key` under this sample.
    pub fn t(&self, key: &str) -> String {
        text::get(self.language, &format!("{}.{key}", self.prefix))
    }

    pub fn tf(&self, key: &str, values: &[&str]) -> String {
        text::format(self.language, &format!("{}.{key}", self.prefix), values)
    }

    /// Shared text (`common.key`).
    pub fn common(&self, key: &str) -> String {
        text::get(self.language, &format!("common.{key}"))
    }

    pub fn layer(&mut self, name: String, operation: Operation) -> LayerId {
        let color = self.document.next_layer_color();
        let layer = Layer::new(name, color, operation);
        let id = layer.id;
        self.document.layers.push(layer);
        id
    }

    /// The outline of the stock on a layer that never runs.
    pub fn stock(&mut self, width: f64, height: f64) {
        let name = self.common("stock_layer");
        let id = self.layer(name.clone(), settings::laser_line());
        if let Some(layer) = self.document.layer_mut(id) {
            layer.output = false;
            layer.color = [150, 150, 150];
        }
        self.add(id, name, vec![shapes::board_outline(width, height)]);
    }

    pub fn add(&mut self, layer: LayerId, name: impl Into<String>, paths: Vec<Path>) {
        if paths.is_empty() {
            return;
        }
        self.document
            .shapes
            .push(Shape::new(name, layer, ShapeKind::Paths { paths }));
    }

    /// A step whose title and lines are `key.title`, `key.1`, `key.2`...
    pub fn step(&mut self, key: &str, layers: Vec<LayerId>) {
        let title = self.t(&format!("{key}.title"));
        let lines = text::lines(self.language, &format!("{}.{key}", self.prefix));
        self.step_with(title, lines, layers);
    }

    pub fn step_with(&mut self, title: String, lines: Vec<String>, layers: Vec<LayerId>) {
        self.steps.push(GuideStep {
            title,
            lines,
            layers,
        });
    }

    /// Layers and steps for parts nested on several boards: on each
    /// board the engraving, then the holes, then the outlines. A router
    /// engraves with a V-bit in a step of its own before the cut.
    pub fn cut_boards(&mut self, boards: Vec<Vec<shapes::Part>>, ops: BoardOps, key: &str) {
        let count = boards.len();
        for (index, board) in boards.into_iter().enumerate() {
            let (number, total) = ((index + 1).to_string(), count.to_string());
            let numbers = [number.as_str(), total.as_str()];
            let mut cut_layers = Vec::new();
            let engraving: Vec<(String, Vec<Path>)> = board
                .iter()
                .filter(|part| !part.engrave.is_empty())
                .map(|part| (part.name.clone(), part.engrave.clone()))
                .collect();
            if let (Some(operation), false) = (&ops.engrave, engraving.is_empty()) {
                let layer = self.layer(
                    text::format(self.language, "common.layer_engrave", &numbers),
                    operation.clone(),
                );
                for (name, paths) in engraving {
                    self.add(layer, name, paths);
                }
                if ops.engrave_alone {
                    let title = self.tf(&format!("{key}.engrave_title"), &numbers);
                    let lines =
                        text::lines(self.language, &format!("{}.{key}.engrave", self.prefix));
                    self.step_with(title, lines, vec![layer]);
                } else {
                    cut_layers.push(layer);
                }
            }
            if board.iter().any(|part| !part.holes.is_empty()) {
                let layer = self.layer(
                    text::format(self.language, "common.layer_holes", &numbers),
                    ops.holes.clone(),
                );
                for part in &board {
                    self.add(layer, part.name.clone(), part.holes.clone());
                }
                cut_layers.push(layer);
            }
            let layer = self.layer(
                text::format(self.language, "common.layer_outline", &numbers),
                ops.outline.clone(),
            );
            for part in &board {
                self.add(layer, part.name.clone(), vec![part.outline.clone()]);
            }
            cut_layers.push(layer);
            // "Parts: 2 x side, base".
            let mut names: Vec<(String, usize)> = Vec::new();
            for part in &board {
                match names.iter_mut().find(|(name, _)| *name == part.name) {
                    Some((_, count)) => *count += 1,
                    None => names.push((part.name.clone(), 1)),
                }
            }
            let list = names
                .iter()
                .map(|(name, count)| {
                    if *count > 1 {
                        format!("{count} × {name}")
                    } else {
                        name.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            let mut lines = vec![text::format(self.language, "common.board_parts", &[&list])];
            lines.extend(text::lines(
                self.language,
                &format!("{}.{key}", self.prefix),
            ));
            let title = self.tf(&format!("{key}.title"), &numbers);
            self.step_with(title, lines, cut_layers);
        }
    }

    pub fn finish(self, machine: MachineKind) -> Document {
        self.finish_guide(machine, false)
    }

    /// For a router with a laser head: each step runs one of them.
    pub fn finish_hybrid(self) -> Document {
        self.finish_guide(MachineKind::Cnc, true)
    }

    fn finish_guide(mut self, machine: MachineKind, hybrid: bool) -> Document {
        let prefix = self.prefix;
        let language = self.language;
        self.document.guide = Some(Guide {
            title: self.t("title"),
            summary: self.t("summary"),
            machine,
            hybrid,
            materials: text::lines(language, &format!("{prefix}.materials")),
            tools: text::lines(language, &format!("{prefix}.tools")),
            steps: self.steps,
        });
        // Opens on the first machining step, so overlapping boards do
        // not clutter the view.
        if let Some(first) = self
            .document
            .guide
            .as_ref()
            .and_then(|guide| guide.steps.iter().position(GuideStep::runs_machine))
        {
            self.document.prepare_step(first);
        }
        self.document
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mml_core::geometry::bounds_of;

    #[test]
    fn every_sample_builds_fits_the_board_and_has_a_guide() {
        for sample in catalog() {
            // Geometry is the same in every language; texts are checked
            // key by key in `text`.
            for language in ["fr", "en"] {
                let document = sample.build(language);
                let guide = document.guide.as_ref().expect("guide");
                assert!(!guide.title.contains('.'), "{}: {}", sample.id, guide.title);
                assert!(
                    !guide.materials.is_empty() && !guide.tools.is_empty(),
                    "{}",
                    sample.id
                );
                assert!(
                    guide.steps.iter().any(|step| step.runs_machine()),
                    "{}",
                    sample.id
                );
                for step in &guide.steps {
                    assert!(!step.lines.is_empty(), "{}: {}", sample.id, step.title);
                    for line in &step.lines {
                        assert!(
                            !line.starts_with(sample.id),
                            "{}: untranslated {line}",
                            sample.id
                        );
                    }
                    // Each layer of a step exists and holds something.
                    for layer in &step.layers {
                        assert!(document.layer(*layer).is_some());
                        assert!(
                            document.shapes.iter().any(|shape| shape.layer == *layer),
                            "{}: empty layer in {}",
                            sample.id,
                            step.title
                        );
                    }
                }
                // Every step fits in the stock placed on the bed (the
                // HiMill travel, 280 x 200).
                for step in guide.steps.iter().filter(|step| step.runs_machine()) {
                    let paths: Vec<Path> = document
                        .shapes
                        .iter()
                        .filter(|shape| step.layers.contains(&shape.layer))
                        .flat_map(|shape| shape.world_paths())
                        .collect();
                    let bounds = bounds_of(&paths);
                    assert!(
                        bounds.min.x >= 0.0 && bounds.min.y >= 0.0,
                        "{}: {:?}",
                        sample.id,
                        bounds
                    );
                    assert!(
                        bounds.max.x <= 280.0 && bounds.max.y <= 200.0,
                        "{}: {} {:?}",
                        sample.id,
                        step.title,
                        bounds
                    );
                }
                assert_eq!(guide.hybrid, sample.hybrid, "{}", sample.id);
                // One head per step: the spindle and the laser never
                // share a job, and only hybrid projects use both.
                let mut families = Vec::new();
                for step in &guide.steps {
                    let laser: Vec<bool> = step
                        .layers
                        .iter()
                        .filter_map(|id| document.layer(*id))
                        .map(|layer| layer.operation.is_laser())
                        .collect();
                    assert!(
                        laser.windows(2).all(|pair| pair[0] == pair[1]),
                        "{}: {} mixes the laser and the spindle",
                        sample.id,
                        step.title
                    );
                    families.extend(laser.first().copied());
                }
                families.sort();
                families.dedup();
                assert_eq!(families.len() == 2, sample.hybrid, "{}", sample.id);
                // One tool per step on a router: a change of cutter
                // means a new step and a new Z zero.
                if sample.machine == MachineKind::Cnc {
                    for step in &guide.steps {
                        let mut tools: Vec<String> = step
                            .layers
                            .iter()
                            .filter_map(|id| document.layer(*id))
                            .map(|layer| match &layer.operation {
                                Operation::CncProfile(p)
                                    if p.side == ProfileSide::On && p.tool_diameter < 1.5 =>
                                {
                                    "V-bit".to_owned()
                                }
                                Operation::CncVCarve(_) => "V-bit".to_owned(),
                                operation => operation.tool_label(),
                            })
                            .collect();
                        tools.dedup();
                        assert!(
                            tools.len() <= 1,
                            "{}: {} {:?}",
                            sample.id,
                            step.title,
                            tools
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn jobs_build_for_each_step() {
        for sample in catalog() {
            let mut document = sample.build("fr");
            let machine = match sample.machine {
                MachineKind::Cnc => mml_core::MachineProfile {
                    // The laser head of a hybrid project, offset from
                    // the spindle axis.
                    laser_mode: sample.hybrid,
                    laser_offset: if sample.hybrid { [2.0, -1.5] } else { [0.0; 2] },
                    ..mml_core::MachineProfile::maxmake_himill_d1s()
                },
                MachineKind::Laser => mml_core::MachineProfile::generic_laser(),
            };
            let steps = document.guide.as_ref().unwrap().steps.len();
            for step in 0..steps {
                if !document.prepare_step(step) {
                    continue;
                }
                let job = mml_job::build_job(&document, &machine);
                assert!(job.gcode.lines().count() > 20, "{} step {step}", sample.id);
                assert!(job.pauses.is_empty(), "{} step {step}", sample.id);
                assert!(
                    job.warnings.is_empty(),
                    "{} step {step}: {:?}",
                    sample.id,
                    job.warnings
                );
            }
        }
    }
}
