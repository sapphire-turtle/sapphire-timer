// SPDX-License-Identifier: GPL-3.0-or-later

//! The Timers screen: every preset in the server's workspace, and in the side
//! pane the logs of the one picked, newest first.
//!
//! Everything comes from the running server's `sapphire-timer.*` methods
//! ([`TimerClient`]), so the screen shows the workspace the server has
//! selected. Calls run on the tokio runtime and come back through a channel;
//! the view itself never blocks. The timers themselves still run from the
//! CLI and over MCP.

use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use chrono::Local;
use grain_id::GrainId;
use sapphire_framework::gui::{Side, egui};
use sapphire_timer_core::{LogEntry, Outcome, Preset};
use sapphire_timer_mcp::methods::{EmptyParams, LogQueryParams, names};

use crate::client::TimerClient;

/// How often the view re-reads the data on its own, to pick up the runs an
/// agent or sync wrote.
const REFRESH_EVERY: Duration = Duration::from_secs(10);

/// One refresh: every preset and every log.
struct Data {
    presets: Vec<Preset>,
    logs: Vec<LogEntry>,
}

/// What a refresh came back with.
type Reply = Result<Data, String>;

pub struct TimersView {
    runtime: tokio::runtime::Handle,
    client: Arc<TimerClient>,
    repaint: Arc<dyn Fn() + Send + Sync>,
    replies: (mpsc::Sender<Reply>, mpsc::Receiver<Reply>),

    data: Option<Data>,
    /// Why the data could not be read, if the last refresh failed.
    load_error: Option<String>,
    loading: bool,
    last_refresh: Option<Instant>,
    /// The preset picked: its logs are in the side pane.
    picked: Option<GrainId>,
}

impl TimersView {
    pub fn new(
        runtime: tokio::runtime::Handle,
        client: TimerClient,
        repaint: Arc<dyn Fn() + Send + Sync>,
    ) -> TimersView {
        TimersView {
            runtime,
            client: Arc::new(client),
            repaint,
            replies: mpsc::channel(),
            data: None,
            load_error: None,
            loading: false,
            last_refresh: None,
            picked: None,
        }
    }

    /// Take what came back, and read the data again when it is due. Called
    /// every frame, whichever screen is open.
    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(reply) = self.replies.1.try_recv() {
            self.loading = false;
            match reply {
                Ok(data) => {
                    self.picked = still_there(self.picked, &data.presets);
                    self.data = Some(data);
                    self.load_error = None;
                }
                // Keep showing the last data: a server restart should not
                // blank the window.
                Err(e) => self.load_error = Some(e),
            }
        }
        if !self.loading
            && self
                .last_refresh
                .is_none_or(|t| t.elapsed() >= REFRESH_EVERY)
        {
            self.refresh();
        }
        ctx.request_repaint_after(REFRESH_EVERY);
    }

    fn refresh(&mut self) {
        self.loading = true;
        self.last_refresh = Some(Instant::now());
        let client = Arc::clone(&self.client);
        let tx = self.replies.0.clone();
        let repaint = Arc::clone(&self.repaint);
        self.runtime.spawn(async move {
            let data = async {
                let presets = client.read(names::PRESET_LIST, EmptyParams {}).await?;
                let logs = client
                    .read(
                        names::LOG_LIST,
                        LogQueryParams {
                            from: None,
                            to: None,
                        },
                    )
                    .await?;
                Ok(Data { presets, logs })
            }
            .await;
            // The receiver lives as long as the view; a send after the window
            // closed has no one to tell.
            let _ = tx.send(data);
            repaint();
        });
    }

    /// The screen: the presets, one to pick.
    pub fn ui(&mut self, ui: &mut egui::Ui, side: &mut Side) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Timers");
            if let Some(e) = &self.load_error {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            let Some(data) = &self.data else {
                ui.weak("Loading…");
                return;
            };
            if data.presets.is_empty() {
                ui.weak("No timers yet: add a preset over MCP.");
                return;
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                for preset in &data.presets {
                    let picked = self.picked == Some(preset.id);
                    let response = ui
                        .selectable_label(picked, &preset.name)
                        .on_hover_text(summary(preset));
                    if response.clicked() {
                        self.picked = Some(preset.id);
                        side.open();
                    }
                }
            });
        });
    }

    /// The side pane: the picked preset's logs, newest first.
    pub fn side_ui(&mut self, ui: &mut egui::Ui) {
        let preset = self.data.as_ref().zip(self.picked).and_then(|(data, id)| {
            data.presets
                .iter()
                .find(|p| p.id == id)
                .map(|p| (p, logs_of(&data.logs, id)))
        });
        let Some((preset, logs)) = preset else {
            ui.weak("Pick a timer to see its logs.");
            return;
        };
        ui.heading(&preset.name);
        ui.weak(summary(preset));
        ui.separator();
        if logs.is_empty() {
            ui.weak("No runs yet.");
            return;
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for log in logs {
                log_ui(ui, log);
                ui.separator();
            }
        });
    }
}

