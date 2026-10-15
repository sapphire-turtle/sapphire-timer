//! The timer's methods on its app server: presets and logs.
//!
//! These are what sync shares, so they live where the workspace is — on the
//! app server — and every client reaches them over its IPC as
//! `sapphire-timer.<method>`: the `mcp` subcommand by dialling the running
//! server, `/mcp` in-process. [`register`] adds them to the server's router;
//! [`crate::tools`] is the other end.
//!
//! The running timers are not here. They belong to the client that started
//! them ([`crate::clients`]), which writes each run's log back through
//! [`names::LOG_PUT`].

use std::sync::Arc;

use chrono::NaiveDate;
use grain_id::GrainId;
use sapphire_framework::ipc::{RequestCtx, Router, RpcError};
use sapphire_timer_core::{LogEntry, PresetUpdate, Step, Timers};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// Where a call finds its data directory: asked once per call.
///
/// The app server's is whatever workspace it has selected, which
/// `workspace select` can change underneath it; an error refuses the call.
pub type StateSource = Arc<dyn Fn() -> std::io::Result<Timers> + Send + Sync>;

/// The timer's methods, `sapphire-timer.<method>`.
pub mod names {
    pub const PRESET_ADD: &str = "sapphire-timer.preset_add";
    pub const PRESET_GET: &str = "sapphire-timer.preset_get";
    pub const PRESET_UPDATE: &str = "sapphire-timer.preset_update";
    pub const PRESET_DELETE: &str = "sapphire-timer.preset_delete";
    pub const PRESET_LIST: &str = "sapphire-timer.preset_list";
    pub const LOG_PUT: &str = "sapphire-timer.log_put";
    pub const LOG_GET: &str = "sapphire-timer.log_get";
    pub const LOG_COMMENT: &str = "sapphire-timer.log_comment";
    pub const LOG_LIST: &str = "sapphire-timer.log_list";
}

/// The version of the methods in [`names`], reported in the IPC handshake.
///
/// Bump it when a method's parameters or meaning change incompatibly: a
/// client built against another version is refused at connect time rather
/// than mid-call.
pub const APP_API: u32 = 1;

/// Add the timer's methods to the app server's `router`, each resolving the
/// data directory through `source` when it is called.
pub fn register(router: Router, source: StateSource) -> Router {
    let router = route(router, names::PRESET_ADD, &source, TimerMethods::preset_add);
    let router = route(router, names::PRESET_GET, &source, TimerMethods::preset_get);
    let router = route(
        router,
        names::PRESET_UPDATE,
        &source,
        TimerMethods::preset_update,
    );
    let router = route(
        router,
        names::PRESET_DELETE,
        &source,
        TimerMethods::preset_delete,
    );
    let router = route(
        router,
        names::PRESET_LIST,
        &source,
        TimerMethods::preset_list,
    );
    let router = route(router, names::LOG_PUT, &source, TimerMethods::log_put);
    let router = route(router, names::LOG_GET, &source, TimerMethods::log_get);
    let router = route(
        router,
        names::LOG_COMMENT,
        &source,
        TimerMethods::log_comment,
    );
    route(router, names::LOG_LIST, &source, TimerMethods::log_list)
}

/// Register one method: parse its parameters, then run `method` against the
/// current data directory on the blocking pool — it is plain files.
///
/// The method's text is the result; its error text is the RPC error's message,
/// which the tool hands to the agent as it is.
fn route<P>(
    router: Router,
    name: &str,
    source: &StateSource,
    method: fn(&TimerMethods, P) -> Result<String, String>,
) -> Router
where
    P: DeserializeOwned + Send + 'static,
{
    let source = Arc::clone(source);
    router.method(name, move |ctx: RequestCtx| {
        let source = Arc::clone(&source);
        async move {
            let params: P = serde_json::from_value(ctx.params)
                .map_err(|e| RpcError::invalid_params(e.to_string()))?;
            let text = tokio::task::spawn_blocking(move || {
                let timers = source().map_err(|e| e.to_string())?;
                method(&TimerMethods::new(timers), params)
            })
            .await
            .map_err(|e| RpcError::internal(format!("the timer method panicked: {e}")))?
            .map_err(RpcError::internal)?;
            Ok(serde_json::Value::String(text))
        }
    })
}

/// The server side of one call: the data directory it works on, and a
/// function per method returning the JSON text the agent reads.
#[derive(Clone, Debug)]
pub struct TimerMethods {
    timers: Timers,
}

