# Changelog

What changed in each version, for the people who use MyMaxmakeLab.
The application updates itself from GitHub releases; each release shows
its section of this file. Nothing has been validated on real hardware
yet: GRBL is tested against simulated controllers.

## [Unreleased]

## [0.1.11] - 2026-11-21

### Fixed
- When GitHub cannot be reached, *Check for updates* says so instead of
  reporting the copy as up to date.

### Added
- **Puzzle on lines**: a children's puzzle whose pieces follow the heavy
  lines of a drawing or a picture; thin lines stay as a drawing to
  engrave, dark patches of colour become pieces, small pieces join a
  neighbour, with an optional tray to fit the pieces in.
- When a HiMill is connected on another profile, the connection card
  offers the HiMill D1S profile in one click.

### Changed
- Puzzles inside a shape no longer leave tiny pieces along the border:
  the grid is placed where its smallest piece is largest, and pieces
  still under 30 % of a normal one join their neighbour.
- Updates are checked on the public release page instead of the GitHub
  API, whose hourly quota is shared by everything on the same
  connection; the API is only a second way.
- Choosing a machine profile keeps the bed size the controller reports.
- The *Start the job* button only shows when there is something to make.

## [0.1.10] - 2026-11-21

### Fixed
- **Jogging after homing.** Jog limits assumed positions from 0 to the
  bed size, while GRBL machine space usually runs from -travel to 0:
  right after homing, a jog towards the front could go backwards and a
  short jog to the right could cross the whole bed. Limits now come from
  the position read right after homing, the homing directions (`$23`)
  and the travel (`$130`-`$132`); a jog is only ever shortened.

### Added
- **Double-click homing**: with hold-to-run homing, a double click lets
  homing finish by itself (Escape or Reset still stop it).
- **Jigsaw puzzle inside any shape**: select a closed shape, then
  *Puzzle…*; its outline becomes the edge of the puzzle.

## [0.1.9] - 2026-11-21

### Added
- **Maxmake HiMill D1S profile**: 280 × 200 × 80 mm, home at the rear
  left of the moving table, spindle scale 13000, programs framed by
  `[STREAM_START]` / `[STREAM_END]` like MaxmakeLAB does.
- **The bed follows the connected machine**: its travel (`$130`-`$132`)
  sets the bed size and Z travel (can be turned off).

### Fixed
- The HiMill welcome line (`GCARVIN`) is recognised, and controllers that
  print nothing at connection are greeted by their first status: their
  settings are read, which the homing diagnosis and the bed size need.
- The end G-code runs last, after the return of the tool; a resumed job
  sends the firmware framing commands again.

## [0.1.8] - 2026-11-21

### Added
- **Homing diagnosis**: after a failed homing, the cause and what to do
  (homing off with `$22=0`, switch not found, pull-off failed, button
  released, too long), shown before any attempt when homing is off.
- **Durations learned from past jobs** on the same machine (estimate,
  remaining time and cost).
- Floating *Start the job* button in the Run step.

### Changed
- The Run step follows the order of the work: jog, origin and
  measurement before the job card.
- GRBL error messages in French, German, Italian and Spanish.

## [0.1.7] - 2026-11-21

### Added
- **Import from your software**: Autodesk Fusion tool libraries,
  LightBurn and MaxmakeLAB material libraries found in their usual
  folders, kept up to date at start.
- **Nesting** on a sheet by the real outline of the parts, with copies.
- **Keyboard jog** with five precision levels (Micro 0.01 to Rapid
  50 mm), Page Up / Down for Z, Shift / Alt for one level more or less.
- **Spindle on / off next to the jog pad**, with jog speeds capped
  while it turns.

### Fixed
- Holding an arrow key queued jogs that went on after release; letting
  go now always stops the head.
- Homing stopped right after starting when its button moved.

## [0.1.6] - 2026-11-21

### Added
- **Resume an interrupted job** from the move the head was really on.
- **Kerf compensation** for laser cuts.
- **Combine shapes**: union, difference, intersection, exclusion.

## [0.1.5] - 2026-11-21

### Added
- Round box with a living-hinge wall.

### Fixed
- The application icon everywhere (Dock, Windows executable, Linux).
- The start panel can be closed.
- No "up to date" while a release is still being published.

## [0.1.4] - 2026-11-21

### Added
- Start panel, layer bar, next-step button, shape library.
- Gears, QR codes, barcodes, circular arrays, contour offsets.
- Stylus drawing page with brushes, eraser and live sync.
- Quick laser setup for paper and board.
- Scan offset for fast bidirectional engraving; centre finding of holes
  and blocks; laser offset on laser and spindle machines.
- Resize without keeping proportions.

## [0.1.3] - 2026-11-21

### Added
- Measuring the stock by hand, without a 3D probe.
- Hold-to-run, gentle and supervised homing.

## [0.1.2] - 2026-11-20

### Added
- Command palette, laser test grid, following a job from a phone,
  stock found on a picture of the bed.

## [0.1.1] - 2026-11-20

### Added
- French user manual, German, Italian and Spanish translations.
- Measured tool length and collision check before Start, tool
  suggestion, gentle milling, statistics and job costs.

## [0.1.0] - 2026-11-20

First release: design, preparation and machine control for GRBL lasers
and milling machines.

[Unreleased]: https://github.com/barracudapps/MyMaxmakeLab/compare/v0.1.11...HEAD
[0.1.11]: https://github.com/barracudapps/MyMaxmakeLab/compare/v0.1.10...v0.1.11
[0.1.10]: https://github.com/barracudapps/MyMaxmakeLab/compare/v0.1.9...v0.1.10
[0.1.9]: https://github.com/barracudapps/MyMaxmakeLab/compare/v0.1.8...v0.1.9
[0.1.8]: https://github.com/barracudapps/MyMaxmakeLab/compare/v0.1.7...v0.1.8
[0.1.7]: https://github.com/barracudapps/MyMaxmakeLab/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/barracudapps/MyMaxmakeLab/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/barracudapps/MyMaxmakeLab/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/barracudapps/MyMaxmakeLab/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/barracudapps/MyMaxmakeLab/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/barracudapps/MyMaxmakeLab/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/barracudapps/MyMaxmakeLab/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/barracudapps/MyMaxmakeLab/releases/tag/v0.1.0
