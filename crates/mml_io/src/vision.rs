//! Movement consistency check by camera: finds the head in a bed photo
//! (the low-power laser dot or a coloured sticker) and compares where
//! it is seen with where the controller says it is.
//!
//! GRBL drives open-loop steppers, so its position is only what it
//! commanded. The camera is an independent measurement: lost steps, a
//! slipping belt or a blocked axis show up as a growing distance
//! between the two.

use crate::camera::Homography;
use mml_core::geometry::Point;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Marker {
    /// Brightest small spot: the laser at low power, or an LED on the
    /// head.
    LaserDot,
    /// Sticker of this colour on the head; `tolerance` is the largest
    /// RGB distance still accepted.
    Color { rgb: [u8; 3], tolerance: f64 },
}

/// Pixels of the largest blob matching `marker`, at most 2 % of the
/// photo (a larger match is the background, not a marker).
fn marker_blob(photo: &image::RgbaImage, marker: Marker) -> Option<(f64, f64)> {
    let (width, height) = (photo.width() as usize, photo.height() as usize);
    let luminance = |pixel: &image::Rgba<u8>| {
        0.299 * pixel[0] as f64 + 0.587 * pixel[1] as f64 + 0.114 * pixel[2] as f64
    };
    let mask: Vec<bool> = match marker {
        Marker::LaserDot => {
            let brightest = photo.pixels().map(luminance).fold(0.0, f64::max);
            if brightest < 200.0 {
                return None;
            }
            let threshold = brightest - 12.0;
            photo
                .pixels()
                .map(|pixel| luminance(pixel) >= threshold)
                .collect()
        }
        Marker::Color { rgb, tolerance } => photo
            .pixels()
            .map(|pixel| {
                let distance: f64 = (0..3)
                    .map(|channel| (pixel[channel] as f64 - rgb[channel] as f64).powi(2))
                    .sum::<f64>()
                    .sqrt();
                distance <= tolerance
            })
            .collect(),
    };
    let mut seen = vec![false; mask.len()];
    let mut best: Option<(usize, f64, f64)> = None;
    let mut stack = Vec::new();
    for start in 0..mask.len() {
        if !mask[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        let (mut count, mut sum_x, mut sum_y) = (0usize, 0.0, 0.0);
        while let Some(index) = stack.pop() {
            let (x, y) = (index % width, index / width);
            count += 1;
            sum_x += x as f64;
            sum_y += y as f64;
            let neighbours = [
                (x > 0).then(|| index - 1),
                (x + 1 < width).then(|| index + 1),
                (y > 0).then(|| index - width),
                (y + 1 < height).then(|| index + width),
            ];
            for next in neighbours.into_iter().flatten() {
                if mask[next] && !seen[next] {
                    seen[next] = true;
                    stack.push(next);
                }
            }
        }
        if best.is_none_or(|(largest, ..)| count > largest) {
            best = Some((count, sum_x, sum_y));
        }
    }
    let (count, sum_x, sum_y) = best?;
    let minimum = if matches!(marker, Marker::LaserDot) {
        1
    } else {
        6
    };
    if count < minimum || count > width * height / 50 {
        return None;
    }
    Some((sum_x / count as f64 + 0.5, sum_y / count as f64 + 0.5))
}

/// Head position on the bed in millimetres, from the photo and the four
/// calibration corners (front left, front right, back right, back left).
pub fn locate(
    photo: &image::RgbaImage,
    image_corners: [Point; 4],
    bed: (f64, f64),
    marker: Marker,
) -> Option<Point> {
    let (x, y) = marker_blob(photo, marker)?;
    let bed_corners = [
        Point::new(0.0, 0.0),
        Point::new(bed.0, 0.0),
        Point::new(bed.0, bed.1),
        Point::new(0.0, bed.1),
    ];
    let to_bed = Homography::from_points(image_corners, bed_corners)?;
    let point = to_bed.apply(Point::new(x, y));
    let inside = (-5.0..=bed.0 + 5.0).contains(&point.x) && (-5.0..=bed.1 + 5.0).contains(&point.y);
    inside.then_some(point)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Verdict {
    /// Still learning the constant offset between marker and position.
    Learning,
    Consistent {
        error: f64,
    },
    /// Seen too far from the commanded path several times in a row.
    Mismatch {
        error: f64,
    },
}

/// Compares camera sightings with the positions the controller
/// reported. A snapshot is taken somewhere between its request and its
/// arrival, so it is matched against every reported position in that
/// window. The marker is not exactly at the tool and the camera adds
/// parallax: the constant offset is learned from the first sightings.
#[derive(Debug, Clone)]
pub struct MotionCheck {
    /// Distance in mm above which a sighting is wrong.
    pub tolerance: f64,
    history: Vec<(f64, Point)>,
    learning: Vec<Point>,
    offset: Option<Point>,
    strikes: u32,
}

const LEARNING_SAMPLES: usize = 5;
const STRIKES: u32 = 3;

impl MotionCheck {
    pub fn new(tolerance: f64) -> Self {
        Self {
            tolerance,
            history: Vec::new(),
            learning: Vec::new(),
            offset: None,
            strikes: 0,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new(self.tolerance);
    }

    /// Position reported by the controller at `time` seconds.
    pub fn commanded(&mut self, time: f64, position: Point) {
        // A still head: stretch the last sample instead of piling up.
        if let [.., (_, previous), (last_time, last)] = self.history.as_mut_slice()
            && previous.distance(position) < 1e-6
            && last.distance(position) < 1e-6
        {
            *last_time = time;
            return;
        }
        self.history.push((time, position));
        let oldest = time - 20.0;
        let stale = self
            .history
            .iter()
            .take_while(|(at, _)| *at < oldest)
            .count();
        // Keep one sample before the window so a still head stays known.
        self.history.drain(..stale.saturating_sub(1));
    }

    fn candidates(&self, from: f64, to: f64) -> impl Iterator<Item = Point> + '_ {
        let before = self
            .history
            .iter()
            .rev()
            .find(|(at, _)| *at <= from)
            .map(|(_, point)| *point);
        before.into_iter().chain(
            self.history
                .iter()
                .filter(move |(at, _)| *at > from && *at <= to)
                .map(|(_, point)| *point),
        )
    }

    /// A sighting of a snapshot requested at `from` and received at
    /// `to` seconds.
    pub fn observed(&mut self, from: f64, to: f64, seen: Point) -> Verdict {
        let Some(offset) = self.offset else {
            let middle = (from + to) / 2.0;
            let Some(reference) = self
                .history
                .iter()
                .min_by(|a, b| (a.0 - middle).abs().total_cmp(&(b.0 - middle).abs()))
                .map(|(_, point)| *point)
            else {
                return Verdict::Learning;
            };
            self.learning
                .push(Point::new(seen.x - reference.x, seen.y - reference.y));
            if self.learning.len() >= LEARNING_SAMPLES {
                let median = |values: &mut Vec<f64>| {
                    values.sort_by(f64::total_cmp);
                    values[values.len() / 2]
                };
                let mut xs: Vec<f64> = self.learning.iter().map(|point| point.x).collect();
                let mut ys: Vec<f64> = self.learning.iter().map(|point| point.y).collect();
                self.offset = Some(Point::new(median(&mut xs), median(&mut ys)));
            }
            return Verdict::Learning;
        };
        let corrected = Point::new(seen.x - offset.x, seen.y - offset.y);
        let error = self
            .candidates(from, to)
            .map(|point| point.distance(corrected))
            .fold(f64::INFINITY, f64::min);
        if !error.is_finite() {
            return Verdict::Learning;
        }
        if error > self.tolerance {
            self.strikes += 1;
        } else {
            self.strikes = 0;
        }
        if self.strikes >= STRIKES {
            Verdict::Mismatch { error }
        } else {
            Verdict::Consistent { error }
        }
    }
}

/// The piece of stock lying on the bed, found on a rectified picture
/// (`pixels_per_mm`, row 0 at the back): the bed colour is taken from
/// the border of the picture, pixels clearly different from it form the
/// stock, and the largest such blob gives its outline in bed
/// millimetres. `None` when nothing stands out.
pub fn detect_stock(
    picture: &image::RgbaImage,
    pixels_per_mm: f64,
) -> Option<mml_core::geometry::Rect> {
    let (width, height) = (picture.width() as usize, picture.height() as usize);
    if width < 8 || height < 8 {
        return None;
    }
    // Median colour of the border: the bed around the stock.
    let mut border: Vec<[u8; 3]> = Vec::new();
    for x in 0..width {
        for y in [0, height - 1] {
            let pixel = picture.get_pixel(x as u32, y as u32);
            if pixel[3] > 0 {
                border.push([pixel[0], pixel[1], pixel[2]]);
            }
        }
    }
    for y in 0..height {
        for x in [0, width - 1] {
            let pixel = picture.get_pixel(x as u32, y as u32);
            if pixel[3] > 0 {
                border.push([pixel[0], pixel[1], pixel[2]]);
            }
        }
    }
    if border.is_empty() {
        return None;
    }
    let median = |channel: usize| {
        let mut values: Vec<u8> = border.iter().map(|color| color[channel]).collect();
        values.sort_unstable();
        values[values.len() / 2] as f64
    };
    let bed = [median(0), median(1), median(2)];
    let mask: Vec<bool> = picture
        .pixels()
        .map(|pixel| {
            pixel[3] > 0
                && ((pixel[0] as f64 - bed[0]).powi(2)
                    + (pixel[1] as f64 - bed[1]).powi(2)
                    + (pixel[2] as f64 - bed[2]).powi(2))
                .sqrt()
                    > 45.0
        })
        .collect();
    // Largest connected blob.
    let mut seen = vec![false; mask.len()];
    let mut best: Option<(usize, [usize; 4])> = None;
    let mut stack = Vec::new();
    for start in 0..mask.len() {
        if !mask[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        let (mut count, mut bounds) = (0usize, [usize::MAX, usize::MAX, 0, 0]);
        while let Some(index) = stack.pop() {
            let (x, y) = (index % width, index / width);
            count += 1;
            bounds = [
                bounds[0].min(x),
                bounds[1].min(y),
                bounds[2].max(x),
                bounds[3].max(y),
            ];
            let neighbours = [
                (x > 0).then(|| index - 1),
                (x + 1 < width).then(|| index + 1),
                (y > 0).then(|| index - width),
                (y + 1 < height).then(|| index + width),
            ];
            for next in neighbours.into_iter().flatten() {
                if mask[next] && !seen[next] {
                    seen[next] = true;
                    stack.push(next);
                }
            }
        }
        if best.is_none_or(|(largest, _)| count > largest) {
            best = Some((count, bounds));
        }
    }
    let (count, [x0, y0, x1, y1]) = best?;
    // Noise, or the whole picture: not a piece of stock.
    if count < (width * height) / 400 || count > (width * height) * 9 / 10 {
        return None;
    }
    let to_mm = |pixels: usize| pixels as f64 / pixels_per_mm;
    let bed_height = to_mm(height);
    Some(mml_core::geometry::Rect::from_points(
        Point::new(to_mm(x0), bed_height - to_mm(y1 + 1)),
        Point::new(to_mm(x1 + 1), bed_height - to_mm(y0)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo_with(spot: (u32, u32), color: [u8; 3], radius: u32) -> image::RgbaImage {
        let mut photo = image::RgbaImage::from_pixel(200, 100, image::Rgba([90, 80, 70, 255]));
        for y in spot.1 - radius..=spot.1 + radius {
            for x in spot.0 - radius..=spot.0 + radius {
                photo.put_pixel(x, y, image::Rgba([color[0], color[1], color[2], 255]));
            }
        }
        photo
    }

    fn corners() -> [Point; 4] {
        // Photo of a 400 x 200 mm bed, 0.5 px per mm, back at the top.
        [
            Point::new(0.0, 100.0),
            Point::new(200.0, 100.0),
            Point::new(200.0, 0.0),
            Point::new(0.0, 0.0),
        ]
    }

    #[test]
    fn the_laser_dot_and_a_sticker_are_found_in_bed_millimetres() {
        let photo = photo_with((50, 25), [255, 250, 240], 1);
        let point = locate(&photo, corners(), (400.0, 200.0), Marker::LaserDot).unwrap();
        assert!(
            (point.x - 101.0).abs() < 2.5 && (point.y - 149.0).abs() < 2.5,
            "{point:?}"
        );
        let photo = photo_with((150, 75), [20, 200, 40], 3);
        let marker = Marker::Color {
            rgb: [30, 190, 50],
            tolerance: 40.0,
        };
        let point = locate(&photo, corners(), (400.0, 200.0), marker).unwrap();
        assert!(
            (point.x - 301.0).abs() < 2.5 && (point.y - 49.0).abs() < 2.5,
            "{point:?}"
        );
        let dull = photo_with((50, 25), [120, 110, 100], 1);
        assert_eq!(
            locate(&dull, corners(), (400.0, 200.0), Marker::LaserDot),
            None
        );
    }

    #[test]
    fn a_head_that_lost_steps_is_reported_after_a_few_sightings() {
        // Status at 5 Hz, 10 mm/s, one snapshot per second taking 0.3 s.
        let mut check = MotionCheck::new(3.0);
        let marker_offset = Point::new(12.0, -4.0);
        let mut verdicts = Vec::new();
        for tick in 0..=70 {
            let time = tick as f64 * 0.2;
            let commanded = Point::new(2.0 * tick as f64, 50.0);
            check.commanded(time, commanded);
            if tick % 5 == 0 && tick > 0 {
                // From 9 s the real head is 10 mm behind in X.
                let slip = if time >= 9.0 { 10.0 } else { 0.4 };
                let seen = Point::new(
                    commanded.x - 1.0 + marker_offset.x - slip,
                    commanded.y + marker_offset.y,
                );
                verdicts.push(check.observed(time - 0.3, time, seen));
            }
        }
        assert!(
            verdicts[..5]
                .iter()
                .all(|verdict| *verdict == Verdict::Learning)
        );
        assert!(matches!(verdicts[6], Verdict::Consistent { error } if error < 1.5));
        assert!(matches!(verdicts[8], Verdict::Consistent { .. }));
        assert!(matches!(verdicts[10], Verdict::Mismatch { error } if error > 5.0));
    }

    #[test]
    fn a_snapshot_taken_during_a_move_matches_any_point_of_its_window() {
        let mut check = MotionCheck::new(2.0);
        for step in 0..5 {
            check.commanded(step as f64, Point::new(0.0, 0.0));
            check.observed(step as f64, step as f64 + 0.1, Point::new(0.0, 0.0));
        }
        check.commanded(5.0, Point::new(0.0, 0.0));
        check.commanded(5.2, Point::new(10.0, 0.0));
        check.commanded(5.4, Point::new(20.0, 0.0));
        let verdict = check.observed(5.1, 5.5, Point::new(10.5, 0.0));
        assert!(matches!(verdict, Verdict::Consistent { error } if error < 1.0));
    }

    #[test]
    fn a_board_on_the_bed_is_found_where_it_lies() {
        // Bed 200 x 100 mm at 2 px/mm, dark grey with a little noise; a
        // light board from X 50 to 130 mm and Y 20 to 70 mm.
        let mut picture = image::RgbaImage::from_fn(400, 200, |x, y| {
            let noise = ((x * 7 + y * 13) % 9) as u8;
            image::Rgba([60 + noise, 62 + noise, 66 + noise, 255])
        });
        for y in 60..160 {
            for x in 100..260 {
                picture.put_pixel(x, y, image::Rgba([210, 180, 140, 255]));
            }
        }
        let stock = detect_stock(&picture, 2.0).unwrap();
        assert!(
            (stock.min.x - 50.0).abs() < 0.6 && (stock.max.x - 130.0).abs() < 0.6,
            "{stock:?}"
        );
        assert!(
            (stock.min.y - 20.0).abs() < 0.6 && (stock.max.y - 70.0).abs() < 0.6,
            "{stock:?}"
        );
        let empty = image::RgbaImage::from_pixel(400, 200, image::Rgba([60, 62, 66, 255]));
        assert_eq!(detect_stock(&empty, 2.0), None);
    }
}
