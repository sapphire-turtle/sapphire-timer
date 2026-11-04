# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.1](https://github.com/sapphire-turtle/sapphire-timer/compare/v0.2.0...v0.2.1) - 2026-11-04

### Fixed

- *(deps)* sapphire-framework 0.27.1 and corundum 0.9.1 (Windows service install) ([#35](https://github.com/sapphire-turtle/sapphire-timer/pull/35))

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
