use crate::{ImportError, ImportedGroup, TOLERANCE_MM};
use mml_core::geometry::{Path, Point, flatten_cubic, flatten_quadratic};
use usvg::tiny_skia_path::PathSegment;

const PX_TO_MM: f64 = 25.4 / 96.0;

pub fn import_svg(data: &[u8]) -> Result<Vec<ImportedGroup>, ImportError> {
    let options = usvg::Options {
        fontdb: crate::text::font_database(),
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_data(data, &options)
        .map_err(|error| ImportError::Svg(error.to_string()))?;
    let height_mm = tree.size().height() as f64 * PX_TO_MM;
    let mut groups: Vec<ImportedGroup> = Vec::new();
    collect(tree.root(), &mut |color, paths| {
        let to_mm = |point: Point| Point::new(point.x * PX_TO_MM, height_mm - point.y * PX_TO_MM);
        let paths = paths
            .into_iter()
            .map(|path| Path::new(path.points.into_iter().map(to_mm).collect(), path.closed))
            .collect::<Vec<_>>();
        match groups.iter_mut().find(|group| group.color == color) {
            Some(group) => group.paths.extend(paths),
            None => groups.push(ImportedGroup { color, paths }),
        }
    });
    groups.retain(|group| !group.paths.is_empty());
    if groups.is_empty() {
        return Err(ImportError::Empty);
    }
    Ok(groups)
}

pub(crate) fn collect(group: &usvg::Group, sink: &mut dyn FnMut(Option<[u8; 3]>, Vec<Path>)) {
    for node in group.children() {
        match node {
            usvg::Node::Group(child) => collect(child, sink),
            usvg::Node::Text(text) => collect(text.flattened(), sink),
            usvg::Node::Path(path) => {
                if !path.is_visible() {
                    continue;
                }
                let color = path
                    .stroke()
                    .map(|stroke| stroke.paint())
                    .or_else(|| path.fill().map(|fill| fill.paint()))
                    .and_then(|paint| match paint {
                        usvg::Paint::Color(color) => Some([color.red, color.green, color.blue]),
                        _ => None,
                    });
                let transform = path.abs_transform();
                let (sx, sy) = (transform.sx as f64, transform.sy as f64);
                let scale = (sx.abs().max(sy.abs()) * PX_TO_MM).max(1e-9);
                sink(color, flatten(path.data(), transform, TOLERANCE_MM / scale));
            }
            usvg::Node::Image(_) => {}
        }
    }
}

pub(crate) fn flatten(
    data: &usvg::tiny_skia_path::Path,
    transform: usvg::Transform,
    tolerance: f64,
) -> Vec<Path> {
    let map = |point: usvg::tiny_skia_path::Point| {
        let mut point = point;
        transform.map_point(&mut point);
        Point::new(point.x as f64, point.y as f64)
    };
    let raw = |point: usvg::tiny_skia_path::Point| Point::new(point.x as f64, point.y as f64);
    let mut paths = Vec::new();
    let mut current: Vec<Point> = Vec::new();
    let mut last = Point::default();
    let finish = |current: &mut Vec<Point>, closed: bool, paths: &mut Vec<Path>| {
        if current.len() >= 2 {
            paths.push(Path::new(std::mem::take(current), closed).cleaned(1e-6));
        }
        current.clear();
    };
    for segment in data.segments() {
        match segment {
            PathSegment::MoveTo(point) => {
                finish(&mut current, false, &mut paths);
                last = raw(point);
                current.push(map(point));
            }
            PathSegment::LineTo(point) => {
                last = raw(point);
                current.push(map(point));
            }
            PathSegment::QuadTo(control, point) => {
                for p in flatten_quadratic(last, raw(control), raw(point), tolerance) {
                    current.push(map(usvg::tiny_skia_path::Point::from_xy(
                        p.x as f32, p.y as f32,
                    )));
                }
                last = raw(point);
            }
            PathSegment::CubicTo(c1, c2, point) => {
                for p in flatten_cubic(last, raw(c1), raw(c2), raw(point), tolerance) {
                    current.push(map(usvg::tiny_skia_path::Point::from_xy(
                        p.x as f32, p.y as f32,
                    )));
                }
                last = raw(point);
            }
            PathSegment::Close => {
                finish(&mut current, true, &mut paths);
            }
        }
    }
    finish(&mut current, false, &mut paths);
    paths.retain(|path| path.points.len() >= 2);
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn millimetre_svg_keeps_its_size_and_flips_y() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="50mm" viewBox="0 0 100 50">
            <rect x="10" y="5" width="20" height="10" fill="none" stroke="red"/>
        </svg>"#;
        let groups = import_svg(svg).unwrap();
        assert_eq!(groups[0].color, Some([255, 0, 0]));
        let bounds = groups[0].paths[0].bounds();
        assert!((bounds.min.x - 10.0).abs() < 1e-3);
        assert!((bounds.max.y - 45.0).abs() < 1e-3);
        assert!(groups[0].paths[0].closed);
    }
}
