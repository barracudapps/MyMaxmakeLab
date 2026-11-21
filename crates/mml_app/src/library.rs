//! Shape library: ready-made drawings (SVG, DXF) sorted by category,
//! inserted with one click. It reads the user's own folder and, when
//! MaxmakeLAB is installed on this computer, its bundled drawings, in
//! place: nothing is copied or shipped with this application.

use crate::app::{Level, MyApp};
use crate::i18n::{tr, trf};
use eframe::egui::{self, ColorImage, RichText, TextureHandle, TextureOptions};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct Item {
    pub name: String,
    pub file: PathBuf,
    thumb: Option<PathBuf>,
}

pub struct Category {
    pub name: String,
    pub items: Vec<Item>,
}

#[derive(Default)]
pub struct LibraryState {
    pub open: bool,
    categories: Option<Vec<Category>>,
    selected: usize,
    search: String,
    textures: HashMap<PathBuf, Option<TextureHandle>>,
}

/// The user's own drawings, one sub-folder per category.
pub fn user_dir() -> Option<PathBuf> {
    crate::settings::library_dir()
}

fn maxmakelab_dirs() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(
            Path::new(&home)
                .join("Library/Application Support/Marxmake/MaxmakeLAB/data/files/library"),
        );
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        roots.push(Path::new(&appdata).join("Marxmake/MaxmakeLAB/data/files/library"));
    }
    roots.push("/Applications/MaxmakeLAB.app/Contents/Resources/data/files/library".into());
    roots.into_iter().filter(|root| root.is_dir()).collect()
}

/// "03-Plant" -> "Plant".
fn category_name(folder: &str) -> String {
    match folder.split_once('-') {
        Some((number, rest)) if number.chars().all(|c| c.is_ascii_digit()) => {
            rest.trim().to_owned()
        }
        _ => folder.to_owned(),
    }
}

/// Categories found under `root`; thumbnails are looked for in
/// `.thumb/<category>/<name>.png`, MaxmakeLAB's layout.
pub fn scan(root: &Path) -> Vec<Category> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut folders: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && !path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with('.'))
        })
        .collect();
    folders.sort();
    folders
        .into_iter()
        .filter_map(|folder| {
            let folder_name = folder.file_name()?.to_str()?.to_owned();
            let mut items: Vec<Item> = std::fs::read_dir(&folder)
                .ok()?
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    path.extension()
                        .and_then(|extension| extension.to_str())
                        .is_some_and(|extension| {
                            matches!(extension.to_lowercase().as_str(), "svg" | "dxf")
                        })
                })
                .map(|file| {
                    let stem = file
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let thumb = root
                        .join(".thumb")
                        .join(&folder_name)
                        .join(format!("{stem}.png"));
                    Item {
                        name: stem,
                        thumb: thumb.is_file().then_some(thumb),
                        file,
                    }
                })
                .collect();
            if items.is_empty() {
                return None;
            }
            // "Plant (2)" before "Plant (10)".
            items.sort_by_key(|item| {
                let number: String = item.name.chars().filter(char::is_ascii_digit).collect();
                (number.parse::<u32>().unwrap_or(u32::MAX), item.name.clone())
            });
            Some(Category {
                name: category_name(&folder_name),
                items,
            })
        })
        .collect()
}

fn load_all() -> Vec<Category> {
    let mut categories = Vec::new();
    if let Some(dir) = user_dir() {
        let _ = std::fs::create_dir_all(&dir);
        for mut category in scan(&dir) {
            category.name = format!("{} · {}", tr("library.mine"), category.name);
            categories.push(category);
        }
    }
    for root in maxmakelab_dirs() {
        categories.extend(scan(&root));
    }
    categories
}

fn thumbnail(ctx: &egui::Context, file: &Path) -> Option<TextureHandle> {
    let image = image::open(file).ok()?.thumbnail(112, 112).to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    Some(ctx.load_texture(
        file.to_string_lossy(),
        ColorImage::from_rgba_unmultiplied(size, image.as_raw()),
        TextureOptions::LINEAR,
    ))
}

fn insert(app: &mut MyApp, item_file: &Path, name: &str) {
    let result = std::fs::read(item_file)
        .map_err(|error| error.to_string())
        .and_then(|data| {
            let svg = item_file
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"));
            if svg {
                mml_io::import_svg(&data)
            } else {
                mml_io::import_dxf(&data)
            }
            .map_err(|error| error.to_string())
        });
    match result {
        Ok(groups) if !groups.is_empty() => app.import_groups(groups, name),
        Ok(_) => app.notify(tr("library.empty_file"), Level::Warning),
        Err(error) => app.notify(trf("file.open_failed", &[&error]), Level::Error),
    }
}

