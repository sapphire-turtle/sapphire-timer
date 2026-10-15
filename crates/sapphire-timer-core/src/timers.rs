//! The facade over one data directory: presets and logs.

use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use grain_id::GrainId;

use crate::error::{Error, Result};
use crate::model::{LogEntry, Preset, PresetUpdate, Step};
use crate::store;

/// A data directory's root, holding `presets/` and `logs/` flat.
#[derive(Debug, Clone)]
pub struct Timers {
    pub root: PathBuf,
}

impl Timers {
    /// Open a data directory, creating `presets/` and `logs/` if missing.
    pub fn open(root: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(root.join(store::PRESETS_DIR))?;
        std::fs::create_dir_all(root.join(store::LOGS_DIR))?;
        Ok(Self { root })
    }

    /// Create a preset under a new grain-id. Its name must not be taken.
    pub fn add_preset(
        &self,
        name: &str,
        cycles: Option<u32>,
        steps: Vec<Step>,
        description: Option<String>,
    ) -> Result<Preset> {
        let preset = Preset {
            id: GrainId::random(),
            name: name.trim().to_owned(),
            cycles: cycles.unwrap_or(1),
            steps,
            description: description.as_deref().and_then(store::normalize_text),
        };
        preset.validate()?;
        self.check_name_free(&preset.name, None)?;
        store::create_preset(&self.root, &preset)?;
        Ok(preset)
    }

    /// One preset, by grain-id or by name (ignoring case).
    pub fn find_preset(&self, key: &str) -> Result<Preset> {
        let key = key.trim();
        if let Ok(id) = key.parse::<GrainId>()
            && store::preset_path(&self.root, id).is_file()
        {
            return store::read_preset(&self.root, id);
        }
        self.list_presets()?
            .into_iter()
            .find(|p| p.name.eq_ignore_ascii_case(key))
            .ok_or_else(|| Error::PresetNotFound(key.to_owned()))
    }

    /// Change a preset's fields, keeping the ones `update` leaves `None`.
    pub fn update_preset(&self, key: &str, update: PresetUpdate) -> Result<Preset> {
        let mut preset = self.find_preset(key)?;
        if let Some(name) = update.name {
            let name = name.trim().to_owned();
            self.check_name_free(&name, Some(preset.id))?;
            preset.name = name;
        }
        if let Some(cycles) = update.cycles {
            preset.cycles = cycles;
        }
        if let Some(steps) = update.steps {
            preset.steps = steps;
        }
        if let Some(description) = update.description {
            preset.description = store::normalize_text(&description);
        }
        preset.validate()?;
        store::replace_preset(&self.root, &preset)?;
        Ok(preset)
    }

    /// Delete a preset. Logs of runs made from it keep its name.
    pub fn delete_preset(&self, key: &str) -> Result<Preset> {
        let preset = self.find_preset(key)?;
        store::remove(&store::preset_path(&self.root, preset.id))?;
        Ok(preset)
    }

    /// Every preset, by name. An unreadable file is skipped with a warning.
    pub fn list_presets(&self) -> Result<Vec<Preset>> {
        let mut out = Vec::new();
        for id in record_ids(&self.root.join(store::PRESETS_DIR))? {
            match store::read_preset(&self.root, id) {
                Ok(p) => out.push(p),
                Err(e) => tracing::warn!(%id, "skipping unreadable preset: {e}"),
            }
        }
        out.sort_by_key(|p| p.name.to_lowercase());
        Ok(out)
    }

    /// Write a run's log, over an earlier write of the same run.
    ///
    /// The timer writes the log again as each step ends, from what it knew
    /// when the run started. A comment added to the file since
    /// ([`Self::set_log_comment`], or by hand) is kept when the timer's
    /// copy has none.
    pub fn put_log(&self, log: &LogEntry) -> Result<()> {
        if log.comment.is_none() && store::log_path(&self.root, log.id).is_file() {
            let on_disk = store::read_log(&self.root, log.id)?;
            if on_disk.comment.is_some() {
                let mut log = log.clone();
                log.comment = on_disk.comment;
                return store::put_log(&self.root, &log);
            }
        }
        store::put_log(&self.root, log)
    }

    /// Set what a run was for; an empty comment removes it.
    pub fn set_log_comment(&self, id: GrainId, comment: &str) -> Result<LogEntry> {
        let mut log = self.get_log(id)?;
        log.comment = store::normalize_text(comment);
        store::put_log(&self.root, &log)?;
        Ok(log)
    }

    /// One log by grain-id.
    pub fn get_log(&self, id: GrainId) -> Result<LogEntry> {
        if !store::log_path(&self.root, id).is_file() {
            return Err(Error::LogNotFound(id.to_string()));
        }
        store::read_log(&self.root, id)
    }

