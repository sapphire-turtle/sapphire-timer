use anyhow::Result;
use clap::Parser as _;
use sapphire_timer::cli::{Cli, Command, FrameworkCommand};
use sapphire_timer::serve::{self, TimerSettings};
use sapphire_timer_core::{AppKind, TIMER_CTX};

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        // `serve` is what the service manager starts; the framework restarts
        // it in place on `server.restart`. `mcp` among the rest: its stdout is
        // the MCP stream, and the framework's console log goes to stderr
        // (sapphire-framework#232).
        Command::Framework(command) => {
            let kind = match command {
                FrameworkCommand::Serve => AppKind::Server,
                _ => AppKind::Cli,
            };
            sapphire_timer_core::init_app_context(kind);
            let code = runtime()?.block_on(command.dispatch(
                &TIMER_CTX,
                TimerSettings,
                serve::build,
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
