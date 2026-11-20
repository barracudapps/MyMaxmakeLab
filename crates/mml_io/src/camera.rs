//! Camera overlay: a photo of the bed (phone, action camera, webcam
//! snapshot) is rectified with a homography computed from the four bed
//! corners clicked on it, so it can be shown under the design at true
//! scale.

use mml_core::geometry::Point;

/// Projective transform `p' = H p` in homogeneous coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Homography(pub [f64; 9]);

impl Homography {
    /// Maps the four `from` points onto the four `to` points.
    pub fn from_points(from: [Point; 4], to: [Point; 4]) -> Option<Self> {
        // Eight equations, eight unknowns (h33 = 1), Gaussian elimination.
        let mut matrix = [[0.0f64; 9]; 8];
        for index in 0..4 {
            let (x, y) = (from[index].x, from[index].y);
            let (u, v) = (to[index].x, to[index].y);
            matrix[2 * index] = [x, y, 1.0, 0.0, 0.0, 0.0, -u * x, -u * y, u];
            matrix[2 * index + 1] = [0.0, 0.0, 0.0, x, y, 1.0, -v * x, -v * y, v];
        }
        for column in 0..8 {
            let pivot = (column..8).max_by(|a, b| {
                matrix[*a][column]
                    .abs()
                    .total_cmp(&matrix[*b][column].abs())
            })?;
            if matrix[pivot][column].abs() < 1e-12 {
                return None;
            }
            matrix.swap(column, pivot);
            let pivot_row = matrix[column];
            for (index, row) in matrix.iter_mut().enumerate() {
                if index != column {
                    let factor = row[column] / pivot_row[column];
                    for (cell, pivot) in row.iter_mut().zip(pivot_row).skip(column) {
                        *cell -= factor * pivot;
                    }
                }
            }
        }
        let mut h = [0.0; 9];
        for row in 0..8 {
            h[row] = matrix[row][8] / matrix[row][row];
        }
        h[8] = 1.0;
        Some(Self(h))
    }

    pub fn apply(&self, point: Point) -> Point {
        let h = &self.0;
        let w = h[6] * point.x + h[7] * point.y + h[8];
        Point::new(
            (h[0] * point.x + h[1] * point.y + h[2]) / w,
            (h[3] * point.x + h[4] * point.y + h[5]) / w,
        )
    }
}

/// Bed-space picture, `pixels_per_mm` resolution, row 0 at the back of
/// the bed (top of the screen). Pixels outside the photo are clear.
pub fn rectify(
    photo: &image::RgbaImage,
    image_corners: [Point; 4],
    bed: (f64, f64),
    pixels_per_mm: f64,
) -> Option<image::RgbaImage> {
    let (width, height) = (
        (bed.0 * pixels_per_mm).round().max(1.0) as u32,
        (bed.1 * pixels_per_mm).round().max(1.0) as u32,
    );
    let bed_corners = [
        Point::new(0.0, 0.0),
        Point::new(bed.0, 0.0),
        Point::new(bed.0, bed.1),
        Point::new(0.0, bed.1),
    ];
    let bed_to_image = Homography::from_points(bed_corners, image_corners)?;
    let mut out = image::RgbaImage::new(width, height);
    for row in 0..height {
        for column in 0..width {
            let bed_point = Point::new(
                (column as f64 + 0.5) / pixels_per_mm,
                bed.1 - (row as f64 + 0.5) / pixels_per_mm,
            );
            let source = bed_to_image.apply(bed_point);
            if source.x >= 0.0
                && source.y >= 0.0
                && source.x < photo.width() as f64 - 1.0
                && source.y < photo.height() as f64 - 1.0
            {
                out.put_pixel(
                    column,
                    row,
                    *photo.get_pixel(source.x as u32, source.y as u32),
                );
            }
        }
    }
    Some(out)
}

/// Downloads a snapshot from a plain HTTP address (phone webcam apps,
/// IP cameras). HTTPS is not supported.
pub fn fetch_snapshot(url: &str) -> Result<Vec<u8>, String> {
    use std::io::{Read, Write};
    let rest = url
        .strip_prefix("http://")
        .ok_or("only http:// addresses are supported")?;
    let (host, path) = rest
        .split_once('/')
        .map_or((rest, "/".to_owned()), |(host, path)| {
            (host, format!("/{path}"))
        });
    let address = if host.contains(':') {
        host.to_owned()
    } else {
        format!("{host}:80")
    };
    let mut stream = std::net::TcpStream::connect(&address).map_err(|error| error.to_string())?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .map_err(|error| error.to_string())?;
    write!(
        stream,
        "GET {path} HTTP/1.0\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    )
    .map_err(|error| error.to_string())?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|error| error.to_string())?;
    let split = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or("invalid answer")?;
    let head = String::from_utf8_lossy(&response[..split]);
    if !head
        .lines()
        .next()
        .is_some_and(|status| status.contains(" 200"))
    {
        return Err(head.lines().next().unwrap_or("no answer").to_owned());
    }
    Ok(response[split + 4..].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn homography_maps_the_four_corners_and_keeps_lines_straight() {
        let from = [
            Point::new(10.0, 300.0),
            Point::new(390.0, 320.0),
            Point::new(360.0, 40.0),
            Point::new(30.0, 20.0),
        ];
        let to = [
            Point::new(0.0, 0.0),
            Point::new(400.0, 0.0),
            Point::new(400.0, 300.0),
            Point::new(0.0, 300.0),
        ];
        let homography = Homography::from_points(from, to).unwrap();
        for (a, b) in from.iter().zip(&to) {
            assert!(homography.apply(*a).distance(*b) < 1e-6);
        }
        let middle = homography.apply(from[0].lerp(from[1], 0.5));
        assert!(
            middle.y.abs() < 1e-6,
            "a point of the front edge stays on the front edge"
        );
        assert!(Homography::from_points([Point::default(); 4], to).is_none());
    }

    #[test]
    fn rectified_photo_puts_the_front_left_corner_at_the_bottom_left() {
        let mut photo = image::RgbaImage::from_pixel(100, 100, image::Rgba([255, 255, 255, 255]));
        for y in 80..100 {
            for x in 0..20 {
                photo.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
            }
        }
        let corners = [
            Point::new(0.0, 100.0),
            Point::new(100.0, 100.0),
            Point::new(100.0, 0.0),
            Point::new(0.0, 0.0),
        ];
        let bed = rectify(&photo, corners, (50.0, 50.0), 2.0).unwrap();
        assert_eq!(bed.get_pixel(2, 97).0, [255, 0, 0, 255]);
        assert_eq!(bed.get_pixel(97, 2).0, [255, 255, 255, 255]);
    }
}
