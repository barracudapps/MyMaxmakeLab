//! Decorations.

use crate::shapes::*;
use crate::tests_projects::text_language;
use crate::{BoardOps, Builder, settings};
use mml_core::geometry::{Path, Point};
use mml_core::layer::ProfileSide;
use mml_core::ornament::{OrnamentLayout, OrnamentParams, OrnamentStyle, generate};
use mml_core::region::{clip_lines, offset_paths, region_of};
use mml_core::{Document, MachineKind};

/// An ornament filling a disc of `radius` around `centre`.
fn ornament_disc(style: OrnamentStyle, seed: u64, centre: Point, radius: f64) -> Vec<Path> {
    let size = radius * 2.0 + 4.0;
    let lines = generate(&OrnamentParams {
        style,
        layout: OrnamentLayout::Tiles,
        width: size,
        height: size,
        cell: (radius / 3.0).max(6.0),
        seed,
    });
    // Closed motifs keep their closing side once clipped.
    let lines: Vec<Path> = translate(&lines, centre.x - size / 2.0, centre.y - size / 2.0)
        .into_iter()
        .map(|mut path| {
            if path.closed {
                if let Some(first) = path.points.first().copied() {
                    path.points.push(first);
                }
                path.closed = false;
            }
            path
        })
        .collect();
    clip_lines(&lines, &region_of(&[circle(centre, radius)]))
}

/// Four round coasters with a different ornament each.
pub fn coasters(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "coasters");
    b.stock_board();
    let styles = [
        OrnamentStyle::Viking,
        OrnamentStyle::Celtic,
        OrnamentStyle::Japanese,
        OrnamentStyle::ArtDeco,
    ];
    let centre = point(0.0, 0.0);
    let parts: Vec<Part> = styles
        .iter()
        .enumerate()
        .map(|(index, style)| {
            let mut lines = ornament_disc(*style, 7 + index as u64, centre, 31.0);
            lines.push(circle(centre, 34.0));
            Part::new(
                b.tf("part.coaster", &[&(index + 1).to_string()]),
                circle(centre, 40.0),
            )
            .with_engraving(lines)
        })
        .collect();
    b.step("s1", Vec::new());
    b.cut_boards(
        boards(parts, 4.0),
        BoardOps {
            engrave: Some(settings::laser_line()),
            engrave_alone: false,
            holes: settings::laser_cut(),
            outline: settings::laser_cut(),
        },
        "board",
    );
    b.step("s2", Vec::new());
    b.finish(MachineKind::Laser)
}

fn star(centre: Point, outer: f64, inner: f64, points: usize) -> Path {
    Path::new(
        (0..points * 2)
            .map(|index| {
                let radius = if index % 2 == 0 { outer } else { inner };
                polar(centre, radius, 90.0 + index as f64 * 180.0 / points as f64)
            })
            .collect(),
        true,
    )
}

/// A snowflake: six arms with side branches, a ring at the top.
fn snowflake(centre: Point) -> Vec<Path> {
    let mut shapes = vec![vec![circle(centre, 9.0)]];
    for arm in 0..6 {
        let angle = 90.0 + arm as f64 * 60.0;
        let tip = polar(centre, 36.0, angle);
        shapes.push(bar(centre, tip, 4.0));
        for (along, length) in [(14.0, 11.0), (25.0, 8.0)] {
            let root = polar(centre, along, angle);
            for side in [-40.0, 40.0] {
                shapes.push(bar(root, polar(root, length, angle + side), 3.0));
            }
        }
    }
    shapes.push(vec![circle(polar(centre, 38.0, 90.0), 5.0)]);
    union(shapes)
}

/// Christmas decorations to hang: star, bauble and snowflake, twice.
pub fn christmas(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "christmas");
    b.stock_board();
    let centre = point(0.0, 0.0);
    let mut parts = Vec::new();
    for copy in 0..2 {
        let shape = star(centre, 42.0, 18.0, 5);
        let inner = offset_paths(std::slice::from_ref(&shape), -4.0);
        // A ring on the top point to hang it.
        let outline = union(vec![vec![shape], vec![circle(point(0.0, 44.0), 5.0)]]);
        parts.push(
            Part::from_area(b.t("part.star"), outline)
                .with_holes(vec![circle(point(0.0, 44.0), 2.0)])
                .with_engraving(inner),
        );
        let ball = union(vec![
            vec![circle(centre, 32.0)],
            vec![rounded_rect(-7.0, 28.0, 14.0, 12.0, 2.0)],
        ]);
        let mut lines = ornament_disc(
            if copy == 0 {
                OrnamentStyle::Celtic
            } else {
                OrnamentStyle::Polynesian
            },
            11 + copy,
            centre,
            25.0,
        );
        lines.push(circle(centre, 27.0));
        parts.push(
            Part::from_area(b.t("part.bauble"), ball)
                .with_holes(vec![circle(point(0.0, 35.0), 2.0)])
                .with_engraving(lines),
        );
        parts.push(
            Part::from_area(b.t("part.snowflake"), snowflake(centre))
                .with_holes(vec![circle(polar(centre, 38.0, 90.0), 2.0)]),
        );
    }
    b.step("s1", Vec::new());
    b.cut_boards(
        boards(parts, 4.0),
        BoardOps {
            engrave: Some(settings::laser_line()),
            engrave_alone: false,
            holes: settings::laser_cut(),
            outline: settings::laser_cut(),
        },
        "board",
    );
    b.step("s2", Vec::new());
    b.finish(MachineKind::Laser)
}

/// Door sign: V-carved word and border, then cut out with a flat end
/// mill.
pub fn door_sign(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "door_sign");
    b.stock(220.0, 100.0);
    let carve = b.layer(b.t("layer.carve"), settings::v_carve(3.0));
    let border = b.layer(b.t("layer.border"), settings::v_line(1.0));
    let holes = b.layer(
        b.t("layer.holes"),
        settings::profile(3.175, 12.3, ProfileSide::Inside, 0),
    );
    let outline = b.layer(
        b.t("layer.outline"),
        settings::profile(3.175, 12.3, ProfileSide::Outside, 4),
    );
    let word = b.t("word");
    let mut letters = text(&word, "Lobster", 34.0, point(120.0, 60.0));
    // Long words are narrowed to stay inside the border.
    let width = mml_core::geometry::bounds_of(&letters).width();
    if width > 150.0 {
        let scale = 150.0 / width;
        let transform = mml_core::Transform::translate(-120.0, -60.0)
            .then(mml_core::Transform::scale(scale, scale))
            .then(mml_core::Transform::translate(120.0, 60.0));
        letters = crate::shapes::transform(&letters, &transform);
    }
    b.add(carve, word, letters);
    b.add(
        border,
        b.t("layer.border"),
        vec![rounded_rect(27.0, 27.0, 186.0, 66.0, 7.0)],
    );
    b.add(
        holes,
        b.t("layer.holes"),
        vec![
            circle(point(36.0, 60.0), 2.5),
            circle(point(204.0, 60.0), 2.5),
        ],
    );
    b.add(
        outline,
        b.t("layer.outline"),
        vec![rounded_rect(20.0, 20.0, 200.0, 80.0, 10.0)],
    );
    b.step("s1", Vec::new());
    b.step("s2", vec![carve, border]);
    b.step("s3", vec![holes, outline]);
    b.step("s4", Vec::new());
    b.finish(MachineKind::Cnc)
}
