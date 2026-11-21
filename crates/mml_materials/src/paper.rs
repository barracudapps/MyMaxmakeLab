//! Quick laser settings for paper and board, from the energy each sheet
//! needs per millimetre of line. Starting points only: a test grid on a
//! scrap confirms them on the actual laser.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Sheet {
    /// Printer paper, 80 g/m².
    Paper,
    /// Card stock, 200 to 300 g/m².
    Card,
    /// Grey board, about 1.5 mm.
    Greyboard,
    /// Single-wall corrugated board, about 3 mm.
    Corrugated,
}

impl Sheet {
    pub const ALL: [Sheet; 4] = [
        Sheet::Paper,
        Sheet::Card,
        Sheet::Greyboard,
        Sheet::Corrugated,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Sheet::Paper => "paper.sheet_paper",
            Sheet::Card => "paper.sheet_card",
            Sheet::Greyboard => "paper.sheet_greyboard",
            Sheet::Corrugated => "paper.sheet_corrugated",
        }
    }

    /// Energy to cut through, J/mm, with a blue diode laser.
    fn cut_energy(self) -> f64 {
        match self {
            Sheet::Paper => 0.12,
            Sheet::Card => 0.5,
            Sheet::Greyboard => 3.0,
            Sheet::Corrugated => 2.0,
        }
    }

    /// Energy for a visible brown mark without cutting, J/mm.
    fn mark_energy(self) -> f64 {
        match self {
            Sheet::Paper => 0.02,
            Sheet::Card => 0.03,
            Sheet::Greyboard | Sheet::Corrugated => 0.05,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    Diode,
    /// Paper absorbs the 10.6 µm of a CO2 tube far better than blue
    /// light, white paper above all.
    Co2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Role {
    Cut,
    /// Dashed cut, for folds and tear lines.
    Perforate,
    /// Thin brown line.
    Mark,
    /// Filled areas (text, logos).
    Engrave,
}

impl Role {
    pub const ALL: [Role; 4] = [Role::Cut, Role::Perforate, Role::Mark, Role::Engrave];

    pub fn key(self) -> &'static str {
        match self {
            Role::Cut => "paper.role_cut",
            Role::Perforate => "paper.role_perforate",
            Role::Mark => "paper.role_mark",
            Role::Engrave => "paper.role_engrave",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Setup {
    pub sheet: Sheet,
    pub source: Source,
    /// Optical power of the laser, W (not the electrical power that
    /// some sellers print in large type).
    pub watts: f64,
    /// Fastest speed the machine moves cleanly, mm/min.
    pub max_speed: f64,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            sheet: Sheet::Paper,
            source: Source::Diode,
            watts: 10.0,
            max_speed: 6000.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Recipe {
    /// mm/min
    pub speed: f64,
    /// Percent of the maximum power.
    pub power: f64,
    pub passes: u32,
    pub air_assist: bool,
    /// Burnt and skipped lengths of a perforation, mm.
    pub dash: Option<(f64, f64)>,
    /// Line spacing of an engraving, mm.
    pub interval: f64,
}

/// Slowest useful speed: below it paper chars around the cut, so more
/// passes are taken instead.
const MIN_SPEED: f64 = 300.0;

pub fn recipe(setup: &Setup, role: Role) -> Recipe {
    let sheet = setup.sheet;
    let absorption = match setup.source {
        Source::Diode => 1.0,
        Source::Co2 => 0.5,
    };
    let (energy, power) = match role {
        Role::Cut | Role::Perforate => (sheet.cut_energy(), 80.0),
        Role::Mark => (sheet.mark_energy(), 30.0),
        Role::Engrave => (sheet.mark_energy() * 0.8, 30.0),
    };
    let energy = energy * absorption;
    let watts = setup.watts.max(0.5);
    let max_speed = setup.max_speed.max(MIN_SPEED);
    // speed (mm/s) = power delivered (W) / energy per mm (J/mm).
    let speed_at =
        |percent: f64, passes: u32| watts * percent / 100.0 / (energy / passes as f64) * 60.0;
    let mut power: f64 = power;
    let mut passes = 1;
    let mut speed = speed_at(power, passes);
    if speed > max_speed {
        // Faster than the machine can go: same energy with less power.
        speed = max_speed;
        power = (energy * speed / 60.0 / watts * 100.0).clamp(5.0, 100.0);
    } else if speed < MIN_SPEED {
        passes = (MIN_SPEED / speed).ceil() as u32;
        speed = speed_at(power, passes);
    }
    Recipe {
        speed: speed.round(),
        power: power.round(),
        passes,
        // Light paper flies away under the air jet; thicker sheets need
        // it against flames.
        air_assist: !matches!(sheet, Sheet::Paper) && matches!(role, Role::Cut | Role::Perforate),
        dash: (role == Role::Perforate).then_some(match sheet {
            Sheet::Paper | Sheet::Card => (1.0, 1.0),
            Sheet::Greyboard | Sheet::Corrugated => (3.0, 1.5),
        }),
        interval: 0.1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delivered(setup: &Setup, recipe: &Recipe) -> f64 {
        // Energy per mm actually delivered over all passes.
        setup.watts * recipe.power / 100.0 / (recipe.speed / 60.0) * recipe.passes as f64
    }

    #[test]
    fn printer_paper_is_cut_fast_with_reduced_power() {
        let setup = Setup::default();
        let cut = recipe(&setup, Role::Cut);
        assert_eq!((cut.speed, cut.power, cut.passes), (4000.0, 80.0, 1));
        assert!(!cut.air_assist, "air would blow light paper away");
        // A stronger laser hits the speed limit: power is lowered to
        // keep the same energy instead of burning wider.
        let strong = Setup {
            watts: 20.0,
            ..Setup::default()
        };
        let cut = recipe(&strong, Role::Cut);
        assert_eq!(cut.speed, 6000.0);
        assert!(cut.power < 80.0);
        assert!((delivered(&strong, &cut) - 0.12).abs() < 0.01);
    }

    #[test]
    fn thick_board_takes_several_passes_and_air() {
        let setup = Setup {
            sheet: Sheet::Greyboard,
            watts: 5.0,
            ..Setup::default()
        };
        let cut = recipe(&setup, Role::Cut);
        assert!(cut.passes > 1 && cut.speed >= MIN_SPEED - 1.0);
        assert!(cut.air_assist);
        assert!(delivered(&setup, &cut) >= 3.0 * 0.95);
    }

    #[test]
    fn co2_needs_less_energy_and_perforations_are_dashed() {
        let diode = recipe(
            &Setup {
                sheet: Sheet::Card,
                ..Setup::default()
            },
            Role::Cut,
        );
        let co2 = recipe(
            &Setup {
                sheet: Sheet::Card,
                source: Source::Co2,
                ..Setup::default()
            },
            Role::Cut,
        );
        assert!(co2.speed > diode.speed || co2.power < diode.power);
        let fold = recipe(&Setup::default(), Role::Perforate);
        assert_eq!(fold.dash, Some((1.0, 1.0)));
        let mark = recipe(&Setup::default(), Role::Mark);
        assert!(mark.dash.is_none() && !mark.air_assist);
    }
}
