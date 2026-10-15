//! The timer's MCP handler: tools, the active-timer resource, and its
//! subscription.
//!
//! - **Presets and logs** are tools that call the app server's
//!   `sapphire-timer.*` methods ([`crate::methods`]), beside the framework's
//!   read-only tools.
//! - **The timer** is the calling client's own ([`crate::clients`]):
//!   `timer_start`, `timer_start_preset`, `timer_cancel`, `timer_status`.
//! - **[`ACTIVE_TIMER_URI`]** is a resource holding that timer's
//!   [`State`](sapphire_timer_core::State). A client that wants to hear from
//!   its timer opens `subscriptions/listen` for the URI: each change to the
//!   timer — a run starting, a step ending, the run finishing or being
//!   cancelled — is a `notifications/resources/updated`, and the client reads
//!   the resource for what happened. That is the only kind of notification
//!   the 2026-07-28 protocol lets a server send outside a request.

use std::sync::Arc;

use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        Extensions, ListResourcesResult, PaginatedRequestParams, ReadResourceRequestParams,
        ReadResourceResponse, ReadResourceResult, Resource, ResourceContents, ServerCapabilities,
        ServerConfig, SubscriptionFilter,
    },
    service::{RequestContext, SubscriptionContext},
    tool, tool_handler, tool_router,
};
use sapphire_framework::ipc;
use sapphire_framework::mcp::{self, McpService, ToolContext};
use sapphire_timer_core::{Plan, Preset};
use serde::{Deserialize, Serialize};

use crate::clients::{Client, Clients};
use crate::methods::{
    EmptyParams, IdParams, LogCommentParams, LogQueryParams, PresetAddParams, PresetParams,
    PresetUpdateParams, names,
};

/// The calling client's timer, as a resource.
pub const ACTIVE_TIMER_URI: &str = "sapphire-timer://active";

/// How the timer answers MCP: a [`TimerTools`] per session (stdio) or per
/// request (HTTP), all sharing one set of [`Clients`].
pub fn service() -> McpService {
    let clients = Clients::new();
    McpService::new(move |ctx| TimerTools::new(ctx, Arc::clone(&clients)))
}

#[derive(Clone)]
pub struct TimerTools {
    ctx: ToolContext,
    clients: Arc<Clients>,
    tool_router: ToolRouter<Self>,
}

impl AsRef<ToolContext> for TimerTools {
    fn as_ref(&self) -> &ToolContext {
        &self.ctx
    }
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct StartParams {
    /// How long, in minutes; fractions are allowed.
    pub minutes: f64,
    /// What the timer is for, said back when it ends.
    #[serde(default)]
    pub label: Option<String>,
    /// A note on what this time is spent on: the run log's body.
    #[serde(default)]
    pub comment: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct StartPresetParams {
    /// The preset's name (ignoring case) or grain-id.
    pub preset: String,
    /// Run this many cycles instead of the preset's own.
    #[serde(default)]
    pub cycles: Option<u32>,
    /// Label the run with this instead of the preset's name.
    #[serde(default)]
    pub label: Option<String>,
    /// A note on what this time is spent on: the run log's body.
    #[serde(default)]
    pub comment: Option<String>,
}

impl TimerTools {
    /// The timer's tools and the framework's read-only ones. The framework's
    /// raw file writes are left out: the tools below cover every write, and
    /// a hand-written preset skips their checks.
    pub fn new(ctx: ToolContext, clients: Arc<Clients>) -> Self {
        Self {
            ctx,
            clients,
            tool_router: Self::tool_router() + mcp::framework_tools(),
        }
    }

    /// Call one of the timer's methods and hand its text back as the tool's.
    ///
    /// An error the method answered with is the timer's message to the agent,
    /// so it goes through without the IPC layer's framing.
    async fn call(&self, method: &str, params: impl Serialize) -> Result<String, String> {
        self.ctx.call(method, params).await.map_err(|e| match e {
            mcp::Error::Ipc(ipc::Error::Rpc(rpc)) => rpc.message,
            other => other.to_string(),
        })
    }

    fn start(&self, extensions: &Extensions, plan: Plan) -> Result<String, String> {
        let timer = self.clients.timer(&Client::of(extensions), &self.ctx);
        let run = timer.start(plan).map_err(|e| e.to_string())?;
        json(&serde_json::json!({ "run": run, "resource": ACTIVE_TIMER_URI }))
    }

    /// The calling client's timer state, as JSON.
    fn state_json(&self, extensions: &Extensions) -> Result<String, String> {
        let state = self
            .clients
            .get(&Client::of(extensions))
            .map(|t| t.state())
            .unwrap_or_default();
        json(&state)
    }
}

#[tool_router]
impl TimerTools {
    #[tool(
        description = "Start a timer of `minutes`, replacing this client's running timer if \
        any. Only this client sees it. Returns the run and the resource URI to subscribe to \
        for its changes."
    )]
    pub async fn timer_start(
        &self,
        extensions: Extensions,
        Parameters(p): Parameters<StartParams>,
    ) -> Result<String, String> {
        self.start(
            &extensions,
            Plan::single(p.minutes, p.label).with_comment(p.comment),
        )
    }

