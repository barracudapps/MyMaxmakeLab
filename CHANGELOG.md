# Changelog

What changed in each version, for the people who use MyMaxmakeLab.
The application updates itself from GitHub releases; each release shows
its section of this file. Nothing has been validated on real hardware
yet: GRBL is tested against simulated controllers.

## [Unreleased]

## [0.1.25] - 2026-11-26

### Fixed
- Before homing in the application, the travel check judges each axis
  by the sign of its own position. It used to take the space of every
  axis from X and Y together, and refused a laser job on the HiMill
  (X positive, Y negative) when the homing was done before connecting.

## [0.1.24] - 2026-11-26

### Added
- **Jobs relative to the head in X, Y and Z**: with the "Head position
  marks…" placement, tick "Z too" and the current head height becomes
  Z0, so the job runs entirely from where the head is (tool on the top
  of the material, or laser at its focus height).

### Fixed
- A laser job on a milling machine with a laser module (HiMill) no
  longer lifts Z to the safe height at the end nor before a pause: the
  head stays at its focus height, and the Z travel check no longer
  refuses the job.
- The travel checks and jog limits correct the homing direction of an
  axis from where the head has been seen since homing: the HiMill in
  laser mode reports a `$23` that does not match its coordinates, and
  jobs placed at the head were refused on X.
- The machine profile turns its laser mode on when the connected
  controller is in laser mode (`$32` = 1), so laser layers are no longer
  reported as "not in laser mode". It never turns it off: the HiMill
  keeps `$32` at 0, and the box ticked in the machine settings stays.

## [0.1.23] - 2026-11-26

### Changed
- **The "Machine zero" placement becomes "On the bed"**: once the
  machine has homed, the X/Y work zero is set on the front left corner
  of the travel, so the drawing is cut where the bed shows it and the
  head cross shows its real place (it used to sit at the bed origin
  while the head was at home, rear left on the HiMill). The X/Y zero
  buttons are greyed in this placement; the Z zero stays free.

### Fixed
- A laser job no longer refuses to start because of the Z axis: a
  program that never commands Z leaves the head at its focus height,
  so its Z travel is not checked before starting nor watched during
  the job.
- The travel check before starting follows the position read right
  after homing and the homing directions (`$23`). It used to guess the
  coordinate space from the sign of the head position: on a machine
  that reports its home at 0, like the HiMill in laser mode, a drawing
  out of reach passed the check.

## [0.1.22] - 2026-11-26

### Added
- **Three more complex 3D projects**, all resizable: a Ferris wheel
  turned by a crank (two spoked wheels tied by tabbed rungs, eight
  gondolas that hang level on pins, A-frame stand), a windmill whose
  lattice sails turn on an axle through both gables (stone-engraved
  finger-jointed tower, shingled roof), and a castle (crenellated
  curtain walls round a courtyard, four corner towers with arrow slits,
  a drawbridge on a pin hinge raised by two strings, a portcullis
  sliding in grooves behind the gate).

## [0.1.21] - 2026-11-26

### Added
- **Three more complex 3D projects**, all resizable: a T. rex skeleton
  (spine and toothed skull, eight ribs slotted from below, a pelvis
  carrying two legs seen from the side, little arms and a stand), a sea
  automaton (turn the crank: a three-cam shaft lifts a bobbing boat, a
  diving whale and a jumping fish between bands of waves) and a working
  trebuchet (A-frames, an arm on an axle with spacers, a hanging boxed
  counterweight and a sling to make from string and cloth).

## [0.1.20] - 2026-11-25

### Added
- **Three bigger projects over several boards**: a book nook (an alley
  at night in a finger-jointed box to slide between books, with house
  fronts in two layers, a bridge whose tabs go through the walls, a
  cobbled floor and string lights), a pair of bookends with a forest and
  a stag cut out, and a code box (cryptex): four letter dials on the
  lid turn cams underneath, and a sliding bolt frees the lid only when
  the four notches face its fingers; the code is chosen when gluing.
  All three can be resized to your plywood.
- The 3D relief operation can carve a **rounded bottom**: the background
  goes down from the outer contour in a quarter circle, into bowls.

### Changed
- The awale's pits and stores are carved as rounded bowls with a 6 mm
  ball end mill, so the seeds come out with a fingertip; the outline is
  cut in a step of its own with the flat end mill.
- V-carving and relief use a finer distance measure (5 × 5 chamfer):
  round shapes no longer come out slightly faceted.

## [0.1.19] - 2026-11-25

### Added
- **Resize the laser-cut puzzles and layered decorations to your
  stock**: opening one of these samples asks for the plywood thickness,
  the board (project size, A4, A5, A6 or custom) and the size. The
  drawing is scaled while the slots stay as wide as the plywood is
  thick and the pin holes stay 2 mm; the parts are shared out on boards
  of that size, the largest size that fits is offered, and a board
  larger than the machine's work area is refused. The guide gives the
  thickness, the number of boards and the scale used.

### Fixed
- The 3D fir tree's large tree came out with the left half of its foot
  as a loose piece (the trunk outline missed a corner; the firs of the
  layered decorations had a crooked trunk for the same reason).
- Layered decorations and the newer 3D puzzles now show the grey board
  their guide asks to place the plywood on.

## [0.1.18] - 2026-11-25