// ── parameter structs ─────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EmptyParams {}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PresetAddParams {
    /// Unique, ignoring case.
    pub name: String,
    /// Run in order, once per cycle.
    pub steps: Vec<Step>,
    /// How many times the steps repeat (default 1).
    #[serde(default)]
    pub cycles: Option<u32>,
    /// Optional Markdown.
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PresetParams {
    /// The preset's name (ignoring case) or grain-id.
    pub preset: String,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PresetUpdateParams {
    /// The preset's name (ignoring case) or grain-id.
    pub preset: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub cycles: Option<u32>,
    /// Replaces every step.
    #[serde(default)]
    pub steps: Option<Vec<Step>>,
    /// An empty string removes it.
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LogPutParams {
    pub log: LogEntry,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct IdParams {
    pub id: GrainId,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LogCommentParams {
    pub id: GrainId,
    /// Markdown; an empty string removes it.
    pub comment: String,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LogQueryParams {
    /// YYYY-MM-DD, inclusive.
    #[serde(default)]
    pub from: Option<String>,
    /// YYYY-MM-DD, inclusive.
    #[serde(default)]
    pub to: Option<String>,
}

impl TimerMethods {
    pub fn new(timers: Timers) -> Self {
        Self { timers }
    }

    pub fn preset_add(&self, p: PresetAddParams) -> Result<String, String> {
        let preset = self
            .timers
            .add_preset(&p.name, p.cycles, p.steps, p.description)
            .map_err(|e| e.to_string())?;
        json(&preset)
    }

    pub fn preset_get(&self, p: PresetParams) -> Result<String, String> {
        json(
            &self
                .timers
                .find_preset(&p.preset)
                .map_err(|e| e.to_string())?,
        )
    }

    pub fn preset_update(&self, p: PresetUpdateParams) -> Result<String, String> {
        let update = PresetUpdate {
            name: p.name,
            cycles: p.cycles,
            steps: p.steps,
            description: p.description,
        };
        json(
            &self
                .timers
                .update_preset(&p.preset, update)
                .map_err(|e| e.to_string())?,
        )
    }

    pub fn preset_delete(&self, p: PresetParams) -> Result<String, String> {
        json(
            &self
                .timers
                .delete_preset(&p.preset)
                .map_err(|e| e.to_string())?,
        )
    }

    pub fn preset_list(&self, _: EmptyParams) -> Result<String, String> {
        json(&self.timers.list_presets().map_err(|e| e.to_string())?)
    }

    pub fn log_put(&self, p: LogPutParams) -> Result<String, String> {
        self.timers.put_log(&p.log).map_err(|e| e.to_string())?;
        json(&p.log.id)
    }

    pub fn log_get(&self, p: IdParams) -> Result<String, String> {
        json(&self.timers.get_log(p.id).map_err(|e| e.to_string())?)
    }

    pub fn log_comment(&self, p: LogCommentParams) -> Result<String, String> {
        json(
            &self
                .timers
                .set_log_comment(p.id, &p.comment)
                .map_err(|e| e.to_string())?,
        )
    }

    pub fn log_list(&self, p: LogQueryParams) -> Result<String, String> {
        let from = date(p.from.as_deref(), "from")?;
        let to = date(p.to.as_deref(), "to")?;
        json(&self.timers.list_logs(from, to).map_err(|e| e.to_string())?)
    }
}

fn date(raw: Option<&str>, field: &str) -> Result<Option<NaiveDate>, String> {
    raw.map(|s| {
        NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
            .map_err(|e| format!("`{field}` must be YYYY-MM-DD, got `{s}`: {e}"))
    })
    .transpose()
}

fn json<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string_pretty(value).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn methods() -> (tempfile::TempDir, TimerMethods) {
        let dir = tempfile::tempdir().unwrap();
        let timers = Timers::open(dir.path().to_owned()).unwrap();
        (dir, TimerMethods::new(timers))
    }

    #[test]
    fn a_preset_added_is_listed_and_found_by_name() {
        let (_dir, m) = methods();
        m.preset_add(PresetAddParams {
            name: "Pomodoro".into(),
            steps: vec![Step {
                label: "Focus".into(),
                minutes: 25.0,
            }],
            cycles: Some(4),
            description: None,
        })
        .unwrap();
        let list: serde_json::Value =
            serde_json::from_str(&m.preset_list(EmptyParams {}).unwrap()).unwrap();
        assert_eq!(list[0]["name"], "Pomodoro");
        let got: serde_json::Value = serde_json::from_str(
            &m.preset_get(PresetParams {
                preset: "POMODORO".into(),
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(got["cycles"], 4);
    }

    #[test]
    fn a_bad_date_is_named_in_the_error() {
        let (_dir, m) = methods();
        let err = m
            .log_list(LogQueryParams {
                from: Some("2026/10/15".into()),
                to: None,
            })
            .unwrap_err();
        assert!(err.contains("`from`"), "{err}");
    }
}
