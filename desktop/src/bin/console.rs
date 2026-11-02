//! The console shim the Windows package puts on `PATH` as `sapphire-timer.exe` (#25):
//! `sapphire-timer-desktop.exe` is built for the GUI subsystem, so a command typed in a
//! terminal would print nothing. This runs it with the same arguments and streams, waits,
//! and exits with its code (corundum-framework-package's `console::forward`).

#[cfg(windows)]
fn main() -> ! {
    sapphire_framework::gui::package::runtime::console::forward("sapphire-timer-desktop.exe")
}

#[cfg(not(windows))]
fn main() -> std::process::ExitCode {
    eprintln!(
        "sapphire-timer-console is the Windows package's command; run sapphire-timer-desktop"
    );
    std::process::ExitCode::FAILURE
}
