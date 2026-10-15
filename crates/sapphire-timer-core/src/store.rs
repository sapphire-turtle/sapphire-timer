//! Reading and writing records as Markdown with YAML front matter.
//!
//! ```text
//! ---
//! id: 012atvw
//! name: Pomodoro
//! cycles: 4
//! steps:
//! - label: Focus
//!   minutes: 25.0
//! - label: Break
//!   minutes: 5.0
//! ---
//!
//! The classic one.
//! ```
//!
//! The free-text field (a preset's `description`, a log's `comment`) is the
//! body; everything else is front matter.

use std::path::{Path, PathBuf};

use chrono::{DateTime, FixedOffset};
use grain_id::GrainId;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::model::{LogEntry, Outcome, Preset, Step, StepLog};

/// The extension every record file has.
pub const EXTENSION: &str = "md";

pub const PRESETS_DIR: &str = "presets";
pub const LOGS_DIR: &str = "logs";

pub fn preset_path(root: &Path, id: GrainId) -> PathBuf {
    root.join(PRESETS_DIR).join(format!("{id}.{EXTENSION}"))
}

pub fn log_path(root: &Path, id: GrainId) -> PathBuf {
    root.join(LOGS_DIR).join(format!("{id}.{EXTENSION}"))
}

#[derive(Serialize, Deserialize)]
struct PresetFront {
    id: GrainId,
    name: String,
    #[serde(default = "one")]
    cycles: u32,
    steps: Vec<Step>,
}

fn one() -> u32 {
    1
}

#[derive(Serialize, Deserialize)]
struct LogFront {
    id: GrainId,
    label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    preset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    preset_id: Option<GrainId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    client: Option<String>,
    started_at: DateTime<FixedOffset>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ended_at: Option<DateTime<FixedOffset>>,
    outcome: Outcome,
    #[serde(default = "one")]
    cycles: u32,
    #[serde(default)]
    steps: Vec<StepLog>,
}

pub fn read_preset(root: &Path, id: GrainId) -> Result<Preset> {
    let path = preset_path(root, id);
    let (front, body): (PresetFront, _) = read(&path)?;
    warn_on_mismatch(&path, front.id, id);
    Ok(Preset {
        id,
        name: front.name,
        cycles: front.cycles,
        steps: front.steps,
        description: body,
    })
}

pub fn read_log(root: &Path, id: GrainId) -> Result<LogEntry> {
    let path = log_path(root, id);
    let (front, body): (LogFront, _) = read(&path)?;
    warn_on_mismatch(&path, front.id, id);
    Ok(LogEntry {
        id,
        label: front.label,
        preset: front.preset,
        preset_id: front.preset_id,
        client: front.client,
        started_at: front.started_at,
        ended_at: front.ended_at,
        outcome: front.outcome,
        cycles: front.cycles,
        steps: front.steps,
        comment: body,
    })
}

/// Write a new preset. An existing file is [`Error::AlreadyExists`].
pub fn create_preset(root: &Path, p: &Preset) -> Result<()> {
    create(&preset_path(root, p.id), &preset_text(p)?)
}

/// Replace an existing preset's file.
pub fn replace_preset(root: &Path, p: &Preset) -> Result<()> {
    std::fs::write(preset_path(root, p.id), preset_text(p)?)?;
    Ok(())
}

/// Write a log, over the one already there: a run's log is written when it
/// starts and again as each step ends.
pub fn put_log(root: &Path, l: &LogEntry) -> Result<()> {
    std::fs::write(log_path(root, l.id), log_text(l)?)?;
    Ok(())
}

/// Remove a record's file.
pub fn remove(path: &Path) -> Result<()> {
    std::fs::remove_file(path)?;
    Ok(())
}

fn preset_text(p: &Preset) -> Result<String> {
    let front = PresetFront {
        id: p.id,
        name: p.name.clone(),
        cycles: p.cycles,
        steps: p.steps.clone(),
    };
    render(&front, p.description.as_deref())
}

fn log_text(l: &LogEntry) -> Result<String> {
    let front = LogFront {
        id: l.id,
        label: l.label.clone(),
        preset: l.preset.clone(),
        preset_id: l.preset_id,
        client: l.client.clone(),
        started_at: l.started_at,
        ended_at: l.ended_at,
        outcome: l.outcome,
        cycles: l.cycles,
        steps: l.steps.clone(),
    };
    render(&front, l.comment.as_deref())
}

