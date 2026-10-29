// SPDX-License-Identifier: GPL-3.0-or-later

//! The desktop's link to the timer's methods on the running app server.
//!
//! The framework's `FrameworkClient` covers the framework's half (status,
//! workspaces, sync); the timer's own `sapphire-timer.*` methods go over a
//! connection of this app's own. It is dialled on the first call and again
//! after it drops — the server may start after the window opens, or restart
//! while it is open.

use std::sync::Arc;

use sapphire_framework::backend::protocol::API_VERSION;
use sapphire_framework::ipc::{self, Client, ClientInfo, Endpoint};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// What this client reports in the IPC handshake.
const CLIENT_KIND: &str = "desktop";

pub struct TimerClient {
    app: &'static str,
    version: &'static str,
    endpoint: Endpoint,
    held: tokio::sync::Mutex<Option<Arc<Client>>>,
}

impl TimerClient {
    pub fn new(app: &'static str, version: &'static str, endpoint: Endpoint) -> TimerClient {
        TimerClient {
            app,
            version,
            endpoint,
            held: tokio::sync::Mutex::new(None),
        }
    }

    /// Call `method` and return its text: JSON for a read. The error is a
    /// message to show as it is.
    pub async fn call(&self, method: &str, params: impl Serialize) -> Result<String, String> {
        let client = self.client().await?;
        match client.call::<_, String>(method, params).await {
            Ok(text) => Ok(text),
            Err(ipc::Error::Rpc(rpc)) => Err(rpc.message),
            Err(ipc::Error::Closed | ipc::Error::Io(_)) => {
                self.forget(&client).await;
                Err(format!(
                    "the connection to the {} server was lost; try again",
                    self.app
                ))
            }
            Err(err) => Err(err.to_string()),
        }
    }

    /// Call a read method and parse the JSON it answers with.
    pub async fn read<R: DeserializeOwned>(
        &self,
        method: &str,
        params: impl Serialize,
    ) -> Result<R, String> {
        let text = self.call(method, params).await?;
        serde_json::from_str(&text).map_err(|e| format!("unexpected answer to {method}: {e}"))
    }

    async fn client(&self) -> Result<Arc<Client>, String> {
        let mut held = self.held.lock().await;
        if let Some(client) = held.as_ref().filter(|c| !c.is_closed()) {
            return Ok(Arc::clone(client));
        }
        let info = ClientInfo {
            kind: CLIENT_KIND.to_owned(),
            version: self.version.to_owned(),
            api: API_VERSION,
            app_api: Some(sapphire_timer_mcp::APP_API),
            pid: std::process::id(),
        };
        let (client, _) = ipc::connect_or_absent(&self.endpoint, self.app, info)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| {
                format!(
                    "no {} server is running; start it with `{} serve`",
                    self.app, self.app
                )
            })?;
        let client = Arc::new(client);
        *held = Some(Arc::clone(&client));
        Ok(client)
    }

    async fn forget(&self, lost: &Arc<Client>) {
        let mut held = self.held.lock().await;
        if held.as_ref().is_some_and(|h| Arc::ptr_eq(h, lost)) {
            *held = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use sapphire_framework::backend::protocol as proto;
    use sapphire_timer_core::{AppKind, LogEntry, Outcome, Preset, Step, TIMER_CTX};
    use sapphire_timer_mcp::methods::{
        EmptyParams, LogPutParams, LogQueryParams, PresetAddParams, names,
    };

    use super::*;

    /// The desktop's reads against a running timer server: that what the
    /// methods answer parses into the types the view shows.
    ///
    /// One test: `TIMER_CTX`'s directories are first-writer-wins per process.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_desktop_reads_the_servers_presets_and_logs() {
        let tmp = tempfile::tempdir().unwrap();
        for cat in ["CACHE", "DATA", "CONFIG"] {
            // SAFETY: the only test in this binary that touches the
            // environment, before the server starts.
            unsafe {
                std::env::set_var(
                    format!("SAPPHIRE_TIMER_{cat}_DIR"),
                    tmp.path().join(cat.to_lowercase()),
                )
            };
        }
        sapphire_timer_core::init_app_context(AppKind::Server);
        std::fs::create_dir_all(tmp.path().join("run")).unwrap();
        let endpoint = Endpoint::in_dir(TIMER_CTX.app_name, tmp.path().join("run"));

        let client = TimerClient::new(TIMER_CTX.app_name, "0.0.0", endpoint.clone());
        let err = client
            .call(names::PRESET_LIST, EmptyParams {})
            .await
            .unwrap_err();
        assert!(
            err.contains("no sapphire-timer server is running"),
            "got: {err}"
        );

        let server = sapphire_timer_server::server().endpoint(endpoint.clone());
        let task = tokio::spawn(async move { server.run().await.unwrap() });
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while !ipc::probe(&endpoint).await.unwrap() {
            assert!(
                tokio::time::Instant::now() < deadline,
                "the server never listened"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        let data = tmp.path().join("timer");
        std::fs::create_dir_all(&data).unwrap();
        let _: proto::WorkspaceInitResult = client
            .client()
            .await
            .unwrap()
            .call(
                proto::WORKSPACE_INIT,
                proto::WorkspaceInitParams { dir: data.clone() },
            )
            .await
            .unwrap();

        let preset: Preset = client
            .read(
                names::PRESET_ADD,
                PresetAddParams {
                    name: "Pomodoro".into(),
                    steps: vec![Step {
                        label: "Focus".into(),
                        minutes: 25.0,
                    }],
                    cycles: Some(4),
                    description: None,
                },
            )
            .await
            .unwrap();
        let started_at = chrono::DateTime::parse_from_rfc3339("2026-10-28T09:00:00+09:00").unwrap();
        let log = LogEntry {
            id: "012atvw".parse().unwrap(),
            label: "Pomodoro".into(),
            preset: Some(preset.name.clone()),
            preset_id: Some(preset.id),
            client: Some("local".into()),
            started_at,
            ended_at: None,
            outcome: Outcome::Running,
            cycles: 4,
            steps: Vec::new(),
            comment: Some("the side pane".into()),
        };
        client
            .call(names::LOG_PUT, LogPutParams { log: log.clone() })
            .await
            .unwrap();

        let presets: Vec<Preset> = client
            .read(names::PRESET_LIST, EmptyParams {})
            .await
            .expect("presets parse");
        assert_eq!(presets, vec![preset]);
        let logs: Vec<LogEntry> = client
            .read(
                names::LOG_LIST,
                LogQueryParams {
                    from: None,
                    to: None,
                },
            )
            .await
            .expect("logs parse");
        assert_eq!(logs, vec![log]);

        let _: serde_json::Value = client
            .client()
            .await
            .unwrap()
            .call(ipc::SHUTDOWN_METHOD, serde_json::json!({}))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .expect("the server stopped")
            .unwrap();
    }
}
