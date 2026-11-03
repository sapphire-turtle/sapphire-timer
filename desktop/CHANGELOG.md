# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/sapphire-turtle/sapphire-timer/compare/v0.1.0...v0.2.0) - 2026-11-03

### Added

- *(desktop)* [**breaking**] package with cargo-packager instead of Velopack ([#34](https://github.com/sapphire-turtle/sapphire-timer/pull/34))
- *(desktop)* ship Sapphire Timer as a Velopack package
- [**breaking**] one desktop binary that runs the CLI too; headless, cli and server crates
- *(desktop)* list the timers, with a picked one's logs in the side pane

### Fixed

- name each edition's releases, so update installs its own file

### Other

- *(deps)* [**breaking**] sapphire-framework 0.24
- Merge pull request #21 from sapphire-turtle/docs/app-icon
- call the shared workflows from sapphire-turtle/.github@v1
- update to sapphire-framework 0.23
- the filled app icon
- add the app icon and show it in the README

### Changed

- **Breaking:** sapphire-framework 0.24. HTTP clients (`/mcp`) authenticate with API keys kept in the workspace (`<root>/.sapphire-timer/api_keys/`), managed with `sapphire-timer api-key list|add|retire|restore|rotate` (`external-device` is a hidden alias for this release). Each API key gets a timer of its own over HTTP, as each external device did. Every key has a profile: the workgroup's existing external-device tokens keep working but become **read-only**, so they may call only the reading tools (`timer_status`, `preset_list`, `log_list`, …) and can no longer start a timer; give such a client a new key, `sapphire-timer api-key add <name> --profile read-write`. Update `sapphire-bridge` together with the timer.
- **Breaking:** the `workgroup`, `device` and `embedding` subcommands are gone: use `sapphire-bridge workgroup|device|embedding`.
- Settings link to the Sapphire Bridge window for the workgroup, devices and embedding (sapphire-framework 0.24), and gain an API keys page.

## [0.1.0](https://github.com/sapphire-turtle/sapphire-timer/releases/tag/desktop-v0.1.0) - 2026-10-25

### Added

- *(desktop)* a desktop app on the framework's shell, Timers a placeholder

### Other

- release with release-plz
