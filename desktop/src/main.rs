// SPDX-License-Identifier: GPL-3.0-or-later

//! sapphire-timer's desktop app: its screens on the framework's rail, and the
//! framework's Settings at the rail's foot.
//!
//! A pure client: it talks to the service-managed `sapphire-timer serve` and
//! `sapphire-bridge` over IPC and never starts either itself. Closing the
//! window stops nothing. The Timers screen is a placeholder for now: the
//! timers run from the CLI and over MCP.

use sapphire_framework::gui::app::{AppOptions, AppSetup};
use sapphire_framework::gui::client::AppIdentity;
use sapphire_framework::gui::{Destination, Settings, Shell, eframe, egui};

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
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.shell.ui(ui, |ui, screen| match screen {
            Screen::Timers => timers_ui(ui),
        });
    }
}

/// The Timers screen, until it has the timers.
fn timers_ui(ui: &mut egui::Ui) {
    egui::CentralPanel::default().show(ui, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() / 3.0);
            ui.heading("Timers");
            ui.label("Not here yet: run the timers with `sapphire-timer` or over MCP.");
        });
    });
}

fn main() -> eframe::Result<()> {
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
            Box::new(App { shell })
        },
    )
}
