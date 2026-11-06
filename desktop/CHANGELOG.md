# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0](https://github.com/sapphire-turtle/sapphire-timer/compare/v0.2.1...v0.3.0) - 2026-11-06

### Added

- On macOS, the app bundle put in `~/Applications` when Sapphire Timer is added to the app menu has the mark on the brand's tile, as the package's icns, rather than the bare mark on the system's plate (corundum-framework 0.11).

### Changed

- **Breaking:** sapphire-framework 0.28 and corundum-framework 0.11. `update` installs only a release file signed with sapphire-turtle's key: it fetches the file's `<file>.sig` from the same release and refuses a file without one, or whose signature does not match (sapphire-turtle/sapphire-framework#402).
- Each release file — the bare binaries for servers, the install script and `cargo binstall` — comes with its `<file>.sig`, made with sapphire-turtle's minisign key, which `update` checks (sapphire-turtle/sapphire-framework#402).

### Fixed

- On Windows, `sapphire-timer-desktop.exe` carries the app icon, in the brand colour, so Explorer and the taskbar show Sapphire Timer's icon rather than a generic one.

## [0.2.1](https://github.com/sapphire-turtle/sapphire-timer/compare/v0.2.0...v0.2.1) - 2026-11-04

### Fixed

- On Windows, keeping the app server running (the first start's offer, or `service install`) no longer fails with "access is denied": the service is now a logon task for the installing user, which needs no administrator (sapphire-framework 0.27, corundum 0.9, sapphire-turtle/corundum-framework#105).
- On Windows, applying an update from the app's window installs it and starts the app again; before, the app only closed and was not updated (sapphire-framework 0.27.1, corundum 0.9.1, sapphire-turtle/corundum-framework#109). An app at v0.2.0 or earlier still hits this when it applies this update: install v0.2.1 by hand once, from its `_x64-setup.exe` ([#35](https://github.com/sapphire-turtle/sapphire-timer/pull/35))

## [0.2.0](https://github.com/sapphire-turtle/sapphire-timer/compare/v0.1.0...v0.2.0) - 2026-11-03

### Added

- **Breaking:** Sapphire Timer is distributed as an installer per OS, made with cargo-packager ([#34](https://github.com/sapphire-turtle/sapphire-timer/pull/34)): `sapphire-timer-desktop_<version>_x64-setup.exe`, `_aarch64.dmg` and `_x86_64.AppImage`. The installed app updates itself from the latest release, checking each update's signature first.
- **Breaking:** one desktop binary that runs the CLI too; headless, cli and server crates
- list the timers, with a picked one's logs in the side pane
- the app icon

### Changed

- **Breaking:** sapphire-framework 0.24. HTTP clients (`/mcp`) authenticate with API keys kept in the workspace (`<root>/.sapphire-timer/api_keys/`), managed with `sapphire-timer api-key list|add|retire|restore|rotate` (`external-device` is a hidden alias for this release). Each API key gets a timer of its own over HTTP, as each external device did. Every key has a profile: the workgroup's existing external-device tokens keep working but become **read-only**, so they may call only the reading tools (`timer_status`, `preset_list`, `log_list`, …) and can no longer start a timer; give such a client a new key, `sapphire-timer api-key add <name> --profile read-write`. Update `sapphire-bridge` together with the timer.
- **Breaking:** the `workgroup`, `device` and `embedding` subcommands are gone: use `sapphire-bridge workgroup|device|embedding`.
- Settings link to the Sapphire Bridge window for the workgroup, devices and embedding (sapphire-framework 0.24), and gain an API keys page.
- **Breaking:** sapphire-framework 0.26: `workspace init` and `select` without a directory use the app's default workspace, `<data dir>/workspaces/default`, not the current directory; pass `.` for the old behaviour.

### Fixed

- name each edition's releases, so update installs its own file

## [0.1.0](https://github.com/sapphire-turtle/sapphire-timer/releases/tag/desktop-v0.1.0) - 2026-10-25

### Added

- *(desktop)* a desktop app on the framework's shell, Timers a placeholder

### Other

- release with release-plz
