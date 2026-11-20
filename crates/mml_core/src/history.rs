use crate::document::Document;

/// Snapshot based undo stack. Documents share image buffers through
/// `Arc`, so a snapshot only copies the vector data.
#[derive(Debug, Default)]
pub struct History {
    undo: Vec<Document>,
    redo: Vec<Document>,
}

const LIMIT: usize = 200;

impl History {
    /// Records `before`, the state preceding a change about to happen.
    pub fn record(&mut self, before: Document) {
        if self.undo.last() == Some(&before) {
            return;
        }
        self.undo.push(before);
        if self.undo.len() > LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self, current: &mut Document) -> bool {
        match self.undo.pop() {
            Some(previous) => {
                self.redo.push(std::mem::replace(current, previous));
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self, current: &mut Document) -> bool {
        match self.redo.pop() {
            Some(next) => {
                self.undo.push(std::mem::replace(current, next));
                true
            }
            None => false,
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Shape, ShapeKind};

    #[test]
    fn undo_then_redo_restores_states() {
        let mut history = History::default();
        let mut document = Document::default();
        history.record(document.clone());
        let layer = document.layers[0].id;
        document.shapes.push(Shape::new(
            "e",
            layer,
            ShapeKind::Ellipse { rx: 1.0, ry: 1.0 },
        ));
        assert!(history.undo(&mut document));
        assert!(document.shapes.is_empty());
        assert!(history.redo(&mut document));
        assert_eq!(document.shapes.len(), 1);
    }
}
