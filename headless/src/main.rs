//! `sapphire-timer`, the headless edition: the command line of
//! [`sapphire_timer_cli`] and nothing else. The desktop app's binary runs the
//! same commands.

use anyhow::Result;
use clap::Parser as _;
use sapphire_timer_cli::Edition;
use sapphire_timer_cli::cli::Cli;

fn main() -> Result<()> {
    sapphire_timer_cli::run(Cli::parse().command, Edition::Headless)
}