    /// Logs of runs started between `from` and `to` (inclusive, by the date
    /// in the run's own offset), oldest first. An unreadable file is skipped
    /// with a warning.
    pub fn list_logs(
        &self,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
    ) -> Result<Vec<LogEntry>> {
        let mut out = Vec::new();
        for id in record_ids(&self.root.join(store::LOGS_DIR))? {
            let log = match store::read_log(&self.root, id) {
                Ok(l) => l,
                Err(e) => {
                    tracing::warn!(%id, "skipping unreadable log: {e}");
                    continue;
                }
            };
            let day = log.started_at.date_naive();
            if from.is_some_and(|f| day < f) || to.is_some_and(|t| day > t) {
                continue;
            }
            out.push(log);
        }
        out.sort_by_key(|l| l.started_at);
        Ok(out)
    }

    fn check_name_free(&self, name: &str, except: Option<GrainId>) -> Result<()> {
        let taken = self
            .list_presets()?
            .into_iter()
            .any(|p| Some(p.id) != except && p.name.eq_ignore_ascii_case(name));
        if taken {
            return Err(Error::PresetNameTaken(name.to_owned()));
        }
        Ok(())
    }
}

/// The grain-ids of the `<id>.md` files in `dir`. Anything else is skipped.
fn record_ids(dir: &Path) -> Result<Vec<GrainId>> {
    let mut ids = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some(store::EXTENSION) {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        match stem.parse() {
            Ok(id) => ids.push(id),
            Err(_) => tracing::debug!(path = %path.display(), "not a record; skipping"),
        }
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steps() -> Vec<Step> {
        vec![
            Step {
                label: "Focus".into(),
                minutes: 25.0,
            },
            Step {
                label: "Break".into(),
                minutes: 5.0,
            },
        ]
    }

    #[test]
    fn a_preset_is_found_by_id_or_by_name_ignoring_case() {
        let dir = tempfile::tempdir().unwrap();
        let timers = Timers::open(dir.path().to_owned()).unwrap();
        let p = timers
            .add_preset("Pomodoro", Some(4), steps(), None)
            .unwrap();
        assert_eq!(timers.find_preset(&p.id.to_string()).unwrap(), p);
        assert_eq!(timers.find_preset("pomodoro").unwrap(), p);
        assert!(matches!(
            timers.find_preset("tea"),
            Err(Error::PresetNotFound(_))
        ));
    }

    #[test]
    fn preset_names_are_unique_ignoring_case() {
        let dir = tempfile::tempdir().unwrap();
        let timers = Timers::open(dir.path().to_owned()).unwrap();
        timers.add_preset("Pomodoro", None, steps(), None).unwrap();
        let err = timers
            .add_preset("POMODORO", None, steps(), None)
            .unwrap_err();
        assert!(matches!(err, Error::PresetNameTaken(_)), "{err}");

        let tea = timers.add_preset("Tea", None, steps(), None).unwrap();
        let err = timers
            .update_preset(
                &tea.id.to_string(),
                PresetUpdate {
                    name: Some("pomodoro".into()),
                    ..Default::default()
                },
            )
            .unwrap_err();
        assert!(matches!(err, Error::PresetNameTaken(_)), "{err}");
        // Renaming to itself is not a clash.
        timers
            .update_preset(
                "tea",
                PresetUpdate {
                    name: Some("TEA".into()),
                    ..Default::default()
                },
            )
            .unwrap();
    }

    #[test]
    fn a_comment_added_to_a_running_log_survives_the_timers_next_write() {
        let dir = tempfile::tempdir().unwrap();
        let timers = Timers::open(dir.path().to_owned()).unwrap();
        let mut log = LogEntry {
            id: GrainId::random(),
            label: "Pomodoro".into(),
            preset: None,
            preset_id: None,
            client: None,
            started_at: chrono::Local::now().fixed_offset(),
            ended_at: None,
            outcome: crate::Outcome::Running,
            cycles: 1,
            steps: vec![],
            comment: None,
        };
        timers.put_log(&log).unwrap();
        timers.set_log_comment(log.id, "PR review").unwrap();

        log.outcome = crate::Outcome::Completed;
        timers.put_log(&log).unwrap();
        let got = timers.get_log(log.id).unwrap();
        assert_eq!(got.outcome, crate::Outcome::Completed);
        assert_eq!(got.comment.as_deref(), Some("PR review"));

        timers.set_log_comment(log.id, " ").unwrap();
        assert_eq!(timers.get_log(log.id).unwrap().comment, None);
    }

    #[test]
    fn an_update_is_validated_before_it_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let timers = Timers::open(dir.path().to_owned()).unwrap();
        timers.add_preset("Pomodoro", None, steps(), None).unwrap();
        let err = timers
            .update_preset(
                "pomodoro",
                PresetUpdate {
                    steps: Some(vec![]),
                    ..Default::default()
                },
            )
            .unwrap_err();
        assert!(matches!(err, Error::Invalid(_)), "{err}");
        assert_eq!(timers.find_preset("pomodoro").unwrap().steps, steps());
    }
}
