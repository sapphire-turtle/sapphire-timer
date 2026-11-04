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
//!
//! With the `package` feature (the release workflow's build) it is a cargo-packager package
//! (src/package.rs): it updates itself, and its first start asks to keep the app server
//! running.

// In the package, a GUI binary on Windows: no console window behind the window or the
// service. The command line goes through the console shim (src/bin/console.rs).
#![cfg_attr(all(windows, feature = "package"), windows_subsystem = "windows")]

mod client;
#[cfg(feature = "package")]
mod package;
mod timers;

use clap::Parser;
use corundum_framework_platform::launcher::LauncherSpec;
use sapphire_framework::gui::app::{AppOptions, AppSetup};
use sapphire_framework::gui::client::AppIdentity;
use sapphire_framework::gui::{Destination, Settings, Shell, ShellApp, Side, eframe, egui};
use sapphire_framework::server::FrameworkCommand;
use sapphire_timer_cli::cli::Command;

use crate::client::TimerClient;
use crate::timers::TimersView;

/// Sapphire Timer in the OS's app menu, and the window's id and icon. As the package's
/// (`[package.metadata.packager]` in Cargo.toml): `org.sapphire-turtle.sapphire-timer` is its
/// identifier and `Utility` its Linux category.
const LAUNCHER: LauncherSpec = LauncherSpec {
    id: "sapphire-timer",
    name: "Sapphire Timer",
    description: "Preset timers with time logs",
    category: "Utility",
    icon_svg: include_bytes!("../../docs/icon.svg"),
    bundle_prefix: "org.sapphire-turtle",
};

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

/// Whether this start goes through the package's start: every start but `serve`, the
/// package's service (the start starts the services an update stopped, and that is this
/// process). The installer's hooks (`--corundum-package <hook>`) pass arguments clap
/// rejects, so this is decided before clap may exit on them.
#[cfg_attr(not(feature = "package"), allow(dead_code))]
fn runs_package_start(parsed: &Result<DesktopCli, clap::Error>) -> bool {
    !matches!(
        parsed,
        Ok(DesktopCli {
            command: Some(Command::Framework(FrameworkCommand::Serve)),
        })
    )
}

fn main() -> anyhow::Result<()> {
    let parsed = DesktopCli::try_parse();
    // The package's start: an install hook runs and exits here. The window's console first,
    // so the hooks log; a command installs its own.
    #[cfg(feature = "package")]
    if runs_package_start(&parsed) {
        if !matches!(parsed, Ok(DesktopCli { command: Some(_) })) {
            sapphire_framework::gui::app::install_logging(APP);
        }
        let _ = sapphire_framework::gui::package::runtime::start(&package::PACKAGE);
    }
    let cli = parsed.unwrap_or_else(|e| e.exit());
    match cli.command {
        // In the package, the package's updater replaces it, not the desktop release file.
        #[cfg(feature = "package")]
        Some(Command::Framework(FrameworkCommand::Update(args)))
            if corundum_framework_platform::package::current().is_some() =>
        {
            std::process::exit(package_update(&args))
        }
        Some(command) => sapphire_timer_cli::run(command, sapphire_timer_cli::Edition::Desktop),
        None => Ok(window()?),
    }
}

/// `update` in the package: `sapphire-bridge` first (unless `--no-bridge`), as the package's
/// own update ends this process, then the package through its updater. The exit code.
#[cfg(feature = "package")]
fn package_update(args: &sapphire_framework::server::AppUpdateArgs) -> i32 {
    let bridge = if args.no_bridge {
        0
    } else {
        match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime
                .block_on(sapphire_framework::server::update_bridge(&args.update))
                .unwrap_or_else(|err| {
                    eprintln!("sapphire-bridge: {err}");
                    1
                }),
            Err(err) => {
                eprintln!("sapphire-timer: {err}");
                1
            }
        }
    };
    let own = match package::APP_PACKAGE.updater() {
        Some(updater) => package::APP_PACKAGE.update(
            &updater,
            args.update.check,
            &mut std::io::stdout(),
            std::time::Duration::from_secs(5 * 60),
        ),
        None => 1,
    };
    own.max(bridge)
}

fn window() -> eframe::Result<()> {
    sapphire_framework::gui::app::run(app_options(), |setup: AppSetup<'_>| {
        let settings = Settings::new(setup.framework);
        // About's service row and updates, in the package.
        #[cfg(feature = "package")]
        let settings = settings.with_package_about(setup.package_about);
        let shell = Shell::new(
            vec![Destination::new(
                Screen::Timers,
                egui::include_image!("../assets/icons/timer.svg"),
                "Timers",
            )],
            settings,
        );
        let client = TimerClient::new(APP.app_name, APP.version, setup.endpoints.app);
        Box::new(App {
            shell,
            screens: Screens {
                timers: TimersView::new(setup.runtime, client, setup.repaint),
            },
        })
    })
}

/// What the window opens with: the package's icon and id, and in the package, the package.
fn app_options() -> AppOptions {
    let options = AppOptions::new(APP, "Sapphire Timer").launcher(LAUNCHER);
    #[cfg(feature = "package")]
    let options = options.package(package::APP_PACKAGE);
    options
}

#[cfg(test)]
mod tests {
    use super::*;

    // The installer's hooks start the binary with arguments clap does not know; the package's
    // start must see them before clap exits on them.
    #[test]
    fn package_start_runs_before_clap_rejects() {
        let parsed = |args: &[&str]| DesktopCli::try_parse_from(args);
        assert!(runs_package_start(&parsed(&[
            "x",
            "--corundum-package",
            "installed"
        ])));
        assert!(runs_package_start(&parsed(&["x", "--version"])));
        assert!(runs_package_start(&parsed(&["x"])));
        assert!(runs_package_start(&parsed(&["x", "status"])));
        assert!(!runs_package_start(&parsed(&["x", "serve"])));
    }

    // The window shows the package's icon even outside a package, and goes by the
    // package's bundle id, so the OS groups it with the installer's entry.
    #[test]
    fn the_window_has_the_packages_icon_and_id() {
        assert_eq!(LAUNCHER.id, "sapphire-timer");
        assert_eq!(LAUNCHER.name, "Sapphire Timer");
        assert_eq!(LAUNCHER.category, "Utility");
        assert_eq!(LAUNCHER.bundle_prefix, "org.sapphire-turtle");
        assert_eq!(
            LAUNCHER.icon_svg,
            include_bytes!("../../docs/icon.svg").as_slice()
        );
        assert!(corundum_framework_platform::icon::render_rgba(LAUNCHER.icon_svg, 64).is_ok());
    }
}
