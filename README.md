# MyMaxmakeLab

Fast, portable laser and CNC workbench for GRBL machines (Marxmake HiMill,
iLam laser and any GRBL 1.1 controller). It replaces MaxmakeLAB with a
simpler three-step workflow: **Design → Prepare → Run**.

## Features

- Design canvas: rectangles, ellipses, polygons, text-free vector import
  (SVG, DXF), raster images, move / scale / rotate, align, undo / redo.
- Layers carry the operation: laser line, laser fill, laser image, CNC
  profile, CNC pocket. One click applies a material preset.
- Laser: vector cut/engrave, scanline fill with overscan, image engraving
  with threshold, ordered, Floyd–Steinberg, Atkinson, Jarvis and Stucki
  dithering.
- CNC: inside / outside / on-line profiles, pocketing, multi-pass depth,
  tabs, safe Z and plunge feed.
- G-code: parser, writer, bounds, length and duration estimate, 2D preview.
- Machine: GRBL over USB serial or Wi-Fi (TCP), live status, jog, homing,
  unlock, zeroing, console, macros, streaming with pause / resume / stop,
  human-readable alarms and errors.
- Material library: built-in presets, LightBurn `.clb` import (the format
  used by the MaxmakeLAB material library).
- English and French interface.

## Build

```bash
cargo run --release -p mml_app
cargo test --workspace
```

Linux needs `libudev-dev` for the serial port and the usual X11/Wayland
development packages for the window.

## Layout

| Crate           | Role                                               |
|-----------------|----------------------------------------------------|
| `mml_core`      | Geometry, document, layers, project file           |
| `mml_gcode`     | G-code parser, writer, job analysis                |
| `mml_laser`     | Laser toolpaths (line, fill, image dithering)      |
| `mml_cnc`       | CNC toolpaths (profile, pocket, tabs)              |
| `mml_io`        | SVG, DXF and raster import                         |
| `mml_materials` | Material presets and LightBurn library import      |
| `mml_device`    | GRBL protocol, serial and TCP transports           |
| `mml_app`       | Desktop application (egui)                         |

See `CONTRIBUTING.md` for the commit rules.
