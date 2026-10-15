//! The app server: what `sapphire-timer serve` (and a bare `sapphire-timer`)
//! runs.
//!
//! The framework's [`AppServer`] owns the IPC socket, the current workspace
//! (`workspace init|select|sync|show|move|remove|map`), signals and the log
//! file. This module adds what is the timer's own:
//!
//! - **sync**, through the host's `sapphire-bridge`. A bridge that is not
//!   running is not fatal: the server serves its workspace without sync and
//!   picks the bridge up once it starts, or again after it restarts.
//! - **the timer's methods** (`sapphire-timer.*`): presets and logs, which
//!   its MCP tools call. The framework serves those tools on stdio
//!   (`sapphire-timer mcp`) and, when `server.toml` turns it on, at `/mcp` on
//!   the server's HTTP listener — where each external device also gets a
//!   running timer of its own (`sapphire_timer_mcp::clients`).

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use sapphire_framework::server::{
    AppServer, SERVER_CONFIG_FILE, ServerConfig, StatusRow, WorkspaceHost,
};
use sapphire_timer_core::{TIMER_CTX, Timers};

/// This binary's version, reported in every handshake.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Where the HTTP listener binds unless `server.toml`'s `[http] listen` says
/// otherwise: loopback, on the port after sapphire-tally's.
pub const DEFAULT_HTTP_ADDR: &str = "127.0.0.1:3175";

/// The server the framework's one-shot verbs dispatch against: the timer's
/// MCP tools and API version, nothing mounted.
///
/// `status`, `service`, `workspace`, `workgroup`, `device`, `embedding` and
/// `external-device` read only the app name and the service spec from it and
/// talk to a *running* server (or the bridge) over IPC themselves; `mcp`
/// serves the timer's tools on stdio, dialling that server. Opening a bridge
/// here would make `status` on a host without one fail with a bridge error
/// instead of the framework's "no server is running".
pub fn build_oneshot() -> AppServer {
    AppServer::new(&TIMER_CTX, VERSION)
        .app_api(sapphire_timer_mcp::APP_API)
        .mcp(sapphire_timer_mcp::service())
}

/// The timer's server, without sync: the framework's server with the timer's
/// methods, its MCP tools, and its HTTP port.
///
/// [`run`] adds the bridge; a test runs this as it is.
pub fn build() -> AppServer {
    let server = build_oneshot().http_listen(default_http_addr());
    let host = Arc::clone(server.host());
    server.extend(move |router| {
        sapphire_timer_mcp::register(router, Arc::new(move || current_timers(&host)))
    })
}

fn default_http_addr() -> SocketAddr {
    DEFAULT_HTTP_ADDR
        .parse()
        .expect("the default address parses")
}

/// The data directory the server's current workspace holds, or why there is
/// none.
///
/// Opened per call: a [`Timers`] is only its root, and `workspace select` can
/// switch the root between two calls.
fn current_timers(host: &WorkspaceHost) -> io::Result<Timers> {
    let root = host.root().ok_or_else(|| {
        io::Error::other(
            "this server has no workspace selected; run \
             `sapphire-timer workspace init <dir>` or `sapphire-timer workspace select <dir>`",
        )
    })?;
    Timers::open(root).map_err(io::Error::other)
}

/// Refuse a configuration that would serve 403 to every `/mcp` client.
///
/// rmcp's `Host` allowlist is loopback-only unless widened. Binding beyond
/// loopback without naming a host is not dangerous — it is useless, and it
/// presents as a client bug. Failing here turns that into an obvious
/// configuration error.
pub fn check_exposure(addr: SocketAddr, allowed_hosts: &[String]) -> anyhow::Result<()> {
    if addr.ip().is_loopback() {
        return Ok(());
    }
    if allowed_hosts.iter().any(|h| !h.trim().is_empty()) {
        return Ok(());
    }
    anyhow::bail!(
        "binding to {addr} without allowed_hosts would refuse every request: \
         rmcp's Host allowlist stays loopback-only, so a client reaching this \
         server by any other name gets 403. List each name clients use in \
         [http] allowed_hosts of {SERVER_CONFIG_FILE}, or bind to loopback."
    )
}

/// Run the server until SIGTERM or SIGINT.
pub async fn run() -> anyhow::Result<()> {
    // The framework reads `server.toml` again when it starts; this read is
    // for what the timer adds on top: the exposure check and the status row.
    let config = ServerConfig::load_for_app(&TIMER_CTX)?;
    let http_addr = config.http.listen.unwrap_or_else(default_http_addr);
    if config.mcp.http {
        check_exposure(http_addr, &config.http.allowed_hosts)?;
    }

    // Sync through the host's bridge. The framework connects, and connects
    // again whenever the bridge restarts; a bridge that is not running yet
    // stops nothing but sync. Any other failure — a bridge of another
    // version, say — stops the start, in `run`.
    let server = build().sync_via_bridge()?;

    let mcp_row = if config.mcp.http {
        format!("http://{http_addr}/mcp")
    } else {
        "off".to_owned()
    };
    let server = server.status_rows(Arc::new(move || {
        vec![StatusRow {
            name: "mcp".to_owned(),
            value: mcp_row.clone(),
        }]
    }));

    Ok(server.run().await?)
}
