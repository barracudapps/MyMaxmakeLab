use crate::document::Document;
use serde::{Deserialize, Serialize};

/// One undoable action: its name and the document on the other side of
/// it (before the action on the undo stack, after it on the redo
/// stack).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub label: String,
    pub document: Document,
}

/// Snapshot based undo stack with named actions. Documents share image
/// buffers through `Arc`, so a snapshot only copies the vector data. A
/// trimmed copy is saved in the project so undo survives reopening.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
}

const LIMIT: usize = 200;

impl History {
    /// Records `before`, the state preceding the action `label`.
    pub fn record(&mut self, before: Document, label: &str) {
        if self.undo.last().is_some_and(|step| step.document == before) {
            return;
        }
        self.undo.push(Step {
            label: label.to_owned(),
            document: before,
        });
        if self.undo.len() > LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Goes back one action; returns its name.
    pub fn undo(&mut self, current: &mut Document) -> Option<String> {
        let step = self.undo.pop()?;
        let after = std::mem::replace(current, step.document);
        self.redo.push(Step {
            label: step.label.clone(),
            document: after,
        });
        Some(step.label)
    }

    pub fn redo(&mut self, current: &mut Document) -> Option<String> {
        let step = self.redo.pop()?;
        let before = std::mem::replace(current, step.document);
        self.undo.push(Step {
            label: step.label.clone(),
            document: before,
        });
        Some(step.label)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Names of the actions that can be undone, most recent first.
    pub fn undo_labels(&self) -> impl Iterator<Item = &str> {
        self.undo.iter().rev().map(|step| step.label.as_str())
    }

    /// Names of the actions that can be redone, next first.
    pub fn redo_labels(&self) -> impl Iterator<Item = &str> {
        self.redo.iter().rev().map(|step| step.label.as_str())
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    /// Copy keeping the `keep` most recent steps each way, to store in
    /// the project file without making it heavy.
    pub fn trimmed(&self, keep: usize) -> History {
        let tail = |steps: &[Step]| steps[steps.len().saturating_sub(keep)..].to_vec();
        History {
            undo: tail(&self.undo),
            redo: tail(&self.redo),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Shape, ShapeKind};

    fn with_shape(mut document: Document) -> Document {
        let layer = document.layers[0].id;
        document.shapes.push(Shape::new(
            "e",
            layer,
            ShapeKind::Ellipse { rx: 1.0, ry: 1.0 },
        ));
        document
    }

    #[test]
    fn undo_then_redo_restores_states_and_names() {
        let mut history = History::default();
        let mut document = Document::default();
        history.record(document.clone(), "add");
        document = with_shape(document);
        assert_eq!(history.undo_labels().collect::<Vec<_>>(), vec!["add"]);
        assert_eq!(history.undo(&mut document).as_deref(), Some("add"));
        assert!(document.shapes.is_empty());
        assert_eq!(history.redo_labels().collect::<Vec<_>>(), vec!["add"]);
        assert_eq!(history.redo(&mut document).as_deref(), Some("add"));
        assert_eq!(document.shapes.len(), 1);
    }

    #[test]
    fn trimmed_history_keeps_the_most_recent_steps() {
        let mut history = History::default();
        let mut document = Document::default();
        for index in 0..30 {
            history.record(document.clone(), &format!("step {index}"));
            document = with_shape(document);
        }
        let trimmed = history.trimmed(10);
        assert_eq!(trimmed.undo_labels().count(), 10);
        assert_eq!(trimmed.undo_labels().next(), Some("step 29"));
        let json = serde_json::to_string(&trimmed).unwrap();
        let back: History = serde_json::from_str(&json).unwrap();
        assert_eq!(back, trimmed);
    }
}
