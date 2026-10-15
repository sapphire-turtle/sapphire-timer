//! The timer: one run at a time, held in memory by the process that owns it.
//!
//! A [`Timer`] is a single slot. Starting a run while another is going ends
//! the old one as [`Outcome::Replaced`]; the run itself steps through its
//! plan on a task of its own. Nothing here is persisted — a run lives only as
//! long as its process — but every change is handed to the timer's journal as
//! the run's [`LogEntry`], which is how the run reaches the workspace's logs.
//!
//! Watchers see the slot through [`Timer::subscribe`]: a [`State`] that is
//! replaced on every change. A watcher that falls behind sees only the latest
//! state, so each state carries a sequence number and the event that produced
//! it; a gap in `seq` is how a watcher learns it missed one.

use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use chrono::{DateTime, FixedOffset, Local};
use grain_id::GrainId;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::error::{Error, Result};
use crate::model::{LogEntry, Outcome, Preset, Step, StepLog};

/// What a run will do: its steps, repeated `cycles` times.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub label: String,
    pub preset: Option<String>,
    pub preset_id: Option<GrainId>,
    pub cycles: u32,
    pub steps: Vec<Step>,
    /// What the run is for; its log's body.
    pub comment: Option<String>,
}

impl Plan {
    /// One step of `minutes`.
    pub fn single(minutes: f64, label: Option<String>) -> Plan {
        let label = label
            .filter(|l| !l.trim().is_empty())
            .unwrap_or_else(|| "Timer".to_owned());
        Plan {
            label: label.clone(),
            preset: None,
            preset_id: None,
            cycles: 1,
            steps: vec![Step { label, minutes }],
            comment: None,
        }
    }

    /// A preset's steps, for its own cycles or `cycles` when given. The run
    /// is labelled with the preset's name unless `label` says otherwise.
    pub fn from_preset(preset: &Preset, cycles: Option<u32>, label: Option<String>) -> Plan {
        Plan {
            label: label
                .filter(|l| !l.trim().is_empty())
                .unwrap_or_else(|| preset.name.clone()),
            preset: Some(preset.name.clone()),
            preset_id: Some(preset.id),
            cycles: cycles.unwrap_or(preset.cycles),
            steps: preset.steps.clone(),
            comment: None,
        }
    }

    /// Say what the run is for. Blank text is none.
    pub fn with_comment(mut self, comment: Option<String>) -> Plan {
        self.comment = comment.as_deref().and_then(crate::store::normalize_text);
        self
    }

    pub fn validate(&self) -> Result<()> {
        if self.cycles == 0 {
            return Err(Error::Invalid("cycles must be at least 1".into()));
        }
        if self.steps.is_empty() {
            return Err(Error::Invalid("a timer needs at least one step".into()));
        }
        self.steps.iter().try_for_each(Step::validate)
    }
}

/// The run in the slot, as watchers and tools see it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Run {
    /// The run's id, which is also its log's.
    pub id: GrainId,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    pub started_at: DateTime<FixedOffset>,
    /// 1-based.
    pub cycle: u32,
    pub total_cycles: u32,
    /// 0-based, within the cycle.
    pub step_index: usize,
    pub total_steps: usize,
    pub step: Step,
    pub step_started_at: DateTime<FixedOffset>,
    pub step_ends_at: DateTime<FixedOffset>,
    /// The step after this one; `None` on the last.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_step: Option<String>,
}

/// What changed the slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// A run began.
    Started,
    /// A step ran to its end and the next one began.
    StepChanged,
    /// The last step ran to its end; the slot is empty.
    Finished,
    /// The run was cancelled; the slot is empty.
    Cancelled,
    /// A new run took the slot. The state that follows is the new run's
    /// `Started`, so a watcher that falls behind may see only that.
    Replaced,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Event {
    pub seq: u64,
    pub kind: EventKind,
    pub run_id: GrainId,
    pub label: String,
    pub at: DateTime<FixedOffset>,
    /// The step that just ended, on `StepChanged` and `Finished`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_step: Option<String>,
    /// The step now running, on `Started` and `StepChanged`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<String>,
}

/// The slot: the run in it, if any, and the last thing that happened.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct State {
    /// Counts events; 0 before the first.
    pub seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<Run>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_event: Option<Event>,
}

/// Where a run's log goes on every change. Called with the slot locked, so
/// it must not block: hand the entry to a queue.
pub type Journal = Box<dyn Fn(LogEntry) + Send + Sync>;

/// A single-slot timer.
pub struct Timer {
    /// Who runs it, written into each log.
    client: Option<String>,
    inner: Mutex<Inner>,
    state: watch::Sender<State>,
    journal: Option<Journal>,
}

