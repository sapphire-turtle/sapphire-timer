use clap::{Parser, Subcommand};

pub use sapphire_framework::server::FrameworkCommand;

/// The command line of the one binary this app ships.
///
/// The framework's verbs sit at the top level, where the timer's own verbs
/// will join them. A bare invocation prints the help; the service manager
/// starts the binary with `serve`.
#[derive(Parser)]
#[command(
    name = "sapphire-timer",
    arg_required_else_help = true,
    about = "Pomodoro-style timer with presets and time logs",
    long_about = None,
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// The framework's verbs: serve, status, service, workspace, workgroup,
    /// device, embedding, external-device, mcp.
    #[command(flatten)]
    Framework(FrameworkCommand),
}
