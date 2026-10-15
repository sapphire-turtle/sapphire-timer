use anyhow::Result;
use clap::Parser as _;
use sapphire_timer::cli::{Cli, Command, FrameworkCommand};
use sapphire_timer::serve;
use sapphire_timer_core::AppKind;

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        // What the service manager starts.
        Command::Framework(FrameworkCommand::Serve) => {
            sapphire_timer_core::init_app_context(AppKind::Server);
            runtime()?.block_on(serve::run())
        }
        // `mcp` among them: its stdout is the MCP stream, and the
        // framework's console log goes to stderr (sapphire-framework#232).
        Command::Framework(command) => {
            sapphire_timer_core::init_app_context(AppKind::Cli);
            let code =
                runtime()?.block_on(command.dispatch(serve::build_oneshot(), serve::VERSION))?;
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
