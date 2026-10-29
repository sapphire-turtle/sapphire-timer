// SPDX-License-Identifier: GPL-3.0-or-later

//! sapphire-timer's desktop app: its screens on the framework's rail, and the
//! framework's Settings at the rail's foot — and, given a subcommand, the CLI.
//!
//! This one binary is a superset of the CLI's: bare, it opens the window;
//! with `serve`, `mcp`, `status`, … it runs the CLI's own code
//! (`sapphire_timer_cli::run`), so the service manager and MCP clients can
//! start it as they start `sapphire-timer`.
//!
//! The window is a pure client: it talks to the service-managed
//! `sapphire-timer serve` and `sapphire-bridge` over IPC and never starts
//! either itself. Closing the window stops nothing. The Timers screen lists
//! the presets and, in the side pane, the logs of the one picked; the timers
//! still run from the CLI and over MCP.

mod client;
mod timers;

use clap::Parser;
use sapphire_framework::gui::app::{AppOptions, AppSetup};
use sapphire_framework::gui::client::AppIdentity;
use sapphire_framework::gui::{Destination, Settings, Shell, ShellApp, Side, eframe, egui};
use sapphire_timer_cli::cli::Command;

use crate::client::TimerClient;
use crate::timers::TimersView;

const APP: AppIdentity = AppIdentity {
    app_name: "sapphire-timer",
    version: env!("CARGO_PKG_VERSION"),
};

/// The app's own screens, on the rail.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Timers,
}

struct App {
    shell: Shell<Screen>,
    screens: Screens,
}

/// What the shell draws: the screens and the side pane.
struct Screens {
    timers: TimersView,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.screens.timers.poll(ui.ctx());
        self.shell.show(ui, &mut self.screens);
    }
}

impl ShellApp<Screen> for Screens {
    fn screen_ui(&mut self, ui: &mut egui::Ui, screen: Screen, side: &mut Side) {
        match screen {
            Screen::Timers => self.timers.ui(ui, side),
        }
    }

    fn side_ui(&mut self, ui: &mut egui::Ui, _screen: Option<Screen>, _side: &mut Side) {
        self.timers.side_ui(ui);
    }
}

/// The CLI's command line, with the subcommand optional: none opens the window.
#[derive(Parser)]
#[command(
    name = "sapphire-timer",
    about = "Pomodoro-style timer with presets and time logs: the desktop app, or the CLI's commands",
    version
)]
struct DesktopCli {
    #[command(subcommand)]
    command: Option<Command>,
}

fn main() -> anyhow::Result<()> {
    match DesktopCli::parse().command {
        Some(command) => sapphire_timer_cli::run(command),
        None => Ok(window()?),
    }
}

fn window() -> eframe::Result<()> {
    sapphire_framework::gui::app::run(
        AppOptions::new(APP, "Sapphire Timer"),
        |setup: AppSetup<'_>| {
            let shell = Shell::new(
                vec![Destination::new(
                    Screen::Timers,
                    egui::include_image!("../assets/icons/timer.svg"),
                    "Timers",
                )],
                Settings::new(setup.framework),
            );
            let client = TimerClient::new(APP.app_name, APP.version, setup.endpoints.app);
            Box::new(App {
                shell,
                screens: Screens {
                    timers: TimersView::new(setup.runtime, client, setup.repaint),
                },
            })
        },
    )
}
