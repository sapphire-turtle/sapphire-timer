//! One timer per client, held where the MCP is served.
//!
//! A timer belongs to whoever started it and is seen by no one else:
//!
//! - On **stdio** (`sapphire-timer mcp`) the client is the process that
//!   launched the subcommand, and the subcommand's process is its own, so the
//!   one timer here is that client's.
//! - On **HTTP** (`/mcp` on the app server) there is no session to tell
//!   clients apart — the 2026-07-28 protocol is stateless — but every request
//!   carries the external device's bearer token, and the framework's auth
//!   layer leaves the device it belongs to in the request. Each external
//!   device gets a timer of its own on the server.
//!
//! Nothing here is persisted. A timer's runs reach the workspace as logs,
//! written through the app server like any other change.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rmcp::model::Extensions;
use sapphire_framework::keys::Authenticated;
use sapphire_framework::mcp::ToolContext;
use sapphire_timer_core::{LogEntry, Timer};
use tokio::sync::mpsc;

use crate::methods::{LogPutParams, names};

/// Who a timer belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ClientKey {
    /// The stdio client: the one process this one serves.
    Local,
    /// An external device, by its id in the workgroup's ledger.
    Device(String),
    /// An HTTP request that no device was authenticated for — only a server
    /// that turned auth off for its tests serves one.
    Anonymous,
}

/// A client: the key its timer is held under, and the name its logs carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client {
    pub key: ClientKey,
    pub name: String,
}

impl Client {
    /// The client a request came from, by what its transport left in the
    /// request's extensions.
    ///
    /// rmcp hands an HTTP request's head to the handler as
    /// [`http::request::Parts`]; a stdio request has none.
    pub fn of(extensions: &Extensions) -> Client {
        let Some(parts) = extensions.get::<http::request::Parts>() else {
            return Client {
                key: ClientKey::Local,
                name: "local".to_owned(),
            };
        };
        match parts.extensions.get::<Authenticated>() {
            Some(who) => Client {
                key: ClientKey::Device(who.id.to_string()),
                name: who.name.clone(),
            },
            None => Client {
                key: ClientKey::Anonymous,
                name: "anonymous".to_owned(),
            },
        }
    }
}

/// Every client's timer, made on first use.
#[derive(Default)]
pub struct Clients {
    timers: Mutex<HashMap<ClientKey, Arc<Timer>>>,
}

impl Clients {
    pub fn new() -> Arc<Clients> {
        Arc::new(Clients::default())
    }

    /// `client`'s timer, made if it has none yet. Its runs' logs go to the
    /// app server through `ctx`.
    ///
    /// Must be called within a Tokio runtime: the logs are written by a task.
    pub fn timer(&self, client: &Client, ctx: &ToolContext) -> Arc<Timer> {
        let mut timers = self.timers.lock().unwrap_or_else(|e| e.into_inner());
        Arc::clone(
            timers.entry(client.key.clone()).or_insert_with(|| {
                Timer::new(Some(client.name.clone()), Some(journal(ctx.clone())))
            }),
        )
    }

    /// `client`'s timer, if it has one.
    pub fn get(&self, client: &Client) -> Option<Arc<Timer>> {
        let timers = self.timers.lock().unwrap_or_else(|e| e.into_inner());
        timers.get(&client.key).cloned()
    }
}

/// A journal that writes each log to the app server, in the order the
/// changes happened.
///
/// The timer calls its journal with the slot locked, so the journal only
/// queues; one task per timer drains the queue, which keeps a step's log from
/// overtaking the run's last. The task ends with the timer, once the queue
/// is empty.
fn journal(ctx: ToolContext) -> sapphire_timer_core::engine::Journal {
    let (tx, mut rx) = mpsc::unbounded_channel::<LogEntry>();
    tokio::spawn(async move {
        while let Some(log) = rx.recv().await {
            let id = log.id;
            if let Err(e) = ctx
                .call::<_, serde_json::Value>(names::LOG_PUT, LogPutParams { log })
                .await
            {
                // The run goes on: a log the server could not take is the
                // log's loss, not the timer's.
                tracing::warn!(%id, "could not write the run's log: {e}");
            }
        }
    });
    Box::new(move |log| {
        let _ = tx.send(log);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stdio_request_is_the_local_client() {
        let client = Client::of(&Extensions::new());
        assert_eq!(client.key, ClientKey::Local);
    }

    #[test]
    fn an_http_request_is_its_external_device() {
        let (mut parts, ()) = http::Request::new(()).into_parts();
        parts.extensions.insert(Authenticated {
            id: "012atvw".parse().unwrap(),
            name: "agent".into(),
        });
        let mut extensions = Extensions::new();
        extensions.insert(parts);
        let client = Client::of(&extensions);
        assert_eq!(client.key, ClientKey::Device("012atvw".into()));
        assert_eq!(client.name, "agent");

        let (bare, ()) = http::Request::new(()).into_parts();
        let mut extensions = Extensions::new();
        extensions.insert(bare);
        assert_eq!(Client::of(&extensions).key, ClientKey::Anonymous);
    }
}