fn warn_on_mismatch(path: &Path, file_id: GrainId, filename_id: GrainId) {
    if file_id != filename_id {
        tracing::warn!(
            path = %path.display(),
            %file_id,
            %filename_id,
            "record id mismatch; using filename id"
        );
    }
}

fn create(path: &Path, text: &str) -> Result<()> {
    if path.exists() {
        return Err(Error::AlreadyExists(path.display().to_string()));
    }
    std::fs::write(path, text)?;
    Ok(())
}

/// Read a file's front matter as `T`, and its body: `None` when blank.
fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<(T, Option<String>)> {
    let raw = std::fs::read_to_string(path)?;
    let (front, body) = sapphire_frontmatter::parse::<T>(&raw).map_err(|e| Error::Parse {
        path: path.display().to_string(),
        message: e.to_string(),
    })?;
    Ok((front, normalize_text(body)))
}

fn render<T: Serialize>(front: &T, body: Option<&str>) -> Result<String> {
    let mut body = body.unwrap_or_default().trim().to_owned();
    if !body.is_empty() {
        body.push('\n');
    }
    sapphire_frontmatter::render(front, &body).map_err(|e| Error::Render(e.to_string()))
}

/// Trim free text, and read blank text as none.
pub fn normalize_text(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(s: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(s).unwrap()
    }

    #[test]
    fn a_preset_round_trips_with_its_description_as_the_body() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(PRESETS_DIR)).unwrap();
        let preset = Preset {
            id: "012atvw".parse().unwrap(),
            name: "Pomodoro".into(),
            cycles: 4,
            steps: vec![
                Step {
                    label: "Focus".into(),
                    minutes: 25.0,
                },
                Step {
                    label: "Break".into(),
                    minutes: 5.0,
                },
            ],
            description: Some("The classic one.".into()),
        };
        create_preset(dir.path(), &preset).unwrap();

        let text = std::fs::read_to_string(preset_path(dir.path(), preset.id)).unwrap();
        let (front, body) = sapphire_frontmatter::split(&text).unwrap();
        assert!(!front.contains("description"), "{text}");
        assert_eq!(body, "\nThe classic one.\n");
        assert_eq!(read_preset(dir.path(), preset.id).unwrap(), preset);
    }

    #[test]
    fn a_hand_written_preset_defaults_to_one_cycle() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(PRESETS_DIR)).unwrap();
        let id: GrainId = "012atvw".parse().unwrap();
        std::fs::write(
            preset_path(dir.path(), id),
            "---\nid: 012atvw\nname: 紅茶\nsteps:\n- label: 蒸らし\n  minutes: 3\n---\n",
        )
        .unwrap();
        let p = read_preset(dir.path(), id).unwrap();
        assert_eq!(p.cycles, 1);
        assert_eq!(p.steps[0].minutes, 3.0);
    }

    #[test]
    fn a_log_is_overwritten_as_the_run_goes_on() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(LOGS_DIR)).unwrap();
        let mut log = LogEntry {
            // All digits: YAML would read it as a number if it were not quoted.
            id: "0000001".parse().unwrap(),
            label: "Pomodoro".into(),
            preset: Some("Pomodoro".into()),
            preset_id: Some("012atvw".parse().unwrap()),
            client: Some("local".into()),
            started_at: ts("2026-10-15T09:00:00+09:00"),
            ended_at: None,
            outcome: Outcome::Running,
            cycles: 1,
            steps: vec![StepLog {
                label: "Focus".into(),
                minutes: 25.0,
                cycle: 1,
                started_at: ts("2026-10-15T09:00:00+09:00"),
                ended_at: None,
                completed: false,
            }],
            comment: Some("フレームワーク移行のPRレビュー".into()),
        };
        put_log(dir.path(), &log).unwrap();
        log.steps[0].ended_at = Some(ts("2026-10-15T09:25:00+09:00"));
        log.steps[0].completed = true;
        log.ended_at = log.steps[0].ended_at;
        log.outcome = Outcome::Completed;
        put_log(dir.path(), &log).unwrap();
        assert_eq!(read_log(dir.path(), log.id).unwrap(), log);
    }
}
