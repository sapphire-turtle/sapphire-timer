//! The app server: what `sapphire-timer serve` runs.
//!
//! The framework's [`AppServer`] owns the IPC socket, the current workspace
//! (`workspace init|select|sync|show|move|remove`), signals and the log
//! file. This module adds what is the timer's own:
//!
//! - **sync**, through the host's `sapphire-bridge`. A bridge that is not
//!   running is not fatal: the server serves its workspace without sync and
//!   picks the bridge up once it starts, or again after it restarts.
//! - **the timer's methods** (`sapphire-timer.*`): presets and logs, which
//!   its MCP tools call. The framework serves those tools on stdio
//!   (`sapphire-timer mcp`) and, when `config.toml` turns it on
//!   (`[http.mcp] enabled`), at `/mcp` on the server's HTTP listener — where
//!   each external device also gets a running timer of its own
//!   (`sapphire_timer_mcp::clients`).

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use sapphire_framework::server::{
    AppFiles, AppServer, AppSettings, FieldError, SERVER_CONFIG_FILE, ServerConfig, SettingDef,
    Settings, Staged, StatusRow, WorkspaceHost,
};
use sapphire_timer_core::{TIMER_CTX, Timers};

/// This binary's version, reported in every handshake.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Where the HTTP listener binds unless `config.toml`'s `[http] listen` says
/// otherwise: loopback, on the port after sapphire-tally's.
pub const DEFAULT_HTTP_ADDR: &str = "127.0.0.1:3175";

/// The timer's server, without sync: the framework's server with the timer's
/// methods, its MCP tools, and its HTTP port.
///
/// [`build`] adds the bridge; a test runs this as it is.
pub fn server() -> AppServer {
    let server = AppServer::new(&TIMER_CTX, VERSION)
        .app_api(sapphire_timer_mcp::APP_API)
        .mcp(sapphire_timer_mcp::service())
        .http_listen(default_http_addr());
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

/// The timer's settings owner: no keys of its own yet, and the exposure check
/// on the framework's `[http]` table.
///
/// The framework runs [`validate`](AppSettings::validate) before every start —
/// the first and each in-place restart — so a configuration that would serve
/// 403 to every `/mcp` client stops the server rather than starting it.
#[derive(Clone)]
pub struct TimerSettings;

impl AppSettings for TimerSettings {
    fn defs(&self) -> Vec<SettingDef> {
        Vec::new()
    }

    fn validate(&self, files: &AppFiles, staged: &Staged) -> Result<(), Vec<FieldError>> {
        // A file that cannot be read is the framework's to report.
        let Ok(Some(text)) = staged.read(files.device()) else {
            return Ok(());
        };
        check_device_config(&text).map_err(|err| {
            vec![FieldError {
                key: Some("http.allowed_hosts".to_owned()),
                file: Some(files.device().to_owned()),
                message: err.to_string(),
            }]
        })
    }
}

/// [`check_exposure`] on the device's `config.toml` text, when it serves `/mcp`
/// over HTTP.
///
/// A file that does not parse, or a value of the wrong kind, is the
/// framework's to report; this only adds what it does not check.
pub fn check_device_config(text: &str) -> anyhow::Result<()> {
    let Ok(table) = text.parse::<toml::Table>() else {
        return Ok(());
    };
    let http = table.get("http").and_then(toml::Value::as_table);
    let get = |key: &str| http.and_then(|http| http.get(key));
    let mcp_over_http = get("mcp")
        .and_then(|mcp| mcp.get("enabled"))
        .and_then(toml::Value::as_bool)
        .unwrap_or(false);
    if !mcp_over_http {
        return Ok(());
    }
    let addr = match get("listen").and_then(toml::Value::as_str) {
        Some(listen) => match listen.parse() {
            Ok(addr) => addr,
            Err(_) => return Ok(()),
        },
        None => default_http_addr(),
    };
    let allowed_hosts: Vec<String> = get("allowed_hosts")
        .and_then(toml::Value::as_array)
        .map(|hosts| {
            hosts
                .iter()
                .filter_map(|h| h.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    check_exposure(addr, &allowed_hosts)
}

/// The server `serve` runs, from the settings it starts with: [`server`] with
/// sync and the timer's status row.
///
/// The framework's other verbs build it too — `service` for its spec, `mcp`
/// for its tools — but only `serve` runs it, so nothing here reaches the
/// bridge until then.
pub fn build(settings: &Settings) -> AppServer {
    // The framework reads the same settings for the listener; this read is
    // for the status row. A value it cannot take stops the start there.
    let config = ServerConfig::from_settings(settings).unwrap_or_default();
    let http_addr = config.http.listen.unwrap_or_else(default_http_addr);
    let mcp_row = if config.http.mcp.enabled {
        format!("http://{http_addr}/mcp")
    } else {
        "off".to_owned()
    };

    // Sync through the host's bridge. The framework connects, and connects
    // again whenever the bridge restarts; a bridge that is not running yet
    // stops nothing but sync. Any other failure — a bridge of another
    // version, say — stops the start, in `run`. Building the runtime fails
    // only when the OS cannot name this executable.
    let server = server()
        .sync_via_bridge()
        .expect("the running executable's path is known");

    server.status_rows(Arc::new(move || {
        vec![StatusRow {
            name: "mcp".to_owned(),
            value: mcp_row.clone(),
        }]
    }))
}
