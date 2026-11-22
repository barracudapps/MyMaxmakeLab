//! Step-by-step guide shipped with a project: what to prepare and, for
//! each step, what to do and which layers to run. A step runs one tool
//! on one material, so changing the tool or the stock is always done
//! between two jobs, with the Z zero measured again, never in the
//! middle of one.

use crate::document::Document;
use crate::layer::LayerId;
use crate::machine::MachineKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Guide {
    pub title: String,
    pub summary: String,
    /// Machine the project was designed for.
    pub machine: MachineKind,
    /// The project also needs a laser head on that router: its steps
    /// alternate between the spindle and the laser.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hybrid: bool,
    /// Stock and supplies (wood, dowels, screws...).
    #[serde(default)]
    pub materials: Vec<String>,
    /// Cutters, bits and accessories.
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub steps: Vec<GuideStep>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuideStep {
    pub title: String,
    /// Instructions, one per line, in order.
    #[serde(default)]
    pub lines: Vec<String>,
    /// Layers this step runs; none for steps done by hand (assembly,
    /// measuring, finishing).
    #[serde(default)]
    pub layers: Vec<LayerId>,
}

impl GuideStep {
    pub fn runs_machine(&self) -> bool {
        !self.layers.is_empty()
    }
}

impl Document {
    /// Shows and outputs only the layers of `step` among the layers
    /// the guide runs: the job then holds that step alone. Layers no
    /// step runs (a reference outline of the stock) are left as they
    /// are. Returns false when the step runs no layer.
    pub fn prepare_step(&mut self, step: usize) -> bool {
        let Some(guide) = &self.guide else {
            return false;
        };
        let Some(chosen) = guide
            .steps
            .get(step)
            .map(|step| step.layers.clone())
            .filter(|layers| !layers.is_empty())
        else {
            return false;
        };
        let guided: Vec<LayerId> = guide
            .steps
            .iter()
            .flat_map(|step| step.layers.iter().copied())
            .collect();
        for layer in self
            .layers
            .iter_mut()
            .filter(|layer| guided.contains(&layer.id))
        {
            let used = chosen.contains(&layer.id);
            layer.visible = used;
            layer.output = used;
        }
        true
    }

    /// Shows and outputs every layer the guide runs, as when designing.
    pub fn show_all_steps(&mut self) {
        let Some(guide) = &self.guide else {
            return;
        };
        let guided: Vec<LayerId> = guide
            .steps
            .iter()
            .flat_map(|step| step.layers.iter().copied())
            .collect();
        for layer in self
            .layers
            .iter_mut()
            .filter(|layer| guided.contains(&layer.id))
        {
            layer.visible = true;
            layer.output = true;
        }
    }

    /// The step whose layers are exactly the guided ones shown and
    /// output.
    pub fn prepared_step(&self) -> Option<usize> {
        let guide = self.guide.as_ref()?;
        let guided: Vec<LayerId> = guide
            .steps
            .iter()
            .flat_map(|step| step.layers.iter().copied())
            .collect();
        guide.steps.iter().position(|step| {
            step.runs_machine()
                && self
                    .layers
                    .iter()
                    .filter(|layer| guided.contains(&layer.id))
                    .all(|layer| {
                        let used = step.layers.contains(&layer.id);
                        layer.visible == used && layer.output == used
                    })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::{Layer, LineParams, Operation};

    #[test]
    fn preparing_a_step_keeps_its_layers_only() {
        let mut document = Document::default();
        let second = Layer::new("b", [0, 0, 0], Operation::LaserLine(LineParams::default()));
        let (a, b) = (document.layers[0].id, second.id);
        document.layers.push(second);
        document.guide = Some(Guide {
            title: "t".into(),
            summary: String::new(),
            machine: MachineKind::Laser,
            hybrid: false,
            materials: Vec::new(),
            tools: Vec::new(),
            steps: vec![
                GuideStep {
                    title: "by hand".into(),
                    lines: Vec::new(),
                    layers: Vec::new(),
                },
                GuideStep {
                    title: "cut b".into(),
                    lines: Vec::new(),
                    layers: vec![b],
                },
            ],
        });
        assert!(!document.prepare_step(0));
        assert!(document.prepare_step(1));
        document.layers[0].output = false;
        assert!(document.layer(b).unwrap().output && document.layer(b).unwrap().visible);
        assert_eq!(document.prepared_step(), Some(1));
        // The first layer belongs to no step: a reference, left alone.
        assert!(document.layer(a).unwrap().visible);
        document.show_all_steps();
        assert!(document.layer(b).unwrap().output);
        // A saved project without a guide still loads.
        let json = r#"{"layers":[],"shapes":[]}"#;
        let loaded: Document = serde_json::from_str(json).unwrap();
        assert!(loaded.guide.is_none());
    }
}
