//! Workshop statistics and costs: a record of every job that ran, the
//! time each tool spent cutting, and what a job really costs (machine
//! wear, cutters, electricity, consumables, labour, material).

use serde::{Deserialize, Serialize};

/// Name of a tool in job records: the same text for the same cutter, so
/// its hours add up and match the tool library.
pub fn tool_label(kind: &str, diameter: f64, angle: Option<f64>) -> String {
    match angle {
        Some(angle) => format!("{kind} {angle:.0}° Ø{diameter:.3}"),
        None => format!("{kind} Ø{diameter:.3}"),
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ToolUse {
    pub label: String,
    pub seconds: f64,
    /// Length cut, mm.
    pub cut: f64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct JobRecord {
    /// Unix time of the end of the job, seconds.
    pub at: u64,
    pub job: String,
    pub machine: String,
    pub seconds: f64,
    pub completed: bool,
    pub tools: Vec<ToolUse>,
    pub cost: f64,
    /// Duration estimated before the job, seconds (0 in older records).
    pub estimated: f64,
}

/// How much longer (or shorter) jobs really take on `machine` than
/// estimated: the median ratio over its last completed jobs, with the
/// number of jobs it comes from. The estimate ignores what the machine
/// does that the program does not say (acceleration limits on short
/// segments, spindle spin-up, controller planning), which this learns.
pub fn time_factor(records: &[JobRecord], machine: &str) -> Option<(f64, usize)> {
    let mut ratios: Vec<f64> = records
        .iter()
        .rev()
        .filter(|record| record.machine == machine && record.completed && record.estimated >= 20.0)
        .take(10)
        .map(|record| record.seconds / record.estimated)
        .filter(|ratio| ratio.is_finite())
        .collect();
    if ratios.len() < 2 {
        return None;
    }
    ratios.sort_by(f64::total_cmp);
    let middle = ratios.len() / 2;
    let median = if ratios.len().is_multiple_of(2) {
        (ratios[middle - 1] + ratios[middle]) / 2.0
    } else {
        ratios[middle]
    };
    // A wildly different ratio is more likely a pause or a mistake.
    Some((median.clamp(0.5, 3.0), ratios.len()))
}

/// What running a machine costs; zero for what does not apply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CostSettings {
    pub currency: String,
    /// Purchase price and expected working life: the machine wears out
    /// a little with every hour of work.
    pub machine_price: f64,
    pub machine_life_hours: f64,
    /// Power drawn while working (spindle or laser, motors, extraction).
    pub power_watts: f64,
    pub electricity_per_kwh: f64,
    /// Air assist, lubricant, filters, laser tube wear... per hour.
    pub consumables_per_hour: f64,
    pub labour_per_hour: f64,
    /// Price and life of each tool, by tool label.
    pub tools: Vec<ToolCost>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolCost {
    pub label: String,
    pub price: f64,
    pub life_hours: f64,
}

impl Default for CostSettings {
    fn default() -> Self {
        Self {
            currency: "€".into(),
            machine_price: 600.0,
            machine_life_hours: 3000.0,
            power_watts: 300.0,
            electricity_per_kwh: 0.25,
            consumables_per_hour: 0.2,
            labour_per_hour: 0.0,
            tools: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CostBreakdown {
    pub machine: f64,
    pub electricity: f64,
    pub consumables: f64,
    pub tools: f64,
    pub labour: f64,
    pub material: f64,
}

impl CostBreakdown {
    pub fn total(&self) -> f64 {
        self.machine
            + self.electricity
            + self.consumables
            + self.tools
            + self.labour
            + self.material
    }
}

impl CostSettings {
    pub fn tool(&self, label: &str) -> Option<&ToolCost> {
        self.tools.iter().find(|tool| tool.label == label)
    }

    pub fn job_cost(&self, seconds: f64, tools: &[ToolUse], material: f64) -> CostBreakdown {
        let hours = seconds.max(0.0) / 3600.0;
        let per_hour = |price: f64, life: f64| if life > 0.0 { price / life } else { 0.0 };
        CostBreakdown {
            machine: hours * per_hour(self.machine_price, self.machine_life_hours),
            electricity: hours * self.power_watts / 1000.0 * self.electricity_per_kwh,
            consumables: hours * self.consumables_per_hour,
            tools: tools
                .iter()
                .filter_map(|usage| {
                    self.tool(&usage.label)
                        .map(|tool| usage.seconds / 3600.0 * per_hour(tool.price, tool.life_hours))
                })
                .sum(),
            labour: hours * self.labour_per_hour,
            material,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Summary {
    pub jobs: usize,
    pub completed: usize,
    pub hours: f64,
    pub cost: f64,
    /// Hours per machine, most used first.
    pub machines: Vec<(String, f64)>,
    /// Hours per tool and share of its life used, most used first.
    pub tools: Vec<(String, f64, Option<f64>)>,
    /// Hours per day over the last `days` days, oldest first.
    pub days: Vec<f64>,
}

fn add(list: &mut Vec<(String, f64)>, name: &str, hours: f64) {
    match list.iter_mut().find(|(known, _)| known == name) {
        Some((_, total)) => *total += hours,
        None => list.push((name.to_owned(), hours)),
    }
}

pub fn summarize(records: &[JobRecord], costs: &CostSettings, days: usize, now: u64) -> Summary {
    let mut summary = Summary {
        days: vec![0.0; days],
        ..Summary::default()
    };
    let mut tools: Vec<(String, f64)> = Vec::new();
    for record in records {
        summary.jobs += 1;
        summary.completed += usize::from(record.completed);
        let hours = record.seconds / 3600.0;
        summary.hours += hours;
        summary.cost += record.cost;
        add(&mut summary.machines, &record.machine, hours);
        for usage in &record.tools {
            add(&mut tools, &usage.label, usage.seconds / 3600.0);
        }
        let age = now.saturating_sub(record.at) / 86_400;
        if (age as usize) < days {
            summary.days[days - 1 - age as usize] += hours;
        }
    }
    summary.machines.sort_by(|a, b| b.1.total_cmp(&a.1));
    tools.sort_by(|a, b| b.1.total_cmp(&a.1));
    summary.tools = tools
        .into_iter()
        .map(|(label, hours)| {
            let worn = costs
                .tool(&label)
                .filter(|tool| tool.life_hours > 0.0)
                .map(|tool| hours / tool.life_hours);
            (label, hours, worn)
        })
        .collect();
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_job_costs_its_share_of_everything() {
        let mut costs = CostSettings {
            machine_price: 3000.0,
            machine_life_hours: 3000.0,
            power_watts: 500.0,
            electricity_per_kwh: 0.2,
            consumables_per_hour: 1.0,
            labour_per_hour: 20.0,
            ..CostSettings::default()
        };
        let mill = tool_label("End mill", 3.175, None);
        costs.tools.push(ToolCost {
            label: mill.clone(),
            price: 12.0,
            life_hours: 6.0,
        });
        let used = [ToolUse {
            label: mill,
            seconds: 1800.0,
            cut: 5000.0,
        }];
        // One hour: 1 machine + 0.1 power + 1 consumables + 20 labour,
        // half an hour of a 2/h cutter, 4 of material.
        let cost = costs.job_cost(3600.0, &used, 4.0);
        assert!((cost.machine - 1.0).abs() < 1e-9);
        assert!((cost.electricity - 0.1).abs() < 1e-9);
        assert!((cost.tools - 1.0).abs() < 1e-9);
        assert!((cost.total() - 27.1).abs() < 1e-9);
    }

    #[test]
    fn the_summary_adds_hours_per_machine_tool_and_day() {
        let day = 86_400;
        let now = 100 * day;
        let costs = CostSettings {
            tools: vec![ToolCost {
                label: "Laser".into(),
                price: 0.0,
                life_hours: 10.0,
            }],
            ..CostSettings::default()
        };
        let record = |at: u64, machine: &str, seconds: f64, completed: bool| JobRecord {
            at,
            machine: machine.into(),
            seconds,
            completed,
            tools: vec![ToolUse {
                label: "Laser".into(),
                seconds,
                cut: 0.0,
            }],
            cost: 1.0,
            ..JobRecord::default()
        };
        let records = [
            record(now - 10, "A", 3600.0, true),
            record(now - day - 10, "B", 7200.0, true),
            record(now - 40 * day, "A", 3600.0, false),
        ];
        let summary = summarize(&records, &costs, 7, now);
        assert_eq!((summary.jobs, summary.completed), (3, 2));
        assert!((summary.hours - 4.0).abs() < 1e-9);
        assert_eq!(summary.machines[0].0, "A");
        assert!((summary.machines[0].1 - 2.0).abs() < 1e-9);
        assert_eq!(summary.tools[0].2, Some(0.4));
        assert!((summary.days[6] - 1.0).abs() < 1e-9 && (summary.days[5] - 2.0).abs() < 1e-9);
    }

    #[test]
    fn estimates_learn_from_the_jobs_of_the_same_machine() {
        let job = |machine: &str, seconds: f64, estimated: f64, completed: bool| JobRecord {
            machine: machine.into(),
            seconds,
            estimated,
            completed,
            ..JobRecord::default()
        };
        let records = vec![
            job("HiMill", 130.0, 100.0, true),
            job("HiMill", 600.0, 100.0, false),
            job("Laser", 50.0, 100.0, true),
            job("HiMill", 120.0, 100.0, true),
            job("HiMill", 10.0, 5.0, true),
            job("HiMill", 125.0, 100.0, true),
        ];
        let (factor, count) = time_factor(&records, "HiMill").unwrap();
        // Unfinished and very short jobs are left out.
        assert_eq!(count, 3);
        assert!((factor - 1.25).abs() < 1e-9);
        assert_eq!(
            time_factor(&records, "Laser"),
            None,
            "one job is not enough"
        );
        // Older records without an estimate are ignored.
        assert_eq!(time_factor(&vec![job("A", 10.0, 0.0, true); 5], "A"), None);
    }
}
