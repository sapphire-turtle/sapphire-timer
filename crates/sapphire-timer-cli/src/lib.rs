//! `sapphire-timer`: the timer's command line, server and MCP server.
//!
//! The framework's verbs (`serve`, `status`, `service`, `workspace`, `mcp`, …)
//! run the app server or talk to a running one.
//!
//! The modules are a library so the integration tests can drive them without a
//! process, and so both editions run the same commands ([`run`]): the headless
//! one (`sapphire-timer-headless`) and the desktop app, which runs them when
//! given a subcommand. Both put `sapphire-timer` on PATH.

pub mod cli;

use std::path::{Path, PathBuf};

use anyhow::Result;
use sapphire_timer_core::{AppKind, TIMER_CTX};
use sapphire_timer_server::{self as serve, TimerSettings};

pub use sapphire_timer_server::Edition;

use crate::cli::{Command, FrameworkCommand};

/// Run one command as `edition`, exiting the process with its code when a
/// framework verb fails with one.
pub fn run(command: Command, edition: Edition) -> Result<()> {
    match command {
        // `serve` is what the service manager starts; the framework restarts
        // it in place on `server.restart`. `mcp` among the rest: its stdout is
        // the MCP stream, and the framework's console log goes to stderr
        // (sapphire-framework#232).
        Command::Framework(command) => {
            let kind = match command {
                FrameworkCommand::Serve => AppKind::Server,
                _ => AppKind::Cli,
            };
            // Warned first: `dispatch` exits when the status is not good.
            if matches!(command, FrameworkCommand::Status) {
                warn_if_shadowed();
            }
            sapphire_timer_core::init_app_context(kind);
            let code = runtime()?.block_on(command.dispatch(
                &TIMER_CTX,
                TimerSettings,
                |settings| serve::build(settings, edition),
                serve::VERSION,
            ))?;
            if code != 0 {
                std::process::exit(code);
            }
            Ok(())
        }
    }
}

fn runtime() -> Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?)
}

/// The command both editions put on PATH.
const COMMAND: &str = "sapphire-timer";

/// Warn when the `sapphire-timer` on PATH is another executable than this
/// one: with both the headless edition and the desktop app installed, the one a shell or an
/// MCP client runs may not be the one the service was registered with.
fn warn_if_shadowed() {
    let Ok(this) = std::env::current_exe().and_then(std::fs::canonicalize) else {
        return;
    };
    let Some(on_path) = std::env::var_os("PATH").and_then(|path| find_on_path(&path)) else {
        return;
    };
    if std::fs::canonicalize(&on_path).is_ok_and(|found| found != this) {
        eprintln!(
            "warning: `{COMMAND}` on PATH is {}, not this executable ({}); \
             keep only one of the headless edition and the desktop app installed",
            on_path.display(),
            this.display(),
        );
    }
}

/// The first `sapphire-timer` executable in `path`, a `PATH` value.
fn find_on_path(path: &std::ffi::OsStr) -> Option<PathBuf> {
    let file = format!("{COMMAND}{}", std::env::consts::EXE_SUFFIX);
    std::env::split_paths(path)
        .map(|dir| dir.join(&file))
        .find(|candidate| Path::is_file(candidate))
}
