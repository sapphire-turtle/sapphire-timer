use chrono::{DateTime, FixedOffset};
use grain_id::GrainId;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// The longest a step may run: a year. Anything longer is a typo, and would
/// overflow the clock arithmetic long before it overflowed a year.
pub const MAX_STEP_MINUTES: f64 = 365.0 * 24.0 * 60.0;

/// One step of a preset: a label ("Focus", "Break") and how long it runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Step {
    pub label: String,
    /// Minutes; fractions are allowed.
    pub minutes: f64,
}

impl Step {
    /// Refuse a step that could not be slept on.
    pub fn validate(&self) -> Result<()> {
        if self.label.trim().is_empty() {
            return Err(Error::Invalid("a step needs a label".into()));
        }
        if !self.minutes.is_finite() || self.minutes <= 0.0 || self.minutes > MAX_STEP_MINUTES {
            return Err(Error::Invalid(format!(
                "step `{}` must run for more than 0 and at most {MAX_STEP_MINUTES} minutes (got {})",
                self.label, self.minutes
            )));
        }
        Ok(())
    }
}

/// A preset: `presets/<id>.md`.
///
/// Every field but `description` is front matter; `description` is the
/// Markdown body. Names are unique, ignoring case, so a preset can be asked
/// for by name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    /// Repeats the filename so the file stands on its own. The filename wins
    /// when they disagree, with a warning.
    pub id: GrainId,
    pub name: String,
    /// How many times `steps` runs. At least 1.
    pub cycles: u32,
    pub steps: Vec<Step>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl Preset {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            return Err(Error::Invalid("a preset needs a name".into()));
        }
        if self.cycles == 0 {
            return Err(Error::Invalid("cycles must be at least 1".into()));
        }
        if self.steps.is_empty() {
            return Err(Error::Invalid(format!(
                "preset `{}` needs at least one step",
                self.name
            )));
        }
        self.steps.iter().try_for_each(Step::validate)
    }
}

/// A change to a preset: `None` keeps a field as it is. An empty
/// `description` removes it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PresetUpdate {
    pub name: Option<String>,
    pub cycles: Option<u32>,
    pub steps: Option<Vec<Step>>,
    pub description: Option<String>,
}

/// How a run ended. `Running` is a run whose log was written before it
/// ended — or whose process died before it could say.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Running,
    /// Every step ran to its end.
    Completed,
    /// Stopped by a cancel.
    Cancelled,
    /// Stopped because a new timer took its place.
    Replaced,
}

/// One step as it ran.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct StepLog {
    pub label: String,
    /// The minutes the step was planned for.
    pub minutes: f64,
    /// 1-based.
    pub cycle: u32,
    pub started_at: DateTime<FixedOffset>,
    /// `None` while it runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<FixedOffset>>,
    /// Whether it ran to its end, rather than being cut short.
    pub completed: bool,
}

/// One run of a timer: `logs/<id>.md`. The id is the run's.
///
/// Every field but `comment` is front matter; `comment` is the Markdown body:
/// what the run was for, written when it starts or added later.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LogEntry {
    pub id: GrainId,
    pub label: String,
    /// The preset's name when the run came from one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset_id: Option<GrainId>,
    /// Who ran it: "local" for a process of its own, or an external device's
    /// name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
    pub started_at: DateTime<FixedOffset>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<FixedOffset>>,
    pub outcome: Outcome,
    /// The cycles planned.
    pub cycles: u32,
    pub steps: Vec<StepLog>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset(steps: Vec<Step>, cycles: u32) -> Preset {
        Preset {
            id: "012atvw".parse().unwrap(),
            name: "Pomodoro".into(),
            cycles,
            steps,
            description: None,
        }
    }

    #[test]
    fn a_preset_needs_steps_and_cycles() {
        let focus = Step {
            label: "Focus".into(),
            minutes: 25.0,
        };
        assert!(preset(vec![focus.clone()], 4).validate().is_ok());
        assert!(preset(vec![], 4).validate().is_err());
        assert!(preset(vec![focus], 0).validate().is_err());
    }

    #[test]
    fn a_step_must_run_for_some_time() {
        for minutes in [0.0, -1.0, f64::NAN, f64::INFINITY, 1e300] {
            let step = Step {
                label: "x".into(),
                minutes,
            };
            assert!(step.validate().is_err(), "{minutes}");
        }
    }
}