#[derive(Default)]
struct Inner {
    seq: u64,
    active: Option<Active>,
}

struct Active {
    plan: Plan,
    log: LogEntry,
    cycle: u32,
    step: usize,
    step_started_at: DateTime<FixedOffset>,
    deadline: Instant,
    task: JoinHandle<()>,
}

impl Active {
    fn run(&self) -> Run {
        let step = self.plan.steps[self.step].clone();
        Run {
            id: self.log.id,
            label: self.plan.label.clone(),
            preset: self.plan.preset.clone(),
            started_at: self.log.started_at,
            cycle: self.cycle,
            total_cycles: self.plan.cycles,
            step_index: self.step,
            total_steps: self.plan.steps.len(),
            step_ends_at: self.step_started_at + to_chrono(step.minutes),
            step,
            step_started_at: self.step_started_at,
            next_step: self.next().map(|(_, i)| self.plan.steps[i].label.clone()),
        }
    }

    /// The (cycle, step) after this one.
    fn next(&self) -> Option<(u32, usize)> {
        if self.step + 1 < self.plan.steps.len() {
            Some((self.cycle, self.step + 1))
        } else if self.cycle < self.plan.cycles {
            Some((self.cycle + 1, 0))
        } else {
            None
        }
    }

    /// Close the running step's log at `at`.
    fn end_step(&mut self, at: DateTime<FixedOffset>, completed: bool) {
        if let Some(step) = self.log.steps.last_mut() {
            step.ended_at = Some(at);
            step.completed = completed;
        }
    }
}

impl Timer {
    /// An empty slot. `client` names who runs it in the logs; `journal`
    /// receives every change to a run's log.
    pub fn new(client: Option<String>, journal: Option<Journal>) -> Arc<Timer> {
        Arc::new(Timer {
            client,
            inner: Mutex::new(Inner::default()),
            state: watch::channel(State::default()).0,
            journal,
        })
    }

    /// Start `plan`, replacing the run in the slot if there is one.
    ///
    /// Must be called within a Tokio runtime: the run steps on a task.
    pub fn start(self: &Arc<Self>, plan: Plan) -> Result<Run> {
        plan.validate()?;
        let mut inner = self.lock();
        let now = now();
        if let Some(old) = inner.active.take() {
            self.end(&mut inner, old, now, Outcome::Replaced, EventKind::Replaced);
        }

        let id = GrainId::random();
        let first = plan.steps[0].clone();
        let log = LogEntry {
            id,
            label: plan.label.clone(),
            preset: plan.preset.clone(),
            preset_id: plan.preset_id,
            client: self.client.clone(),
            started_at: now,
            ended_at: None,
            outcome: Outcome::Running,
            cycles: plan.cycles,
            steps: vec![step_log(&first, 1, now)],
            comment: plan.comment.clone(),
        };
        let task = tokio::spawn(drive(Arc::downgrade(self), id));
        let active = Active {
            plan,
            log,
            cycle: 1,
            step: 0,
            step_started_at: now,
            deadline: Instant::now() + to_std(first.minutes),
            task,
        };
        let run = active.run();
        self.record(&active.log);
        inner.active = Some(active);
        self.publish(
            &mut inner,
            EventKind::Started,
            &run,
            now,
            None,
            Some(run.step.label.clone()),
        );
        Ok(run)
    }

    /// Cancel the run in the slot; the run as it was, or `None` if the slot
    /// was empty.
    pub fn cancel(&self) -> Option<Run> {
        let mut inner = self.lock();
        let active = inner.active.take()?;
        let run = active.run();
        self.end(
            &mut inner,
            active,
            now(),
            Outcome::Cancelled,
            EventKind::Cancelled,
        );
        Some(run)
    }

    /// The slot as it is now.
    pub fn state(&self) -> State {
        self.state.borrow().clone()
    }

    /// Watch the slot.
    pub fn subscribe(&self) -> watch::Receiver<State> {
        self.state.subscribe()
    }

    /// The run's current step is due: move on to the next, or finish.
    /// Whether there is a next step to wait for.
    fn advance(&self, run_id: GrainId) -> bool {
        let mut inner = self.lock();
        let Some(active) = inner.active.as_mut().filter(|a| a.log.id == run_id) else {
            return false;
        };
        let now = now();
        let ended = active.plan.steps[active.step].label.clone();
        match active.next() {
            Some((cycle, step)) => {
                active.end_step(now, true);
                active.cycle = cycle;
                active.step = step;
                active.step_started_at = now;
                // From the old deadline, not from now: a late wake-up does
                // not push every later step back.
                let next = active.plan.steps[step].clone();
                active.deadline += to_std(next.minutes);
                active.log.steps.push(step_log(&next, cycle, now));
                let run = active.run();
                self.record(&active.log);
                self.publish(
                    &mut inner,
                    EventKind::StepChanged,
                    &run,
                    now,
                    Some(ended),
                    Some(next.label),
                );
                true
            }
            None => {
                let active = inner.active.take().expect("checked above");
                self.end(
                    &mut inner,
                    active,
                    now,
                    Outcome::Completed,
                    EventKind::Finished,
                );
                false
            }
        }
    }