### Added
- **Layered decorations**: five sample projects of 3 mm plywood sheets
  glued in a stack, each opening onto the ones behind: a moose framing
  mountains, firs and two stags, a bear at sunset, an owl in the
  moonlight, a round Christmas bauble with a leaping deer, and a
  stepped mandala. Every sheet keeps a frame, pin holes line them up
  and hidden engraved numbers give the order.
- **Three more 3D puzzles**: a bear and a wolf standing on their four
  legs (two full side views held apart by rounded spacers) and a 3D fir
  tree with its star.
- **Five more layered decorations**: the sea bed inside a whale, a
  penguin on the pack ice under the northern lights, a city at night in
  an arched window, a porthole on space and a Halloween pumpkin.
- **Four more 3D puzzles**: a rhinoceros, a tortoise, a rocket standing
  on its four fins and an airplane whose wing slides through the
  fuselage.
- The animal outlines come from public domain (CC0) drawings from
  Openclipart, credited in `crates/mml_samples/art/CREDITS.md`.

## [0.1.17] - 2026-11-23

### Added
- **Relief carving from drawings**: a new CNC operation, "3D relief". In
  its layer, an outer contour is the carved area and the shapes inside
  it stand up as rounded motifs above the background (edge rounding,
  depth, ball or flat cutter, roughing with the same tool). The cutter
  never leaves the outer contour. Pictures on the layer are carved as
  height maps.
- **Relief medallions** sample project: a sun, the triple moon, a
  six-petal rosette and the tree of life carved in relief in hardwood
  with a 3.175 mm ball end mill, then cut out.

## [0.1.16] - 2026-11-23

### Added
- **Two more 3D puzzles** to slot together from 3 mm plywood: a
  triceratops (body, frill, two pairs of legs) and a galleon (hull and
  masts in one part, square sails slid onto the masts, two cradles).

## [0.1.15] - 2026-11-22

### Added
- **Five more sample projects**: two 3D puzzles slotted together from
  3 mm plywood (a dragon with engraved scales, a globe of rings and
  ribs), and esoteric objects: sacred geometry medallions (flower of
  life, Metatron's cube, tree of life), rune stones V-carved with the 24
  runes of the elder futhark, and a talking board whose words follow
  the language of the guide.

## [0.1.14] - 2026-11-22

### Added
- **Six more sample projects**, four of them for a router with a laser
  head (CNC + laser): an alignment test whose milled scales and laser
  verniers read the laser offset to 0.1 mm, an awale board, a valet
  tray with a compass rose burnt at the bottom of its pocket, spinning
  tops with optical illusions (Benham's disc) and a horizontal sundial;
  plus a CNC fit test (notches with dog bones, dowel holes). Their
  guides walk through each head change, laser or milling mode, homing
  and focus, the board never moving.

## [0.1.13] - 2026-11-22

### Added
- **Sample projects** (*File › Sample projects…*): CNC calibration part,
  laser settings test, marble maze, Tower of Hanoi, pentominoes, fifteen
  puzzle, coasters, Christmas decorations, V-carved door sign and a
  weight-driven pendulum clock with every part. Each comes with a
  step-by-step guide: stock, tools, one step per tool and material
  (*Prepare this step* keeps only its layers, so a cutter change always
  means a new Z probe), assembly and use. The current step stays in view
  on the Run tab.

## [0.1.12] - 2026-11-22

### Fixed
- **Puzzle on lines** with colour pictures: thin outlines are no longer
  missed, light colours (skin, pale objects) stay inside the puzzle, and
  a small isolated mark such as a signature stays engraved but uncut.

### Added
- **Puzzle on lines** measures the outlines of a selected picture and
  sets the heavy line to match (*Measure* button to do it again).

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

[Unreleased]: https://github.com/barracudapps/MyMaxmakeLab/compare/v0.1.25...HEAD
[0.1.25]: https://github.com/barracudapps/MyMaxmakeLab/compare/v0.1.24...v0.1.25
[0.1.24]: https://github.com/barracudapps/MyMaxmakeLab/compare/v0.1.23...v0.1.24
[0.1.23]: https://github.com/barracudapps/MyMaxmakeLab/compare/v0.1.22...v0.1.23
[0.1.22]: https://github.com/barracudapps/MyMaxmakeLab/compare/v0.1.21...v0.1.22
[0.1.21]: https://github.com/barracudapps/MyMaxmakeLab/compare/v0.1.20...v0.1.21
[0.1.20]: https://github.com/barracudapps/MyMaxmakeLab/compare/v0.1.19...v0.1.20
[0.1.19]: https://github.com/barracudapps/MyMaxmakeLab/compare/v0.1.18...v0.1.19
[0.1.18]: https://github.com/barracudapps/MyMaxmakeLab/compare/v0.1.17...v0.1.18
[0.1.17]: https://github.com/barracudapps/MyMaxmakeLab/compare/v0.1.16...v0.1.17
[0.1.16]: https://github.com/barracudapps/MyMaxmakeLab/compare/v0.1.15...v0.1.16
[0.1.15]: https://github.com/barracudapps/MyMaxmakeLab/compare/v0.1.14...v0.1.15
[0.1.14]: https://github.com/barracudapps/MyMaxmakeLab/compare/v0.1.13...v0.1.14
[0.1.13]: https://github.com/barracudapps/MyMaxmakeLab/compare/v0.1.12...v0.1.13
[0.1.12]: https://github.com/barracudapps/MyMaxmakeLab/compare/v0.1.11...v0.1.12
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
