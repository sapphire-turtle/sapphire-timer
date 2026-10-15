//! sapphire-timer-core: sapphire-timer's data model, its Markdown store, and
//! the timer itself.
//!
//! Two kinds of record live in the workspace, one Markdown file each with
//! YAML front matter, named `<grain-id>.md`:
//!
//! - `presets/`: a named sequence of steps (a Pomodoro, say), repeated for a
//!   number of cycles.
//! - `logs/`: one run of a timer — what was planned, which steps ran, and how
//!   it ended — for looking back on later.
//!
//! Those are what sync shares between devices. A running timer is not: it is
//! held in memory by the process that started it ([`engine::Timer`]), and
//! only its log reaches the workspace.

pub mod engine;
pub mod error;
pub mod model;
pub mod store;
pub mod timers;

pub use engine::{Event, EventKind, Plan, Run, State, Timer};
pub use error::{Error, Result};
pub use model::{LogEntry, Outcome, Preset, PresetUpdate, Step, StepLog};
pub use timers::Timers;

/// Process-wide application context, naming the cache, data and config
/// directories the framework uses. Mirrors `TALLY_CTX` in sapphire-tally.
pub static TIMER_CTX: sapphire_workspace::AppContext =
    sapphire_workspace::AppContext::new("sapphire-timer");

pub use sapphire_workspace::AppKind;

/// Point [`TIMER_CTX`] at this machine's cache, data and config directories.
///
/// `AppContext::cache_dir()` panics if this has not run, so every binary that
/// touches the context calls this first. The framework resolves the
/// directories (honouring `SAPPHIRE_TIMER_{CACHE,DATA,CONFIG}_DIR`) and
/// installs its console log subscriber. Idempotent: first writer wins.
pub fn init_app_context(kind: AppKind) {
    TIMER_CTX.init(kind);
}