    #[tool(
        description = "Start a preset (e.g. a Pomodoro) by name or id, replacing this client's \
        running timer if any. Each step's end is a change to the timer resource. `cycles` \
        overrides the preset's own. Returns the run and the resource URI."
    )]
    pub async fn timer_start_preset(
        &self,
        extensions: Extensions,
        Parameters(p): Parameters<StartPresetParams>,
    ) -> Result<String, String> {
        let text = self
            .call(names::PRESET_GET, PresetParams { preset: p.preset })
            .await?;
        let preset: Preset = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        self.start(
            &extensions,
            Plan::from_preset(&preset, p.cycles, p.label).with_comment(p.comment),
        )
    }

    #[tool(
        description = "Cancel this client's running timer. Returns the run as it was, or \
        null if none was running."
    )]
    pub async fn timer_cancel(
        &self,
        extensions: Extensions,
        Parameters(_): Parameters<EmptyParams>,
    ) -> Result<String, String> {
        let run = self
            .clients
            .get(&Client::of(&extensions))
            .and_then(|t| t.cancel());
        json(&run)
    }

    #[tool(
        description = "This client's timer: the running run (step, cycle, when the step ends) \
        if any, and the last thing that happened to it.",
        annotations(read_only_hint = true)
    )]
    pub async fn timer_status(
        &self,
        extensions: Extensions,
        Parameters(_): Parameters<EmptyParams>,
    ) -> Result<String, String> {
        self.state_json(&extensions)
    }

    #[tool(
        description = "Create a preset: a unique name, steps (each a label and minutes) run in \
        order, and how many cycles they repeat (default 1). `description` is optional Markdown. \
        Returns the preset JSON including its id."
    )]
    pub async fn preset_add(
        &self,
        Parameters(p): Parameters<PresetAddParams>,
    ) -> Result<String, String> {
        self.call(names::PRESET_ADD, p).await
    }

    #[tool(
        description = "Show one preset by name (ignoring case) or id.",
        annotations(read_only_hint = true)
    )]
    pub async fn preset_get(
        &self,
        Parameters(p): Parameters<PresetParams>,
    ) -> Result<String, String> {
        self.call(names::PRESET_GET, p).await
    }

    #[tool(
        description = "Change a preset's name, cycles, steps (all replaced) or description. \
        Omitted fields stay as they are; an empty description removes it. Returns the updated \
        preset JSON."
    )]
    pub async fn preset_update(
        &self,
        Parameters(p): Parameters<PresetUpdateParams>,
    ) -> Result<String, String> {
        self.call(names::PRESET_UPDATE, p).await
    }

    #[tool(
        description = "Delete a preset by name or id. Logs of its runs keep its name. Returns \
        the preset as it was.",
        annotations(destructive_hint = true)
    )]
    pub async fn preset_delete(
        &self,
        Parameters(p): Parameters<PresetParams>,
    ) -> Result<String, String> {
        self.call(names::PRESET_DELETE, p).await
    }

    #[tool(
        description = "List every preset as a JSON array, by name.",
        annotations(read_only_hint = true)
    )]
    pub async fn preset_list(
        &self,
        Parameters(p): Parameters<EmptyParams>,
    ) -> Result<String, String> {
        self.call(names::PRESET_LIST, p).await
    }

    #[tool(
        description = "Show one run's log by id: its steps as they ran and how it ended.",
        annotations(read_only_hint = true)
    )]
    pub async fn log_get(&self, Parameters(p): Parameters<IdParams>) -> Result<String, String> {
        self.call(names::LOG_GET, p).await
    }

    #[tool(
        description = "Set the comment on a run's log: what the time went on, as Markdown. \
        An empty comment removes it. Returns the updated log."
    )]
    pub async fn log_comment(
        &self,
        Parameters(p): Parameters<LogCommentParams>,
    ) -> Result<String, String> {
        self.call(names::LOG_COMMENT, p).await
    }

    #[tool(
        description = "List run logs as a JSON array, oldest first. Optional from/to \
        (YYYY-MM-DD, inclusive) filter by the day a run started. Logs are every client's, \
        synced across devices.",
        annotations(read_only_hint = true)
    )]
    pub async fn log_list(
        &self,
        Parameters(p): Parameters<LogQueryParams>,
    ) -> Result<String, String> {
        self.call(names::LOG_LIST, p).await
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for TimerTools {
    fn get_info(&self) -> ServerConfig {
        let mut info = mcp::server_info(&self.ctx);
        info.capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .enable_resources_subscribe()
            .build();
        let framework = info.instructions.clone().unwrap_or_default();
        info.with_instructions(format!(
            "Sapphire Timer runs timers and presets (a Pomodoro: steps repeated for some \
             cycles) and keeps a log of every run. Each client has one timer of its own; \
             starting another replaces it. Presets and logs are shared through the workspace. \
             To hear when a step or the run ends, subscribe to {ACTIVE_TIMER_URI} with \
             subscriptions/listen and read it on each update.\n\n{framework}"
        ))
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        Ok(ListResourcesResult::with_all_items(vec![
            Resource::new(ACTIVE_TIMER_URI, "active-timer")
                .with_title("This client's timer")
                .with_description(
                    "The running run, if any, and the last event; changes on every step.",
                )
                .with_mime_type("application/json"),
        ]))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        if request.uri != ACTIVE_TIMER_URI {
            return Err(ErrorData::resource_not_found(
                format!("no resource {}", request.uri),
                None,
            ));
        }
        let text = self
            .state_json(&context.extensions)
            .map_err(|e| ErrorData::internal_error(e, None))?;
        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(text, ACTIVE_TIMER_URI).with_mime_type("application/json"),
        ])
        .into())
    }

    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        let wants_timer = requested
            .resource_subscriptions
            .as_ref()
            .is_some_and(|uris| uris.iter().any(|u| u == ACTIVE_TIMER_URI));
        Some(if wants_timer {
            SubscriptionFilter::builder()
                .resource_subscription(ACTIVE_TIMER_URI)
                .build()
        } else {
            SubscriptionFilter::new()
        })
    }

    /// Hold the subscription open, telling the client each time its timer
    /// changes, until the client cancels it or goes away.
    async fn listen(&self, context: SubscriptionContext) -> Result<(), ErrorData> {
        let client = Client::of(&context.request_context().extensions);
        let mut changes = self.clients.timer(&client, &self.ctx).subscribe();
        changes.mark_unchanged();
        let sink = context.sink().clone();
        loop {
            tokio::select! {
                () = context.cancelled() => return Ok(()),
                changed = changes.changed() => {
                    if changed.is_err() {
                        return Ok(());
                    }
                    if let Err(e) = sink.notify_resource_updated(ACTIVE_TIMER_URI).await {
                        tracing::debug!("the timer subscription ended: {e}");
                        return Ok(());
                    }
                }
            }
        }
    }
}