pub fn show(app: &mut MyApp, ctx: &egui::Context) {
    if !app.shapes_library.open {
        return;
    }
    app.shapes_library.categories.get_or_insert_with(load_all);
    let mut open = true;
    let mut chosen: Option<(PathBuf, String)> = None;
    let mut loaded_this_frame = 0;
    let state = &mut app.shapes_library;
    egui::Window::new(tr("library.title"))
        .open(&mut open)
        .default_size([720.0, 480.0])
        .max_width(980.0)
        .max_height(700.0)
        .show(ctx, |ui| {
            if state
                .categories
                .as_ref()
                .is_none_or(|categories| categories.is_empty())
            {
                ui.add(egui::Label::new(tr("library.none")).wrap());
            }
            ui.horizontal(|ui| {
                ui.label(tr("library.search"));
                ui.text_edit_singleline(&mut state.search);
                if let Some(dir) = user_dir()
                    && ui
                        .button(tr("library.my_folder"))
                        .on_hover_text(tr("library.my_folder_hint"))
                        .clicked()
                {
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = open_folder(&dir);
                }
                if ui.button(tr("library.refresh")).clicked() {
                    state.categories = None;
                    state.textures.clear();
                }
            });
            let Some(categories) = state.categories.as_ref() else {
                return;
            };
            ui.separator();
            let query = state.search.to_lowercase();
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(180.0);
                    egui::ScrollArea::vertical()
                        .id_salt("library-categories")
                        .show(ui, |ui| {
                            for (index, category) in categories.iter().enumerate() {
                                let label = format!("{} ({})", category.name, category.items.len());
                                if ui
                                    .selectable_label(
                                        state.selected == index && query.is_empty(),
                                        label,
                                    )
                                    .clicked()
                                {
                                    state.selected = index;
                                    state.search.clear();
                                }
                            }
                        });
                });
                ui.separator();
                let items: Vec<&Item> = if query.is_empty() {
                    categories
                        .get(state.selected)
                        .map(|category| category.items.iter().collect())
                        .unwrap_or_default()
                } else {
                    categories
                        .iter()
                        .flat_map(|category| category.items.iter())
                        .filter(|item| item.name.to_lowercase().contains(&query))
                        .take(300)
                        .collect()
                };
                egui::ScrollArea::vertical()
                    .id_salt("library-items")
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for item in items {
                                let texture = match &item.thumb {
                                    Some(thumb) => match state.textures.get(thumb) {
                                        Some(texture) => texture.clone(),
                                        // A few thumbnails per frame keep the
                                        // window responsive on large folders.
                                        None if loaded_this_frame < 12 => {
                                            loaded_this_frame += 1;
                                            let texture = thumbnail(ui.ctx(), thumb);
                                            state.textures.insert(thumb.clone(), texture.clone());
                                            texture
                                        }
                                        None => {
                                            ui.ctx().request_repaint();
                                            None
                                        }
                                    },
                                    None => None,
                                };
                                let response = match texture {
                                    Some(texture) => ui.add(
                                        egui::Button::image(
                                            egui::Image::new(&texture)
                                                .fit_to_exact_size(egui::vec2(72.0, 72.0)),
                                        )
                                        .min_size(egui::vec2(84.0, 84.0))
                                        .fill(egui::Color32::from_gray(238)),
                                    ),
                                    None => ui.add(
                                        egui::Button::new(RichText::new(&item.name).small())
                                            .min_size(egui::vec2(84.0, 84.0))
                                            .wrap(),
                                    ),
                                };
                                if response.on_hover_text(&item.name).clicked() {
                                    chosen = Some((item.file.clone(), item.name.clone()));
                                }
                            }
                        });
                    });
            });
        });
    if let Some((file, name)) = chosen {
        insert(app, &file, &name);
    }
    if !open {
        app.shapes_library.open = false;
    }
}

fn open_folder(dir: &Path) -> std::io::Result<std::process::Child> {
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(target_os = "windows")]
    let program = "explorer";
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let program = "xdg-open";
    std::process::Command::new(program).arg(dir).spawn()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_become_categories_with_thumbnails() {
        let root = std::env::temp_dir().join(format!("mml-library-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("02-Animals")).unwrap();
        std::fs::create_dir_all(root.join(".thumb/02-Animals")).unwrap();
        std::fs::create_dir_all(root.join("empty")).unwrap();
        for name in ["Cat (10).svg", "Cat (2).svg", "notes.txt"] {
            std::fs::write(root.join("02-Animals").join(name), "<svg/>").unwrap();
        }
        std::fs::write(root.join(".thumb/02-Animals/Cat (2).png"), "x").unwrap();
        let categories = scan(&root);
        assert_eq!(categories.len(), 1);
        assert_eq!(categories[0].name, "Animals");
        let names: Vec<&str> = categories[0]
            .items
            .iter()
            .map(|item| item.name.as_str())
            .collect();
        assert_eq!(names, ["Cat (2)", "Cat (10)"]);
        assert!(categories[0].items[0].thumb.is_some() && categories[0].items[1].thumb.is_none());
        let _ = std::fs::remove_dir_all(&root);
    }
}