    /// End `active`, which has been taken out of the slot.
    fn end(
        &self,
        inner: &mut Inner,
        mut active: Active,
        at: DateTime<FixedOffset>,
        outcome: Outcome,
        kind: EventKind,
    ) {
        let completed = outcome == Outcome::Completed;
        if !completed {
            active.task.abort();
        }
        let run = active.run();
        active.end_step(at, completed);
        active.log.ended_at = Some(at);
        active.log.outcome = outcome;
        self.record(&active.log);
        let ended = completed.then(|| run.step.label.clone());
        self.publish(inner, kind, &run, at, ended, None);
    }

    fn record(&self, log: &LogEntry) {
        if let Some(journal) = &self.journal {
            journal(log.clone());
        }
    }

    fn publish(
        &self,
        inner: &mut Inner,
        kind: EventKind,
        run: &Run,
        at: DateTime<FixedOffset>,
        ended_step: Option<String>,
        step: Option<String>,
    ) {
        inner.seq += 1;
        let state = State {
            seq: inner.seq,
            run: inner.active.as_ref().map(Active::run),
            last_event: Some(Event {
                seq: inner.seq,
                kind,
                run_id: run.id,
                label: run.label.clone(),
                at,
                ended_step,
                step,
            }),
        };
        self.state.send_replace(state);
    }

    fn deadline(&self, run_id: GrainId) -> Option<Instant> {
        self.lock()
            .active
            .as_ref()
            .filter(|a| a.log.id == run_id)
            .map(|a| a.deadline)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        // A panic while holding the lock leaves nothing half-done that a
        // later call could trip over: every change is made in one go.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        if let Some(active) = self.lock().active.take() {
            active.task.abort();
        }
    }
}

/// Step `run_id` through its plan for as long as it holds the slot.
async fn drive(timer: Weak<Timer>, run_id: GrainId) {
    loop {
        let Some(deadline) = timer.upgrade().and_then(|t| t.deadline(run_id)) else {
            return;
        };
        tokio::time::sleep_until(deadline).await;
        let Some(timer) = timer.upgrade() else {
            return;
        };
        if !timer.advance(run_id) {
            return;
        }
    }
}

fn step_log(step: &Step, cycle: u32, at: DateTime<FixedOffset>) -> StepLog {
    StepLog {
        label: step.label.clone(),
        minutes: step.minutes,
        cycle,
        started_at: at,
        ended_at: None,
        completed: false,
    }
}

fn now() -> DateTime<FixedOffset> {
    Local::now().fixed_offset()
}

/// Validated minutes: positive and at most [`crate::model::MAX_STEP_MINUTES`].
fn to_std(minutes: f64) -> Duration {
    Duration::from_secs_f64(minutes * 60.0)
}