/// One run: when it started, how it ended, who ran it, and its comment.
fn log_ui(ui: &mut egui::Ui, log: &LogEntry) {
    let started = log.started_at.with_timezone(&Local);
    ui.horizontal(|ui| {
        ui.strong(started.format("%Y-%m-%d %H:%M").to_string());
        ui.label(outcome(log.outcome));
        if let Some(ended) = log.ended_at {
            let minutes = (ended - log.started_at).num_minutes();
            ui.weak(format!("{minutes} min"));
        }
    });
    if let Some(client) = &log.client {
        ui.weak(format!("by {client}"));
    }
    if let Some(comment) = &log.comment {
        ui.label(comment);
    }
}

fn outcome(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Running => "running",
        Outcome::Completed => "completed",
        Outcome::Cancelled => "cancelled",
        Outcome::Replaced => "replaced",
    }
}

/// A preset's steps on one line: "4 × Focus 25 min, Break 5 min".
fn summary(preset: &Preset) -> String {
    let steps = preset
        .steps
        .iter()
        .map(|s| format!("{} {} min", s.label, s.minutes))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{} × {steps}", preset.cycles)
}

/// The logs of the preset `id`, newest first.
fn logs_of(logs: &[LogEntry], id: GrainId) -> Vec<&LogEntry> {
    let mut out: Vec<&LogEntry> = logs.iter().filter(|l| l.preset_id == Some(id)).collect();
    out.sort_by_key(|l| std::cmp::Reverse(l.started_at));
    out
}

/// The pick, unless its preset has gone.
fn still_there(picked: Option<GrainId>, presets: &[Preset]) -> Option<GrainId> {
    picked.filter(|id| presets.iter().any(|p| p.id == *id))
}

#[cfg(test)]
mod tests {
    use sapphire_timer_core::Step;

    use super::*;

    fn preset(id: &str) -> Preset {
        Preset {
            id: id.parse().unwrap(),
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
            description: None,
        }
    }

    fn log(id: &str, preset: Option<&str>, started: &str) -> LogEntry {
        LogEntry {
            id: id.parse().unwrap(),
            label: "Pomodoro".into(),
            preset: None,
            preset_id: preset.map(|p| p.parse().unwrap()),
            client: None,
            started_at: chrono::DateTime::parse_from_rfc3339(started).unwrap(),
            ended_at: None,
            outcome: Outcome::Completed,
            cycles: 4,
            steps: Vec::new(),
            comment: None,
        }
    }

    #[test]
    fn a_presets_logs_are_its_own_newest_first() {
        let logs = vec![
            log("012atv1", Some("012atvw"), "2026-10-27T09:00:00+09:00"),
            log("012atv2", Some("012atvx"), "2026-10-28T09:00:00+09:00"),
            log("012atv3", None, "2026-10-28T10:00:00+09:00"),
            log("012atv4", Some("012atvw"), "2026-10-28T08:00:00+09:00"),
        ];
        let ids: Vec<String> = logs_of(&logs, "012atvw".parse().unwrap())
            .iter()
            .map(|l| l.id.to_string())
            .collect();
        assert_eq!(ids, ["012atv4", "012atv1"]);
    }

    #[test]
    fn a_pick_is_dropped_with_its_preset() {
        let presets = vec![preset("012atvw")];
        let id = "012atvw".parse().unwrap();
        assert_eq!(still_there(Some(id), &presets), Some(id));
        assert_eq!(still_there(Some(id), &[]), None);
    }

    #[test]
    fn a_summary_names_the_cycles_and_steps() {
        assert_eq!(summary(&preset("012atvw")), "4 × Focus 25 min, Break 5 min");
    }
}
