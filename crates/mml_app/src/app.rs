use crate::canvas::{Canvas, CanvasAction, CanvasInput, Tool};
use crate::i18n::{self, set_language, tr, trf};
use crate::machine_panel::DeviceState;
use crate::settings::{Settings, ThemeChoice, library_path};
use eframe::egui::{self, Color32, Key, KeyboardShortcut, Modifiers, RichText};
use mml_core::document::{Document, Shape, ShapeId, ShapeKind};
use mml_core::geometry::{Point, Transform};
use mml_core::history::History;
use mml_core::layer::{ImageParams, Layer, LayerId, Operation};
use mml_core::machine::{MachineKind, MachineProfile};
use mml_core::project::Project;
use mml_gcode::{JobStats, MotionSettings, Program};
use mml_job::JobWarning;
use mml_materials::Library;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Design,
    Prepare,
    Run,
}

pub struct BuiltJob {
    pub gcode: Arc<String>,
    pub program: Program,
    pub stats: JobStats,
    pub warnings: Vec<JobWarning>,
    /// G-code opened from a file rather than generated.
    pub external: Option<String>,
    pub pauses: Vec<mml_job::PauseReason>,
    pub anchor: Option<Point>,
}

/// Where the job is placed on the machine.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Placement {
    /// Design coordinates are machine work coordinates.
    MachineOrigin,
    /// The head marks this point of the design.
    Head(Anchor),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Anchor {
    Center,
    BottomLeft,
    BottomRight,
    TopLeft,
    TopRight,
    Picked(Point),
}

impl Anchor {
    pub fn resolve(self, bounds: mml_core::geometry::Rect) -> Point {
        match self {
            Self::Center => bounds.center(),
            Self::BottomLeft => bounds.min,
            Self::BottomRight => Point::new(bounds.max.x, bounds.min.y),
            Self::TopLeft => Point::new(bounds.min.x, bounds.max.y),
            Self::TopRight => bounds.max,
            Self::Picked(point) => point,
        }
    }
}

pub struct JobState {
    pub built: Option<BuiltJob>,
    pub built_revision: (u64, u64),
    pub building: Option<((u64, u64), Receiver<BuiltJob>)>,
    pub placement: Placement,
    pub selection_only: bool,
    /// Bumped when placement or selection-only change.
    pub options_revision: u64,
    pub picking_anchor: bool,
    /// Pause the job reached and that the operator must handle.
    pub waiting_pause: Option<mml_job::PauseReason>,
    /// Job key the last frame (dry run) was traced for.
    pub frame_for: Option<(u64, u64)>,
    /// Job key whose frame the operator approved.
    pub frame_approved: Option<(u64, u64)>,
    pub frame_review: bool,
    pub checklist: Option<crate::workflow::Checklist>,
    pub rotary: Option<mml_job::Rotary>,
    pub simulate: Option<f32>,
    pub simulating: bool,
    pub preview_line: Option<Option<usize>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Warning,
    Error,
}

pub use crate::text_tools::TextDialog;

pub struct ArrayDialog {
    pub columns: u32,
    pub rows: u32,
    pub gap_x: f64,
    pub gap_y: f64,
}

pub struct PresetDialog {
    pub material: String,
    pub thickness: f64,
    pub title: String,
}

/// Imported file followed for changes, so a drawing made in another
/// program (Inkscape, Krita, Procreate exporting to a synced folder,
/// a graphics tablet application) updates on the canvas when saved.
pub struct LiveLink {
    pub path: PathBuf,
    pub modified: Option<std::time::SystemTime>,
    pub shapes: Vec<ShapeId>,
}

/// A project open in a background tab, with its own undo history.
pub struct ProjectTab {
    document: Document,
    machine: MachineProfile,
    history: History,
    anchor: Document,
    revision: u64,
    saved_revision: u64,
    project_path: Option<PathBuf>,
    selection: Vec<ShapeId>,
    current_layer: LayerId,
    live_links: Vec<LiveLink>,
}

impl ProjectTab {
    fn modified(&self) -> bool {
        self.revision != self.saved_revision
            && !(self.document.shapes.is_empty() && self.project_path.is_none())
    }
}

pub struct MyApp {
    pub settings: Settings,
    pub library: Library,
    pub document: Document,
    pub machine: MachineProfile,
    history: History,
    anchor: Document,
    last_edit: Option<Instant>,
    pub revision: u64,
    saved_revision: u64,
    pub project_path: Option<PathBuf>,
    pub mode: Mode,
    pub selection: Vec<ShapeId>,
    pub current_layer: LayerId,
    pub canvas: Canvas,
    pub job: JobState,
    pub device: DeviceState,
    pub notices: Vec<(String, Level, Instant)>,
    pub text_dialog: Option<TextDialog>,
    pub array_dialog: Option<ArrayDialog>,
    pub preset_dialog: Option<PresetDialog>,
    pub machine_dialog: Option<MachineProfile>,
    pub settings_open: bool,
    pub about_open: bool,
    pub confirm_close: bool,
    pub library_offer: Vec<PathBuf>,
    applied_theme: Option<ThemeChoice>,
    lock_ratio: bool,
    pub live_links: Vec<LiveLink>,
    last_link_check: Instant,
    pub probe: crate::workflow::ProbeState,
    pub settings_editor: crate::workflow::SettingsEditor,
    pub tutorial_step: Option<usize>,
    pub relief: Option<crate::relief_dialog::ReliefDialog>,
    pub assist: Option<crate::assist::AssistDialog>,
    pub view3d: crate::view3d::View3d,
    pub tools_open: bool,
    pub tool_selected: Option<usize>,
    pub deform: Option<(ShapeId, crate::text_tools::WarpSettings)>,
    pub svg_code: Option<crate::text_tools::SvgCodeDialog>,
    /// Optimised version of the opened G-code, keyed by its length.
    pub optimize_preview: Option<(usize, (String, mml_gcode::OptimizeReport))>,
    /// Open projects; the active one lives in the fields above and its
    /// slot is `None`.
    pub tabs: Vec<Option<ProjectTab>>,
    pub active_tab: usize,
    pub closing_tab: Option<usize>,
    pub pairing: crate::pairing_dialog::PairingState,
    pub camera: crate::camera_panel::CameraState,
    pub firmware: crate::firmware::FirmwareState,
}

