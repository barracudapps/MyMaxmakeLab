use crate::document::Document;
use crate::history::History;
use crate::machine::MachineProfile;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Bumped whenever the file layout changes in a non-additive way.
pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("cannot access the project file: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid project file: {0}")]
    Format(#[from] serde_json::Error),
    #[error("project format {0} is newer than this application supports")]
    TooNew(u32),
}

/// Content of a `.mml` file: the drawing and the machine it targets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub format: u32,
    pub document: Document,
    pub machine: MachineProfile,
    /// Recent named actions, so undo still works after reopening.
    #[serde(default)]
    pub history: Option<History>,
}

impl Project {
    pub fn new(document: Document, machine: MachineProfile) -> Self {
        Self {
            format: FORMAT_VERSION,
            document,
            machine,
            history: None,
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), ProjectError> {
        let json = serde_json::to_vec(self)?;
        let temporary = path.with_extension("mml.tmp");
        std::fs::write(&temporary, json)?;
        std::fs::rename(&temporary, path)?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self, ProjectError> {
        let project: Project = serde_json::from_slice(&std::fs::read(path)?)?;
        if project.format > FORMAT_VERSION {
            return Err(ProjectError::TooNew(project.format));
        }
        Ok(project)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{RasterImage, Shape, ShapeKind};
    use std::sync::Arc;

    #[test]
    fn save_and_load_round_trip_keeps_images() {
        let mut document = Document::default();
        let image = RasterImage {
            width: 2,
            height: 1,
            pixels: vec![0, 255],
        };
        document.shapes.push(Shape::new(
            "img",
            document.layers[0].id,
            ShapeKind::Image {
                image: Arc::new(image),
                width: 2.0,
                height: 1.0,
            },
        ));
        let mut history = History::default();
        history.record(Document::default(), "Import");
        let mut project = Project::new(document, MachineProfile::generic_laser());
        project.history = Some(history);
        let directory = std::env::temp_dir().join(format!("mml-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("test.mml");
        project.save(&path).unwrap();
        assert_eq!(Project::load(&path).unwrap(), project);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