fn to_chrono(minutes: f64) -> chrono::Duration {
    chrono::Duration::from_std(to_std(minutes)).unwrap_or(chrono::Duration::MAX)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;

    use super::*;

    fn pomodoro(cycles: u32) -> Plan {
        Plan {
            label: "Pomodoro".into(),
            preset: Some("Pomodoro".into()),
            preset_id: None,
            cycles,
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
            comment: None,
        }
    }

    fn journaled() -> (Arc<Timer>, Arc<StdMutex<Vec<LogEntry>>>) {
        let logs = Arc::new(StdMutex::new(Vec::new()));
        let sink = Arc::clone(&logs);
        let timer = Timer::new(
            Some("local".into()),
            Some(Box::new(move |l| sink.lock().unwrap().push(l))),
        );
        (timer, logs)
    }

    async fn next_event(rx: &mut watch::Receiver<State>) -> State {
        rx.changed().await.expect("the timer is alive");
        rx.borrow_and_update().clone()
    }

    #[tokio::test(start_paused = true)]
    async fn a_preset_steps_through_every_cycle_then_finishes() {
        let (timer, logs) = journaled();
        let mut rx = timer.subscribe();
        let run = timer.start(pomodoro(2)).unwrap();
        assert_eq!(run.step.label, "Focus");
        assert_eq!(run.next_step.as_deref(), Some("Break"));

        let mut seen = Vec::new();
        loop {
            let state = next_event(&mut rx).await;
            let event = state.last_event.clone().unwrap();
            seen.push((event.kind, event.ended_step.clone(), event.step.clone()));
            if event.kind == EventKind::Finished {
                assert_eq!(state.run, None);
                break;
            }
        }
        let s = |x: &str| Some(x.to_owned());
        assert_eq!(
            seen,
            vec![
                (EventKind::Started, None, s("Focus")),
                (EventKind::StepChanged, s("Focus"), s("Break")),
                (EventKind::StepChanged, s("Break"), s("Focus")),
                (EventKind::StepChanged, s("Focus"), s("Break")),
                (EventKind::Finished, s("Break"), None),
            ]
        );

        let logs = logs.lock().unwrap();
        let last = logs.last().unwrap();
        assert_eq!(last.id, run.id);
        assert_eq!(last.outcome, Outcome::Completed);
        assert_eq!(last.steps.len(), 4);
        assert!(
            last.steps
                .iter()
                .all(|s| s.completed && s.ended_at.is_some())
        );
        assert_eq!(last.steps[2].cycle, 2);
        assert_eq!(last.client.as_deref(), Some("local"));
    }

    #[tokio::test(start_paused = true)]
    async fn steps_keep_to_the_plan_however_late_the_task_wakes() {
        let (timer, _) = journaled();
        let start = Instant::now();
        timer.start(pomodoro(1)).unwrap();
        let mut rx = timer.subscribe();
        // Focus ends at 25 min.
        next_event(&mut rx).await;
        assert_eq!(Instant::now() - start, Duration::from_secs(25 * 60));
        // Break ends at 30 min.
        next_event(&mut rx).await;
        assert_eq!(Instant::now() - start, Duration::from_secs(30 * 60));
    }

    #[tokio::test(start_paused = true)]
    async fn the_comment_is_the_logs_body_from_the_start() {
        let (timer, logs) = journaled();
        timer
            .start(Plan::single(5.0, None).with_comment(Some("  write the spec \n".into())))
            .unwrap();
        let first = logs.lock().unwrap()[0].clone();
        assert_eq!(first.comment.as_deref(), Some("write the spec"));
        timer.cancel();
    }

    #[tokio::test(start_paused = true)]
    async fn a_cancel_empties_the_slot_and_logs_the_cut_short_step() {
        let (timer, logs) = journaled();
        let run = timer.start(pomodoro(4)).unwrap();
        tokio::time::sleep(Duration::from_secs(60)).await;

        let cancelled = timer.cancel().unwrap();
        assert_eq!(cancelled.id, run.id);
        let state = timer.state();
        assert_eq!(state.run, None);
        assert_eq!(state.last_event.unwrap().kind, EventKind::Cancelled);
        assert!(timer.cancel().is_none());

        let last = logs.lock().unwrap().last().unwrap().clone();
        assert_eq!(last.outcome, Outcome::Cancelled);
        assert_eq!(last.steps.len(), 1);
        assert!(!last.steps[0].completed);
        assert!(last.ended_at.is_some());

        // Nothing fires after a cancel.
        let seq = timer.state().seq;
        tokio::time::sleep(Duration::from_secs(3 * 3600)).await;
        assert_eq!(timer.state().seq, seq);
    }

    #[tokio::test(start_paused = true)]
    async fn a_new_run_replaces_the_old_one() {
        let (timer, logs) = journaled();
        let old = timer.start(pomodoro(4)).unwrap();
        let new = timer.start(Plan::single(3.0, Some("Tea".into()))).unwrap();
        assert_ne!(old.id, new.id);

        let state = timer.state();
        assert_eq!(state.run.as_ref().unwrap().id, new.id);
        assert_eq!(state.last_event.unwrap().kind, EventKind::Started);
        let logs = logs.lock().unwrap();
        let old_log = logs.iter().rev().find(|l| l.id == old.id).unwrap();
        assert_eq!(old_log.outcome, Outcome::Replaced);
    }

    #[tokio::test(start_paused = true)]
    async fn a_single_timer_finishes_after_its_minutes() {
        let (timer, _) = journaled();
        let mut rx = timer.subscribe();
        let start = Instant::now();
        timer.start(Plan::single(1.5, None)).unwrap();
        next_event(&mut rx).await; // started
        let state = next_event(&mut rx).await;
        assert_eq!(state.last_event.unwrap().kind, EventKind::Finished);
        assert_eq!(Instant::now() - start, Duration::from_secs(90));
    }

    #[tokio::test]
    async fn an_invalid_plan_leaves_the_slot_alone() {
        let (timer, _) = journaled();
        timer.start(Plan::single(10.0, None)).unwrap();
        assert!(timer.start(Plan::single(0.0, None)).is_err());
        assert!(timer.state().run.is_some());
        timer.cancel();
    }
}