impl MyApp {
    pub fn new(creation: &eframe::CreationContext<'_>) -> Self {
        let settings = Settings::load();
        set_language(settings.language);
        let library = library_path()
            .and_then(|path| Library::load(&path).ok())
            .unwrap_or_default();
        let document = Document::default();
        let current_layer = document.layers[0].id;
        let library_offer = if settings.library_import_offered || !library.presets.is_empty() {
            Vec::new()
        } else {
            mml_materials::maxmakelab_library_candidates()
                .into_iter()
                .filter(|path| path.to_string_lossy().contains("en_US"))
                .take(1)
                .collect()
        };
        creation
            .egui_ctx
            .options_mut(|options| options.zoom_with_keyboard = true);
        let mut app = Self {
            machine: settings.machine.clone(),
            device: DeviceState::new(&settings),
            settings,
            library,
            anchor: document.clone(),
            document,
            history: History::default(),
            last_edit: None,
            revision: 1,
            saved_revision: 1,
            project_path: None,
            mode: Mode::Design,
            selection: Vec::new(),
            current_layer,
            canvas: Canvas::default(),
            job: JobState {
                built: None,
                built_revision: (0, 0),
                building: None,
                placement: Placement::MachineOrigin,
                selection_only: false,
                options_revision: 0,
                picking_anchor: false,
                waiting_pause: None,
                frame_for: None,
                frame_approved: None,
                frame_review: false,
                checklist: None,
                rotary: None,
                simulate: None,
                simulating: false,
                preview_line: None,
            },
            notices: Vec::new(),
            text_dialog: None,
            array_dialog: None,
            preset_dialog: None,
            machine_dialog: None,
            settings_open: false,
            about_open: false,
            confirm_close: false,
            library_offer,
            applied_theme: None,
            lock_ratio: true,
            live_links: Vec::new(),
            last_link_check: Instant::now(),
            probe: crate::workflow::ProbeState::default(),
            settings_editor: crate::workflow::SettingsEditor {
                drafts: Default::default(),
            },
            tutorial_step: None,
            relief: None,
            assist: None,
            view3d: crate::view3d::View3d::default(),
            tools_open: false,
            tool_selected: None,
            deform: None,
            svg_code: None,
            optimize_preview: None,
            tabs: vec![None],
            active_tab: 0,
            closing_tab: None,
            pairing: Default::default(),
            camera: Default::default(),
            firmware: Default::default(),
        };
        if let Some(fonts) = crate::settings::fonts_dir().filter(|directory| directory.is_dir()) {
            mml_io::add_font_directory(&fonts);
        }
        if !app.settings.tutorial_done {
            app.tutorial_step = Some(0);
        }
        app.apply_arguments(std::env::args().skip(1).collect());
        app
    }

    pub fn lock_ratio(&self) -> bool {
        self.lock_ratio
    }

    pub fn set_lock_ratio(&mut self, value: bool) {
        self.lock_ratio = value;
    }

    pub fn notify(&mut self, text: impl Into<String>, level: Level) {
        self.notices.push((text.into(), level, Instant::now()));
    }

    // ----- editing and undo -------------------------------------------

    /// `mymaxmakelab [--mode design|prepare|run] [--3d] [--play] [files…]`:
    /// files are opened or imported in order, then the step is chosen.
    /// Useful for file associations, kiosks and training material.
    fn apply_arguments(&mut self, arguments: Vec<String>) {
        let mut mode = None;
        let mut iterator = arguments.into_iter();
        while let Some(argument) = iterator.next() {
            match argument.as_str() {
                "--mode" => {
                    mode = match iterator.next().as_deref() {
                        Some("prepare") => Some(Mode::Prepare),
                        Some("run") => Some(Mode::Run),
                        _ => Some(Mode::Design),
                    }
                }
                "--3d" => self.view3d.enabled = true,
                "--play" => {
                    self.job.simulate = Some(0.0);
                    self.job.simulating = true;
                }
                path => self.open_path(Path::new(path)),
            }
        }
        if let Some(mode) = mode {
            self.mode = mode;
        }
        self.selection.clear();
    }

    /// Records an undo step before a discrete change; `label` is the
    /// translation key of the action shown in the history.
    pub fn begin_edit(&mut self, label: &'static str) {
        self.flush_burst();
        self.history.record(self.document.clone(), label);
    }

    pub fn end_edit(&mut self) {
        self.anchor = self.document.clone();
        self.revision += 1;
    }

    /// Continuous edits (sliders, fields) are grouped into one undo step
    /// per burst of changes.
    pub fn touch(&mut self) {
        let new_burst = self
            .last_edit
            .is_none_or(|time| time.elapsed() > Duration::from_millis(800));
        if new_burst {
            self.history.record(self.anchor.clone(), "history.settings");
        }
        self.last_edit = Some(Instant::now());
        self.revision += 1;
    }

    fn flush_burst(&mut self) {
        if self.last_edit.take().is_some() {
            self.anchor = self.document.clone();
        }
    }

    fn update_burst(&mut self) {
        if self
            .last_edit
            .is_some_and(|time| time.elapsed() > Duration::from_millis(800))
        {
            self.flush_burst();
        }
    }

    pub fn undo(&mut self) {
        self.flush_burst();
        if let Some(label) = self.history.undo(&mut self.document) {
            self.after_history();
            self.notify(trf("history.undone", &[&i18n::tr_key(&label)]), Level::Info);
        }
    }

    pub fn redo(&mut self) {
        self.flush_burst();
        if let Some(label) = self.history.redo(&mut self.document) {
            self.after_history();
            self.notify(trf("history.redone", &[&i18n::tr_key(&label)]), Level::Info);
        }
    }

    /// Goes back `steps` actions at once (history list).
    pub fn undo_steps(&mut self, steps: usize) {
        self.flush_burst();
        let mut done = false;
        for _ in 0..steps {
            done |= self.history.undo(&mut self.document).is_some();
        }
        if done {
            self.after_history();
        }
    }

    pub fn redo_steps(&mut self, steps: usize) {
        self.flush_burst();
        let mut done = false;
        for _ in 0..steps {
            done |= self.history.redo(&mut self.document).is_some();
        }
        if done {
            self.after_history();
        }
    }

    fn after_history(&mut self) {
        self.anchor = self.document.clone();
        self.selection
            .retain(|id| self.document.shape(*id).is_some());
        if self.document.layer(self.current_layer).is_none() {
            self.current_layer = self.document.layers[0].id;
        }
        self.revision += 1;
    }

    pub fn is_modified(&self) -> bool {
        self.revision != self.saved_revision
            && !(self.document.shapes.is_empty() && self.project_path.is_none())
    }

    pub fn delete_selection(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        self.begin_edit("history.delete");
        let selection = std::mem::take(&mut self.selection);
        self.document
            .shapes
            .retain(|shape| !selection.contains(&shape.id));
        self.end_edit();
    }

    pub fn duplicate_selection(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        self.begin_edit("history.duplicate");
        let copies: Vec<Shape> = self
            .document
            .shapes
            .iter()
            .filter(|shape| self.selection.contains(&shape.id))
            .map(|shape| {
                let mut copy = shape.clone();
                copy.id = ShapeId::new();
                copy.transform = copy.transform.then(Transform::translate(5.0, -5.0));
                copy
            })
            .collect();
        self.selection = copies.iter().map(|shape| shape.id).collect();
        self.document.shapes.extend(copies);
        self.end_edit();
    }

    pub fn transform_selection(&mut self, transform: Transform) {
        if self.selection.is_empty() {
            return;
        }
        self.begin_edit("history.transform");
        for shape in self
            .document
            .shapes
            .iter_mut()
            .filter(|shape| self.selection.contains(&shape.id))
        {
            shape.transform = shape.transform.then(transform);
        }
        self.end_edit();
    }

    pub fn add_shape(&mut self, shape: Shape) {
        self.begin_edit("history.add");
        self.selection = vec![shape.id];
        self.document.shapes.push(shape);
        self.end_edit();
    }

    /// Layer for new content of the given nature, created when missing.
    pub fn layer_for(&mut self, image: bool) -> LayerId {
        let fits = |layer: &Layer| matches!(layer.operation, Operation::LaserImage(_)) == image;
        if self.document.layer(self.current_layer).is_some_and(fits) {
            return self.current_layer;
        }
        if let Some(layer) = self.document.layers.iter().find(|layer| fits(layer)) {
            return layer.id;
        }
        let layer = if image {
            Layer::new(
                tr("layer.image"),
                self.document.next_layer_color(),
                Operation::LaserImage(ImageParams::default()),
            )
        } else {
            Layer::new(
                tr("layer.cut"),
                self.document.next_layer_color(),
                self.default_vector_operation(),
            )
        };
        let id = layer.id;
        self.document.layers.push(layer);
        id
    }

