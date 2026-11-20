//! Writes the relief program of a procedural surface:
//! `cargo run -p mml_relief --example wave_relief -- out.nc`

use mml_relief::{ReliefParams, Triangle, relief_gcode};

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "wave_relief.nc".into());
    let size = 60;
    let height = |x: usize, y: usize| {
        let (u, v) = (x as f32 / size as f32 - 0.5, y as f32 / size as f32 - 0.5);
        let r = (u * u + v * v).sqrt();
        2.0 + (r * 18.0).cos() * (-r * 3.0).exp() * 2.0
    };
    let mut triangles: Vec<Triangle> = Vec::new();
    for y in 0..size {
        for x in 0..size {
            let corner = |cx: usize, cy: usize| [cx as f32, cy as f32, height(cx, cy)];
            let (a, b, c, d) = (
                corner(x, y),
                corner(x + 1, y),
                corner(x + 1, y + 1),
                corner(x, y + 1),
            );
            triangles.push([a, b, c]);
            triangles.push([a, c, d]);
        }
    }
    let params = ReliefParams {
        origin: [20.0, 20.0],
        width: 60.0,
        depth: 4.0,
        ..ReliefParams::default()
    };
    std::fs::write(&path, relief_gcode(&triangles, &params)).expect("cannot write the program");
}