fn json<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string_pretty(value).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools() -> Vec<rmcp::model::Tool> {
        let endpoint = ipc::Endpoint::in_dir("sapphire-timer", std::env::temp_dir());
        let ctx = ToolContext::dial("sapphire-timer", "0.0.0", None, endpoint);
        TimerTools::new(ctx, Clients::new()).tool_router.list_all()
    }

    /// Zero-argument tools must advertise a top-level `type: object`, or
    /// Anthropic rejects the tool with
    /// `tools.<N>.custom.input_schema.type: Field required`.
    #[test]
    fn every_tool_input_schema_declares_object_type() {
        for tool in tools() {
            let schema = serde_json::to_value(&tool.input_schema).expect("schema to json");
            assert_eq!(
                schema.get("type").and_then(|t| t.as_str()),
                Some("object"),
                "tool {} has a non-object input schema: {schema}",
                tool.name
            );
        }
    }

    /// The timer's twelve, the framework's read-only seven, and none of the
    /// framework's raw file writes.
    #[test]
    fn the_timer_serves_its_tools_and_the_frameworks_read_only_ones() {
        let names: Vec<String> = tools().iter().map(|t| t.name.to_string()).collect();
        for expected in [
            "timer_start",
            "timer_start_preset",
            "preset_add",
            "log_list",
            "status",
        ] {
            assert!(
                names.iter().any(|n| n == expected),
                "missing {expected}: {names:?}"
            );
        }
        assert!(
            !names.iter().any(|n| n.starts_with("workspace_write")),
            "raw writes would bypass the timer's checks: {names:?}"
        );
        assert_eq!(names.len(), 12 + 7, "{names:?}");
    }
}