    pub fn default_vector_operation(&self) -> Operation {
        match self.machine.kind {
            MachineKind::Laser => Operation::LaserLine(Default::default()),
            MachineKind::Cnc => Operation::CncProfile(Default::default()),
        }
    }

    // ----- files ------------------------------------------------------

    pub fn new_project(&mut self) {
        self.document = Document::default();
        if self.machine.kind == MachineKind::Cnc {
            self.document.layers[0].operation = self.default_vector_operation();
        }
        self.current_layer = self.document.layers[0].id;
        self.history.clear();
        self.anchor = self.document.clone();
        self.selection.clear();
        self.project_path = None;
        self.job.built = None;
        self.revision += 1;
        self.saved_revision = self.revision;
        self.canvas.request_fit();
        self.mode = Mode::Design;
    }

    pub fn open_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(
                tr("file.all_supported"),
                &[
                    "mml", "svg", "dxf", "png", "jpg", "jpeg", "bmp", "nc", "gcode", "ngc", "tap",
                    "stl", "obj",
                ],
            )
            .pick_file()
        {
            self.open_path(&path);
        }
    }

    pub fn import_dialog(&mut self) {
        if let Some(paths) = rfd::FileDialog::new()
            .add_filter(
                tr("file.importable"),
                &["svg", "dxf", "png", "jpg", "jpeg", "bmp"],
            )
            .pick_files()
        {
            for path in paths {
                self.open_path(&path);
            }
        }
    }

    pub fn open_path(&mut self, path: &Path) {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_lowercase();
        let result: Result<(), String> = match extension.as_str() {
            "mml" => Project::load(path)
                .map_err(|error| error.to_string())
                .map(|project| {
                    if let Some(index) = self.tab_with_path(path) {
                        self.switch_tab(index);
                        return;
                    }
                    if self.is_modified()
                        || !self.document.shapes.is_empty()
                        || self.project_path.is_some()
                    {
                        self.new_tab();
                    }
                    self.new_project();
                    self.history = project.history.unwrap_or_default();
                    self.document = project.document;
                    self.machine = project.machine;
                    self.anchor = self.document.clone();
                    self.current_layer = self.document.layers[0].id;
                    self.project_path = Some(path.to_path_buf());
                    self.revision += 1;
                    self.saved_revision = self.revision;
                    self.settings.push_recent(path.to_path_buf());
                    self.canvas.request_fit();
                }),
            "nc" | "gcode" | "ngc" | "tap" | "txt" => std::fs::read_to_string(path)
                .map_err(|error| error.to_string())
                .map(|text| {
                    let program = mml_gcode::parse(&text);
                    let stats = mml_gcode::analyze(&program, MotionSettings::default());
                    let name = path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    self.job.built = Some(BuiltJob {
                        gcode: Arc::new(text),
                        program,
                        stats,
                        warnings: Vec::new(),
                        external: Some(name),
                        pauses: Vec::new(),
                        anchor: None,
                    });
                    self.job.preview_line = Some(None);
                    self.mode = Mode::Prepare;
                    self.canvas.request_fit();
                }),
            "svg" | "dxf" => std::fs::read(path)
                .map_err(|error| error.to_string())
                .and_then(|data| {
                    let groups = if extension == "svg" {
                        mml_io::import_svg(&data)
                    } else {
                        mml_io::import_dxf(&data)
                    }
                    .map_err(|error| error.to_string())?;
                    self.import_vectors(groups, path);
                    Ok(())
                }),
            "stl" | "obj" => std::fs::read(path)
                .map_err(|error| error.to_string())
                .and_then(|data| {
                    let triangles =
                        mml_relief::load_mesh(&data).map_err(|error| error.to_string())?;
                    let name = Self::file_stem(path);
                    let bed = (self.machine.width, self.machine.height);
                    self.relief = Some(crate::relief_dialog::ReliefDialog::open(
                        name, triangles, bed,
                    ));
                    Ok(())
                }),
            "png" | "jpg" | "jpeg" | "bmp" => std::fs::read(path)
                .map_err(|error| error.to_string())
                .and_then(|data| {
                    let (image, width, height) =
                        mml_io::import_raster(&data).map_err(|error| error.to_string())?;
                    self.import_image(image, width, height, path);
                    Ok(())
                }),
            _ => Err(tr("file.unsupported").to_owned()),
        };
        if let Err(error) = result {
            self.notify(
                trf("file.open_failed", &[&path.display(), &error]),
                Level::Error,
            );
        }
    }

    fn file_stem(path: &Path) -> String {
        path.file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// Places imported content at the centre of the bed when it fits,
    /// at the origin otherwise.
    /// Centre of the bed for the first import; later imports go next to
    /// the existing content (right, then above) so nothing lands on top
    /// of the previous drawing.
    fn placement(&self, width: f64, height: f64) -> Point {
        if width > self.machine.width || height > self.machine.height {
            return Point::new(width / 2.0, height / 2.0);
        }
        let center = Point::new(self.machine.width / 2.0, self.machine.height / 2.0);
        let existing = self.document.bounds();
        if existing.is_empty() {
            return center;
        }
        let gap = 5.0;
        let candidate = mml_core::geometry::Rect::from_points(
            Point::new(center.x - width / 2.0, center.y - height / 2.0),
            Point::new(center.x + width / 2.0, center.y + height / 2.0),
        );
        if !candidate.intersects(&existing) {
            return center;
        }
        let right = Point::new(existing.max.x + gap + width / 2.0, existing.center().y);
        if right.x + width / 2.0 <= self.machine.width
            && right.y + height / 2.0 <= self.machine.height
        {
            return right;
        }
        let above = Point::new(existing.center().x, existing.max.y + gap + height / 2.0);
        if above.y + height / 2.0 <= self.machine.height
            && above.x + width / 2.0 <= self.machine.width
        {
            return above;
        }
        center
    }

    fn import_vectors(&mut self, groups: Vec<mml_io::ImportedGroup>, path: &Path) {
        self.import_groups(groups, &Self::file_stem(path));
        self.link(path);
    }

    pub fn import_groups(&mut self, groups: Vec<mml_io::ImportedGroup>, name: &str) {
        self.begin_edit("history.import");
        let all: Vec<_> = groups
            .iter()
            .flat_map(|group| group.paths.iter().cloned())
            .collect();
        let bounds = mml_core::geometry::bounds_of(&all);
        let target = self.placement(bounds.width(), bounds.height());
        let offset = target - bounds.center();
        let stem = name.to_owned();
        let fallback = self.layer_for(false);
        let mut ids = Vec::new();
        let several = groups.len() > 1;
        for group in groups {
            // Each colour of a multi-colour drawing gets its own layer.
            let layer = match group.color {
                Some(color) if several => match self
                    .document
                    .layers
                    .iter()
                    .find(|layer| layer.color == color)
                {
                    Some(layer) => layer.id,
                    None => {
                        let name = crate::theme::color_name(color).to_owned();
                        let layer = Layer::new(name, color, self.default_vector_operation());
                        let id = layer.id;
                        self.document.layers.push(layer);
                        id
                    }
                },
                _ => fallback,
            };
            let paths = group
                .paths
                .into_iter()
                .map(|path| {
                    mml_core::geometry::Path::new(
                        path.points
                            .into_iter()
                            .map(|point| point - bounds.center())
                            .collect(),
                        path.closed,
                    )
                })
                .collect();
            let mut shape = Shape::new(stem.clone(), layer, ShapeKind::Paths { paths });
            shape.transform =
                Transform::translate(bounds.center().x + offset.x, bounds.center().y + offset.y);
            ids.push(shape.id);
            self.document.shapes.push(shape);
        }
        self.selection = ids;
        // A multi-colour drawing creates one layer per colour: drop the
        // untouched default layer instead of leaving an empty one.
        if several
            && self.document.layers.len() > 1
            && self.document.shapes_on(fallback).next().is_none()
        {
            self.document.remove_layer(fallback);
            if self.current_layer == fallback {
                self.current_layer = self.document.layers[0].id;
            }
        }
        self.end_edit();
        self.mode = Mode::Design;
        self.canvas.request_fit();
    }

    fn import_image(&mut self, image: mml_core::RasterImage, width: f64, height: f64, path: &Path) {
        self.begin_edit("history.import");
        // Large photos default to 100 mm wide, a sensible engraving size.
        let scale = if width > 100.0 { 100.0 / width } else { 1.0 };
        let (width, height) = (width * scale, height * scale);
        let layer = self.layer_for(true);
        let mut shape = Shape::new(
            Self::file_stem(path),
            layer,
            ShapeKind::Image {
                image: Arc::new(image),
                width,
                height,
            },
        );
        let center = self.placement(width, height);
        shape.transform = Transform::translate(center.x, center.y);
        self.selection = vec![shape.id];
        self.document.shapes.push(shape);
        self.end_edit();
        self.mode = Mode::Design;
        self.canvas.request_fit();
        self.link(path);
    }

    fn link(&mut self, path: &Path) {
        let modified = std::fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok();
        self.live_links.retain(|link| link.path != path);
        self.live_links.push(LiveLink {
            path: path.to_path_buf(),
            modified,
            shapes: self.selection.clone(),
        });
    }

    /// Re-imports linked files saved since the last check, keeping the
    /// place, the width and the layer of the previous version.
    fn refresh_live_links(&mut self) {
        if self.last_link_check.elapsed() < Duration::from_secs(1) || self.live_links.is_empty() {
            return;
        }
        self.last_link_check = Instant::now();
        self.live_links.retain(|link| {
            link.shapes
                .iter()
                .any(|id| self.document.shape(*id).is_some())
        });
        let changed: Vec<usize> = self
            .live_links
            .iter()
            .enumerate()
            .filter(|(_, link)| {
                let modified = std::fs::metadata(&link.path)
                    .and_then(|metadata| metadata.modified())
                    .ok();
                modified.is_some() && modified != link.modified
            })
            .map(|(index, _)| index)
            .collect();
        for index in changed.into_iter().rev() {
            let link = self.live_links.remove(index);
            let old_bounds = self.document.bounds_of(&link.shapes);
            let old_layer = link
                .shapes
                .iter()
                .find_map(|id| self.document.shape(*id))
                .map(|shape| shape.layer);
            let mode = self.mode;
            let selection = std::mem::take(&mut self.selection);
            self.begin_edit("history.live_update");
            self.document
                .shapes
                .retain(|shape| !link.shapes.contains(&shape.id));
            self.end_edit();
            self.open_path(&link.path);
            self.canvas.cancel_fit();
            self.mode = mode;
            let new_ids = self.selection.clone();
            let new_bounds = self.document.bounds_of(&new_ids);
            if !old_bounds.is_empty() && !new_bounds.is_empty() {
                let scale = old_bounds.width() / new_bounds.width().max(1e-9);
                let center = old_bounds.center();
                let transform = Transform::scale(scale, scale)
                    .around(new_bounds.center())
                    .then(Transform::translate(
                        center.x - new_bounds.center().x,
                        center.y - new_bounds.center().y,
                    ));
                for shape in self
                    .document
                    .shapes
                    .iter_mut()
                    .filter(|shape| new_ids.contains(&shape.id))
                {
                    shape.transform = shape.transform.then(transform);
                    if let Some(layer) = old_layer {
                        shape.layer = layer;
                    }
                }
                self.anchor = self.document.clone();
            }
            self.selection = selection
                .into_iter()
                .filter(|id| self.document.shape(*id).is_some())
                .collect();
            let name = link
                .path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            self.notify(trf("link.updated", &[&name]), Level::Info);
        }
    }

    pub fn unlink(&mut self, id: ShapeId) {
        self.live_links.retain(|link| !link.shapes.contains(&id));
    }

    pub fn link_of(&self, id: ShapeId) -> Option<&LiveLink> {
        self.live_links
            .iter()
            .find(|link| link.shapes.contains(&id))
    }

    pub fn save(&mut self, save_as: bool) -> bool {
        let path = match (&self.project_path, save_as) {
            (Some(path), false) => path.clone(),
            _ => match rfd::FileDialog::new()
                .add_filter("MyMaxmakeLab", &["mml"])
                .set_file_name(format!("{}.mml", tr("file.untitled")))
                .save_file()
            {
                Some(path) => path.with_extension("mml"),
                None => return false,
            },
        };
        let mut project = Project::new(self.document.clone(), self.machine.clone());
        // A short history keeps Cmd+Z working after reopening without
        // making the file heavy.
        project.history = Some(self.history.trimmed(15));
        match project.save(&path) {
            Ok(()) => {
                self.project_path = Some(path.clone());
                self.saved_revision = self.revision;
                self.settings.push_recent(path);
                self.notify(tr("file.saved"), Level::Info);
                true
            }
            Err(error) => {
                self.notify(trf("file.save_failed", &[&error]), Level::Error);
                false
            }
        }
    }

    pub fn export_gcode(&mut self) {
        let Some(built) = &self.job.built else {
            self.notify(tr("job.nothing"), Level::Warning);
            return;
        };
        let gcode = built.gcode.clone();
        let name = self
            .project_path
            .as_deref()
            .map(Self::file_stem)
            .unwrap_or_else(|| tr("file.untitled").to_owned());
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("G-code", &["nc", "gcode"])
            .set_file_name(format!("{name}.nc"))
            .save_file()
        {
            match std::fs::write(&path, gcode.as_bytes()) {
                Ok(()) => self.notify(trf("file.exported", &[&path.display()]), Level::Info),
                Err(error) => self.notify(trf("file.save_failed", &[&error]), Level::Error),
            }
        }
    }

    // ----- job ----------------------------------------------------------

    pub fn motion_settings(&self) -> MotionSettings {
        let mut settings = MotionSettings::default();
        if let Some(rate) = self.device.setting_f64(110) {
            settings.rapid_rate = rate;
        }
        if let Some(acceleration) = self.device.setting_f64(120) {
            settings.acceleration = acceleration;
        }
        settings
    }

    /// Regenerates the job in the background when the design changed.
    fn maintain_job(&mut self, ctx: &egui::Context) {
        if let Some((revision, receiver)) = &self.job.building
            && let Ok(built) = receiver.try_recv()
        {
            self.job.built_revision = *revision;
            self.job.built = Some(built);
            self.job.building = None;
            self.job.preview_line = Some(None);
        }
        let external = self
            .job
            .built
            .as_ref()
            .is_some_and(|built| built.external.is_some());
        let wanted = self.mode != Mode::Design && !external;
        let key = (self.revision, self.job.options_revision);
        let stale = self.job.built_revision != key;
        let idle = self
            .last_edit
            .is_none_or(|time| time.elapsed() > Duration::from_millis(250));
        if wanted && stale && idle && self.job.building.is_none() && !self.device.job_running() {
            let (sender, receiver) = channel();
            let document = self.document.clone();
            let machine = self.machine.clone();
            let motion = self.motion_settings();
            let repaint = ctx.clone();
            let options = self.job_options();
            std::thread::spawn(move || {
                let job = mml_job::build_job_with(&document, &machine, &options);
                let program = mml_gcode::parse(&job.gcode);
                let stats = mml_gcode::analyze(&program, motion);
                let _ = sender.send(BuiltJob {
                    gcode: Arc::new(job.gcode),
                    program,
                    stats,
                    warnings: job.warnings,
                    external: None,
                    pauses: job.pauses,
                    anchor: job.anchor,
                });
                repaint.request_repaint();
            });
            self.job.building = Some((key, receiver));
        }
        if let Some(line) = self.job.preview_line.take() {
            let preview = self.job.built.as_ref().and_then(|built| {
                crate::preview::render(
                    &built.program,
                    self.machine.max_s,
                    line,
                    self.machine.kind == MachineKind::Cnc,
                    ctx.theme() == egui::Theme::Dark,
                    // While simulating only what is already done shows;
                    // while running the rest stays visible, faint.
                    self.mode == Mode::Run,
                )
            });
            let first = !self.canvas.has_preview();
            self.canvas.set_preview(ctx, preview);
            if first && self.canvas.has_preview() && self.mode != Mode::Design {
                self.canvas.request_fit();
            }
        }
    }

    /// Shapes the job outputs, honouring "selection only".
    pub fn output_bounds(&self) -> mml_core::geometry::Rect {
        let only = self.job.selection_only.then_some(&self.selection);
        self.document
            .shapes
            .iter()
            .filter(|shape| {
                self.document
                    .layer(shape.layer)
                    .is_some_and(|layer| layer.output && layer.visible)
            })
            .filter(|shape| only.is_none_or(|ids| ids.contains(&shape.id)))
            .fold(mml_core::geometry::Rect::EMPTY, |rect, shape| {
                rect.union(shape.bounds())
            })
    }

    pub fn job_options(&self) -> mml_job::JobOptions {
        let anchor = match self.job.placement {
            Placement::MachineOrigin => None,
            Placement::Head(anchor) => Some(anchor.resolve(self.output_bounds())),
        };
        mml_job::JobOptions {
            only: self.job.selection_only.then(|| self.selection.clone()),
            anchor,
            rotary: self.job.rotary,
        }
    }

    pub fn set_placement(&mut self, placement: Placement) {
        if self.job.placement != placement {
            self.job.placement = placement;
            self.job.options_revision += 1;
        }
    }

    pub fn open_sequence_dialog(&mut self) {
        let Some(paths) = rfd::FileDialog::new()
            .add_filter("G-code", &["nc", "gcode", "ngc", "tap", "txt"])
            .pick_files()
        else {
            return;
        };
        let mut files = Vec::new();
        for path in &paths {
            match std::fs::read_to_string(path) {
                Ok(text) => files.push((Self::file_stem(path), text)),
                Err(error) => {
                    self.notify(
                        trf("file.open_failed", &[&path.display(), &error]),
                        Level::Error,
                    );
                    return;
                }
            }
        }
        let (text, pauses) = mml_job::sequence(&files);
        let program = mml_gcode::parse(&text);
        let stats = mml_gcode::analyze(&program, self.motion_settings());
        let names: Vec<String> = files.iter().map(|(name, _)| name.clone()).collect();
        self.job.built = Some(BuiltJob {
            gcode: Arc::new(text),
            program,
            stats,
            warnings: Vec::new(),
            external: Some(names.join(" + ")),
            pauses,
            anchor: None,
        });
        self.job.preview_line = Some(None);
        self.mode = Mode::Prepare;
    }

    /// Identifies the job content; frames and approvals are tied to it.
    pub fn job_key(&self) -> (u64, u64) {
        match &self.job.built {
            Some(built) if built.external.is_some() => (u64::MAX, built.gcode.len() as u64),
            _ => (self.revision, self.job.options_revision),
        }
    }

    pub fn job_is_current(&self) -> bool {
        self.job.built.as_ref().is_some_and(|built| {
            built.external.is_some()
                || self.job.built_revision == (self.revision, self.job.options_revision)
        })
    }

    // ----- frame --------------------------------------------------------

    fn apply_theme(&mut self, ctx: &egui::Context) {
        if self.applied_theme == Some(self.settings.theme) {
            return;
        }
        if self.applied_theme.is_none() {
            crate::theme::apply(ctx);
        }
        let preference = match self.settings.theme {
            ThemeChoice::System => egui::ThemePreference::System,
            ThemeChoice::Light => egui::ThemePreference::Light,
            ThemeChoice::Dark => egui::ThemePreference::Dark,
        };
        ctx.set_theme(preference);
        self.applied_theme = Some(self.settings.theme);
        self.job.preview_line = Some(None);
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let command = |key| KeyboardShortcut::new(Modifiers::COMMAND, key);
        let command_shift = |key| KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, key);
        let typing = ctx.memory(|memory| memory.focused().is_some());
        let pressed =
            |shortcut: KeyboardShortcut| ctx.input_mut(|input| input.consume_shortcut(&shortcut));
        if pressed(command_shift(Key::Z)) || pressed(command(Key::Y)) {
            self.redo();
        } else if pressed(command(Key::Z)) {
            self.undo();
        }
        if pressed(command_shift(Key::S)) {
            self.save(true);
        } else if pressed(command(Key::S)) {
            self.save(false);
        }
        if pressed(command(Key::O)) {
            self.open_dialog();
        }
        if pressed(command(Key::I)) {
            self.import_dialog();
        }
        if pressed(command(Key::N)) {
            self.new_tab();
        }
        if pressed(command(Key::E)) {
            self.export_gcode();
        }
        for (key, mode) in [
            (Key::Num1, Mode::Design),
            (Key::Num2, Mode::Prepare),
            (Key::Num3, Mode::Run),
        ] {
            if pressed(command(key)) {
                self.mode = mode;
            }
        }
        if typing {
            return;
        }
        if pressed(command(Key::D)) {
            self.duplicate_selection();
        }
        if pressed(command(Key::A)) && self.mode == Mode::Design {
            self.selection = self
                .document
                .shapes
                .iter()
                .filter(|shape| !shape.locked)
                .map(|shape| shape.id)
                .collect();
        }
        let keys = ctx.input(|input| {
            (
                input.key_pressed(Key::Delete) || input.key_pressed(Key::Backspace),
                input.key_pressed(Key::Escape),
                [
                    Key::ArrowLeft,
                    Key::ArrowRight,
                    Key::ArrowUp,
                    Key::ArrowDown,
                ]
                .map(|key| input.key_pressed(key)),
                input.modifiers.shift,
                [Key::V, Key::R, Key::E, Key::P, Key::T, Key::F, Key::D]
                    .map(|key| input.key_pressed(key) && input.modifiers.is_none()),
            )
        });
        let (delete, escape, arrows, shift, tools) = keys;
        if self.mode == Mode::Design {
            if delete {
                self.delete_selection();
            }
            if escape {
                self.selection.clear();
                self.canvas.tool = Tool::Select;
            }
            let step = if shift { 10.0 } else { 1.0 };
            let nudge = [(-step, 0.0), (step, 0.0), (0.0, step), (0.0, -step)];
            for (pressed, (dx, dy)) in arrows.iter().zip(nudge) {
                if *pressed {
                    self.transform_selection(Transform::translate(dx, dy));
                }
            }
            let tool_keys = [
                Tool::Select,
                Tool::Rectangle,
                Tool::Ellipse,
                Tool::Polygon,
                Tool::Text,
            ];
            for (index, tool) in tool_keys.into_iter().enumerate() {
                if tools[index] {
                    self.canvas.tool = tool;
                }
            }
            if tools[6] {
                self.canvas.tool = Tool::Pen;
            }
        }
        if tools[5] {
            self.canvas.request_fit();
        }
        if self.mode == Mode::Run && !self.device.job_running() {
            let step = self.settings.jog_step;
            let jogs = [(-step, 0.0), (step, 0.0), (0.0, step), (0.0, -step)];
            for (pressed, (dx, dy)) in arrows.iter().zip(jogs) {
                if *pressed {
                    self.device
                        .jog(dx, dy, 0.0, self.settings.jog_feed, &self.machine);
                }
            }
        }
    }

    /// Homing durations are compared with the usual one: a much slower
    /// homing is the typical sign of a binding axis or a loose belt.
    fn process_machine_events(&mut self) {
        for (duration, success) in std::mem::take(&mut self.device.homing_results) {
            if !success {
                continue;
            }
            let seconds = duration.as_secs_f64();
            let history = &self.settings.homing_times;
            if history.len() >= 3 {
                let usual = history.iter().sum::<f64>() / history.len() as f64;
                if seconds > usual * 1.4 + 2.0 {
                    let text = trf(
                        "safety.homing_slow",
                        &[&format!("{seconds:.1}"), &format!("{usual:.1}")],
                    );
                    self.notify(text, Level::Warning);
                }
            }
            self.settings.homing_times.push(seconds);
            let excess = self.settings.homing_times.len().saturating_sub(10);
            self.settings.homing_times.drain(..excess);
        }
        if let Some(index) = self.device.reached_pause.take() {
            let reason = self
                .job
                .built
                .as_ref()
                .and_then(|built| built.pauses.get(index).cloned());
            if let Some(reason) = reason {
                self.job.waiting_pause = Some(reason);
            }
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .map(|file| file.path().to_path_buf())
                .filter(|path| !path.as_os_str().is_empty())
                .collect()
        });
        for path in dropped {
            self.open_path(&path);
        }
    }

    fn take_current(&mut self) -> ProjectTab {
        self.flush_burst();
        let fresh = Document::default();
        let current_layer = fresh.layers[0].id;
        ProjectTab {
            document: std::mem::replace(&mut self.document, fresh),
            machine: self.machine.clone(),
            history: std::mem::take(&mut self.history),
            anchor: std::mem::take(&mut self.anchor),
            revision: self.revision,
            saved_revision: self.saved_revision,
            project_path: self.project_path.take(),
            selection: std::mem::take(&mut self.selection),
            current_layer: std::mem::replace(&mut self.current_layer, current_layer),
            live_links: std::mem::take(&mut self.live_links),
        }
    }

    fn restore(&mut self, tab: ProjectTab) {
        self.document = tab.document;
        self.machine = tab.machine;
        self.history = tab.history;
        self.anchor = tab.anchor;
        self.revision = tab.revision;
        self.saved_revision = tab.saved_revision;
        self.project_path = tab.project_path;
        self.selection = tab.selection;
        self.current_layer = tab.current_layer;
        self.live_links = tab.live_links;
        self.job.built = None;
        self.job.building = None;
        self.job.built_revision = (0, 0);
        self.canvas.request_fit();
    }

    pub fn switch_tab(&mut self, index: usize) {
        if index == self.active_tab || index >= self.tabs.len() {
            return;
        }
        let current = self.take_current();
        self.tabs[self.active_tab] = Some(current);
        if let Some(tab) = self.tabs[index].take() {
            self.restore(tab);
        }
        self.active_tab = index;
    }

    pub fn new_tab(&mut self) {
        let current = self.take_current();
        self.tabs[self.active_tab] = Some(current);
        self.tabs.push(None);
        self.active_tab = self.tabs.len() - 1;
        let revision = self.revision;
        self.new_project();
        self.revision = revision + 1;
        self.saved_revision = self.revision;
    }

    /// Closes a tab without asking; the interface asks first when the
    /// tab has unsaved changes.
    pub fn close_tab(&mut self, index: usize) {
        if self.tabs.len() == 1 {
            self.new_project();
            return;
        }
        if index == self.active_tab {
            let next = if index + 1 < self.tabs.len() {
                index + 1
            } else {
                index - 1
            };
            self.switch_tab(next);
        }
        self.tabs.remove(index);
        if self.active_tab > index {
            self.active_tab -= 1;
        }
    }

    fn tab_with_path(&self, path: &Path) -> Option<usize> {
        self.tabs.iter().position(|tab| {
            tab.as_ref()
                .is_some_and(|tab| tab.project_path.as_deref() == Some(path))
        })
    }

    /// Name and unsaved state of a tab.
    pub fn tab_label(&self, index: usize) -> (String, bool) {
        let untitled = || tr("file.untitled").to_owned();
        match &self.tabs[index] {
            None => (
                self.project_path
                    .as_deref()
                    .map(Self::file_stem)
                    .unwrap_or_else(untitled),
                self.is_modified(),
            ),
            Some(tab) => (
                tab.project_path
                    .as_deref()
                    .map(Self::file_stem)
                    .unwrap_or_else(untitled),
                tab.modified(),
            ),
        }
    }

    pub fn tab_modified(&self, index: usize) -> bool {
        self.tab_label(index).1
    }

    pub fn any_modified(&self) -> bool {
        (0..self.tabs.len()).any(|index| self.tab_modified(index))
    }

    pub fn history_labels(&self) -> (Vec<String>, Vec<String>) {
        (
            self.history
                .undo_labels()
                .take(15)
                .map(i18n::tr_key)
                .collect(),
            self.history
                .redo_labels()
                .take(15)
                .map(i18n::tr_key)
                .collect(),
        )
    }

    fn tab_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                let mut switch = None;
                let mut close = None;
                for index in 0..self.tabs.len() {
                    let (name, modified) = self.tab_label(index);
                    let text = if modified {
                        format!("{name} •")
                    } else {
                        name
                    };
                    let active = index == self.active_tab;
                    if ui.selectable_label(active, text).clicked() {
                        switch = Some(index);
                    }
                    if ui
                        .small_button("×")
                        .on_hover_text(tr("tabs.close"))
                        .clicked()
                    {
                        close = Some(index);
                    }
                    ui.separator();
                }
                if ui.small_button("+").on_hover_text(tr("tabs.new")).clicked() {
                    self.new_tab();
                }
                if let Some(index) = switch {
                    self.switch_tab(index);
                }
                if let Some(index) = close {
                    if self.tab_modified(index) {
                        self.closing_tab = Some(index);
                    } else {
                        self.close_tab(index);
                    }
                }
            });
        });
    }

    fn title(&self) -> String {
        let name = self
            .project_path
            .as_deref()
            .map(Self::file_stem)
            .unwrap_or_else(|| tr("file.untitled").to_owned());
        format!(
            "{}{} — MyMaxmakeLab",
            if self.is_modified() { "● " } else { "" },
            name
        )
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button(tr("menu.file"), |ui| {
                    if ui.button(tr("menu.new")).clicked() {
                        self.new_tab();
                    }
                    if ui.button(tr("menu.open")).clicked() {
                        self.open_dialog();
                    }
                    ui.menu_button(tr("menu.recent"), |ui| {
                        for path in self.settings.recent.clone() {
                            if ui.button(path.display().to_string()).clicked() {
                                self.open_path(&path);
                            }
                        }
                    });
                    ui.separator();
                    if ui.button(tr("menu.save")).clicked() {
                        self.save(false);
                    }
                    if ui.button(tr("menu.save_as")).clicked() {
                        self.save(true);
                    }
                    ui.separator();
                    if ui.button(tr("menu.import")).clicked() {
                        self.import_dialog();
                    }
                    if ui.button(tr("menu.export_gcode")).clicked() {
                        self.export_gcode();
                    }
                    if ui.button(tr("menu.open_sequence")).clicked() {
                        self.open_sequence_dialog();
                    }
                    if ui.button(tr("pairing.title")).clicked() {
                        self.pairing.open = true;
                    }
                    if ui.button(tr("menu.relief")).clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("3D", &["stl", "obj"])
                            .pick_file()
                    {
                        self.open_path(&path);
                    }
                });
                ui.menu_button(tr("menu.edit"), |ui| {
                    if ui
                        .add_enabled(self.history.can_undo(), egui::Button::new(tr("menu.undo")))
                        .clicked()
                    {
                        self.undo();
                    }
                    if ui
                        .add_enabled(self.history.can_redo(), egui::Button::new(tr("menu.redo")))
                        .clicked()
                    {
                        self.redo();
                    }
                    ui.separator();
                    if ui.button(tr("menu.duplicate")).clicked() {
                        self.duplicate_selection();
                    }
                    if ui.button(tr("menu.delete")).clicked() {
                        self.delete_selection();
                    }
                    ui.menu_button(tr("history.title"), |ui| {
                        let (undo, redo) = self.history_labels();
                        if undo.is_empty() && redo.is_empty() {
                            ui.label(RichText::new(tr("history.empty")).weak());
                        }
                        for (index, label) in redo.iter().enumerate().rev() {
                            if ui
                                .button(RichText::new(label).weak())
                                .on_hover_text(tr("history.redo_to"))
                                .clicked()
                            {
                                self.redo_steps(index + 1);
                            }
                        }
                        if !redo.is_empty() {
                            ui.label(RichText::new(tr("history.now")).strong());
                        }
                        for (index, label) in undo.iter().enumerate() {
                            if ui
                                .button(label)
                                .on_hover_text(tr("history.undo_to"))
                                .clicked()
                            {
                                self.undo_steps(index + 1);
                            }
                        }
                    });
                    ui.separator();
                    if ui.button(tr("svgcode.title")).clicked() {
                        self.svg_code = Some(Default::default());
                    }
                    if ui.button(tr("menu.array")).clicked() {
                        self.array_dialog = Some(ArrayDialog {
                            columns: 3,
                            rows: 2,
                            gap_x: 5.0,
                            gap_y: 5.0,
                        });
                    }
                });
                ui.menu_button(tr("menu.machine"), |ui| {
                    if ui.button(tr("menu.machine_settings")).clicked() {
                        self.machine_dialog = Some(self.machine.clone());
                    }
                    if ui.button(tr("tools.title")).clicked() {
                        self.tools_open = true;
                    }
                    if self.settings.advanced && ui.button(tr("firmware.title")).clicked() {
                        self.firmware.open = true;
                    }
                    ui.separator();
                    for preset in MachineProfile::presets() {
                        if ui.button(preset.name.clone()).clicked() {
                            self.machine = preset;
                            self.revision += 1;
                        }
                    }
                });
                ui.menu_button(tr("menu.materials"), |ui| {
                    if ui.button(tr("menu.import_library")).clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("LightBurn", &["clb"])
                            .pick_file()
                    {
                        self.import_library(&path);
                    }
                });
                ui.menu_button(tr("menu.view"), |ui| {
                    if ui.button(tr("menu.fit")).clicked() {
                        self.canvas.request_fit();
                    }
                    ui.checkbox(&mut self.settings.show_travel, tr("menu.show_travel"));
                    if ui.button(tr("camera.title")).clicked() {
                        self.camera.open = true;
                    }
                    ui.separator();
                    for (choice, key) in [
                        (ThemeChoice::System, "theme.system"),
                        (ThemeChoice::Light, "theme.light"),
                        (ThemeChoice::Dark, "theme.dark"),
                    ] {
                        ui.radio_value(&mut self.settings.theme, choice, tr(key));
                    }
                    ui.separator();
                    for language in i18n::Language::ALL {
                        if ui
                            .radio_value(
                                &mut self.settings.language,
                                language,
                                language.native_name(),
                            )
                            .clicked()
                        {
                            set_language(language);
                        }
                    }
                });
                ui.menu_button(tr("menu.help"), |ui| {
                    if ui.button(tr("menu.preferences")).clicked() {
                        self.settings_open = true;
                    }
                    if ui.button(tr("menu.about")).clicked() {
                        self.about_open = true;
                    }
                });
            });
        });
        egui::Panel::top("workflow").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let steps = [
                    (Mode::Design, "mode.design"),
                    (Mode::Prepare, "mode.prepare"),
                    (Mode::Run, "mode.run"),
                ];
                for (index, (mode, key)) in steps.into_iter().enumerate() {
                    let pill = crate::theme::step_pill(ui, index + 1, tr(key), self.mode == mode);
                    if pill
                        .on_hover_text(format!("Ctrl/Cmd+{}", index + 1))
                        .clicked()
                    {
                        self.mode = mode;
                    }
                }
                ui.separator();
                ui.label(RichText::new(&self.machine.name).weak());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.device.is_connected() {
                        let stop = egui::Button::new(
                            RichText::new(tr("device.emergency_stop"))
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(crate::theme::DANGER)
                        .min_size(egui::vec2(170.0, 30.0));
                        if ui
                            .add(stop)
                            .on_hover_text(tr("device.emergency_stop_hint"))
                            .clicked()
                        {
                            self.device.send(mml_device::Command::EmergencyStop);
                            self.job.waiting_pause = None;
                            self.notify(tr("device.emergency_stopped"), Level::Warning);
                        }
                    }
                    self.device.connection_badge(ui);
                });
            });
            ui.add_space(4.0);
        });
    }

    pub fn import_library(&mut self, path: &Path) {
        match std::fs::read_to_string(path)
            .map_err(|error| error.to_string())
            .and_then(|xml| {
                mml_materials::import_lightburn(&xml).map_err(|error| error.to_string())
            }) {
            Ok(presets) => {
                let added = self.library.merge(presets);
                self.save_library();
                self.notify(trf("materials.imported", &[&added]), Level::Info);
            }
            Err(error) => self.notify(
                trf("file.open_failed", &[&path.display(), &error]),
                Level::Error,
            ),
        }
    }

    pub fn save_library(&mut self) {
        if let Some(path) = library_path()
            && let Err(error) = self.library.save(&path)
        {
            self.notify(trf("file.save_failed", &[&error]), Level::Error);
        }
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(cursor) = self.canvas.cursor_world {
                    ui.monospace(format!("X {:>8.2}  Y {:>8.2}", cursor.x, cursor.y));
                    ui.separator();
                }
                ui.label(format!("{:.0} %", self.canvas.zoom / 3.78 * 100.0));
                ui.separator();
                ui.label(trf(
                    "status.selection",
                    &[&self.selection.len(), &self.document.shapes.len()],
                ));
                if self.job.building.is_some() {
                    ui.separator();
                    ui.spinner();
                    ui.label(tr("job.building"));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(tr("status.hints")).weak().small());
                });
            });
        });
    }

    fn tool_strip(&mut self, ui: &mut egui::Ui) {
        use crate::icons::{Icon, icon_button};
        egui::Panel::left("tools")
            .resizable(false)
            .exact_size(52.0)
            .show(ui, |ui| {
                ui.add_space(6.0);
                ui.vertical_centered(|ui| {
                    let tools = [
                        (Tool::Select, Icon::Select, "tool.select"),
                        (Tool::Rectangle, Icon::Rectangle, "tool.rectangle"),
                        (Tool::Ellipse, Icon::Ellipse, "tool.ellipse"),
                        (Tool::Polygon, Icon::Polygon, "tool.polygon"),
                        (Tool::Text, Icon::Text, "tool.text"),
                        (Tool::Pen, Icon::Pen, "tool.pen"),
                    ];
                    for (tool, icon, key) in tools {
                        if icon_button(ui, icon, self.canvas.tool == tool, tr(key)).clicked() {
                            self.canvas.tool = tool;
                        }
                    }
                    ui.separator();
                    if icon_button(ui, Icon::Import, false, tr("menu.import")).clicked() {
                        self.import_dialog();
                    }
                    if icon_button(ui, Icon::Fit, false, tr("menu.fit")).clicked() {
                        self.canvas.request_fit();
                    }
                    if icon_button(ui, Icon::ZoomIn, false, tr("view.zoom_in")).clicked() {
                        self.canvas.zoom_by(1.25);
                    }
                    if icon_button(ui, Icon::ZoomOut, false, tr("view.zoom_out")).clicked() {
                        self.canvas.zoom_by(0.8);
                    }
                    if icon_button(ui, Icon::Grid, self.settings.snap, tr("view.snap")).clicked() {
                        self.settings.snap = !self.settings.snap;
                    }
                });
            });
    }

    fn central(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                if self.mode != Mode::Design {
                    let mut toggle = self.view3d.enabled;
                    egui::Frame::new().inner_margin(6.0).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut toggle, false, "2D");
                            ui.selectable_value(&mut toggle, true, "3D")
                                .on_hover_text(tr("view.3d_hint"));
                            if toggle {
                                ui.add(
                                    egui::Slider::new(&mut self.view3d.z_scale, 1.0..=10.0)
                                        .text(tr("view.z_scale")),
                                );
                            }
                        });
                    });
                    self.view3d.enabled = toggle;
                    if toggle {
                        let program = self.job.built.as_ref().map(|built| &built.program);
                        let map = self.probe.map.as_ref();
                        self.view3d.show(
                            ui,
                            program,
                            map,
                            (self.machine.width, self.machine.height),
                        );
                        return;
                    }
                }
                let travel: Vec<(Point, Point)> =
                    if self.settings.show_travel && self.mode != Mode::Design {
                        self.job
                            .built
                            .as_ref()
                            .map(|built| {
                                built
                                    .program
                                    .motions
                                    .iter()
                                    .filter(|motion| motion.kind == mml_gcode::MoveKind::Rapid)
                                    .take(20_000)
                                    .map(|motion| {
                                        (
                                            Point::new(motion.from[0], motion.from[1]),
                                            Point::new(motion.to[0], motion.to[1]),
                                        )
                                    })
                                    .collect()
                            })
                            .unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                self.canvas.picking = self.job.picking_anchor && self.mode != Mode::Design;
                self.canvas.anchor_marker = (self.mode != Mode::Design)
                    .then(|| self.job_options().anchor)
                    .flatten();
                self.canvas.machine_position = (self.mode == Mode::Run)
                    .then(|| {
                        self.device.status.as_ref().map(|status| {
                            Point::new(status.work_position[0], status.work_position[1])
                        })
                    })
                    .flatten();
                let show_preview = self.mode != Mode::Design;
                let input = CanvasInput {
                    document: &self.document,
                    selection: &self.selection,
                    bed: (self.machine.width, self.machine.height),
                    grid: self.settings.grid,
                    snap: self.settings.snap,
                    current_layer: self.current_layer,
                    show_preview,
                    dim_design: show_preview && self.canvas.has_preview(),
                    travel: &travel,
                };
                match self.canvas.show(ui, input) {
                    CanvasAction::None => {}
                    CanvasAction::Select(ids) => self.selection = ids,
                    CanvasAction::Transform(transform) => self.transform_selection(transform),
                    CanvasAction::Create(shape) => self.add_shape(shape),
                    CanvasAction::Pick(point) => {
                        self.job.picking_anchor = false;
                        self.set_placement(Placement::Head(Anchor::Picked(point)));
                    }
                    CanvasAction::TextAt(position) => {
                        self.text_dialog = Some(TextDialog::new(position));
                    }
                }
            });
    }

    fn notices(&mut self, ctx: &egui::Context) {
        self.notices.retain(|(_, level, time)| {
            time.elapsed() < Duration::from_secs(if *level == Level::Error { 10 } else { 4 })
        });
        if self.notices.is_empty() {
            return;
        }
        egui::Area::new(egui::Id::unique("notices"))
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -40.0))
            .show(ctx, |ui| {
                for (text, level, _) in &self.notices {
                    let color = match level {
                        Level::Info => crate::theme::SUCCESS,
                        Level::Warning => crate::theme::WARNING,
                        Level::Error => crate::theme::DANGER,
                    };
                    egui::Frame::popup(ui.style())
                        .stroke(egui::Stroke::new(1.5, color))
                        .show(ui, |ui| {
                            ui.label(RichText::new(text).color(color).strong());
                        });
                }
            });
        ctx.request_repaint_after(Duration::from_millis(500));
    }
}

impl eframe::App for MyApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.apply_theme(&ctx);
        self.update_burst();
        self.device.poll(&ctx, &mut self.notices);
        self.process_machine_events();
        self.refresh_live_links();
        crate::workflow::process_finished(self);
        crate::pairing_dialog::poll(self);
        crate::camera_panel::poll(self, &ctx);
        self.handle_dropped_files(&ctx);
        self.shortcuts(&ctx);
        self.maintain_job(&ctx);
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.title()));
        if ctx.input(|input| input.viewport().close_requested())
            && self.any_modified()
            && !self.confirm_close
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_close = true;
        }

        let mode_before = self.mode;
        self.top_bar(ui);
        self.tab_bar(ui);
        if self.mode != mode_before {
            self.canvas.request_fit();
        }
        self.status_bar(ui);
        match self.mode {
            Mode::Design => {
                self.tool_strip(ui);
                egui::Panel::right("design-panel")
                    .resizable(true)
                    .default_size(330.0)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .show(ui, |ui| crate::panels::design_panel(self, ui));
                    });
            }
            Mode::Prepare => {
                egui::Panel::right("prepare-panel")
                    .resizable(true)
                    .default_size(340.0)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .show(ui, |ui| crate::panels::prepare_panel(self, ui));
                    });
            }
            Mode::Run => {
                egui::Panel::right("run-panel")
                    .resizable(true)
                    .default_size(360.0)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .show(ui, |ui| crate::machine_panel::run_panel(self, ui));
                    });
            }
        }
        self.central(ui);
        crate::panels::dialogs(self, &ctx);
        self.notices(&ctx);
        if self.device.is_connected()
            || self.job.building.is_some()
            || self.job.simulating
            || self.pairing.server.is_some()
        {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        self.settings.machine = self.machine.clone();
        self.device.store_connection(&mut self.settings);
        self.settings.save();
    }
}
