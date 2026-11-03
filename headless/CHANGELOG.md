# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/sapphire-turtle/sapphire-timer/releases/tag/v0.2.0) - 2026-11-03

### Added

- **Breaking:** one desktop binary that runs the CLI too; headless, cli and server crates

### Changed

- **Breaking:** sapphire-framework 0.24. HTTP clients (`/mcp`) authenticate with API keys kept in the workspace (`<root>/.sapphire-timer/api_keys/`), managed with `sapphire-timer api-key list|add|retire|restore|rotate` (`external-device` is a hidden alias for this release). Each API key gets a timer of its own over HTTP, as each external device did. Every key has a profile: the workgroup's existing external-device tokens keep working but become **read-only**, so they may call only the reading tools (`timer_status`, `preset_list`, `log_list`, …) and can no longer start a timer; give such a client a new key, `sapphire-timer api-key add <name> --profile read-write`. Update `sapphire-bridge` together with the timer.
- **Breaking:** the `workgroup`, `device` and `embedding` subcommands are gone: use `sapphire-bridge workgroup|device|embedding`.
- **Breaking:** sapphire-framework 0.26: `workspace init` and `select` without a directory use the app's default workspace, `<data dir>/workspaces/default`, not the current directory; pass `.` for the old behaviour.

### Fixed

- name each edition's releases, so update installs its own file

## [0.1.0](https://github.com/sapphire-turtle/sapphire-timer/releases/tag/v0.1.0) - 2026-10-25

### Added

- *(desktop)* a desktop app on the framework's shell, Timers a placeholder
- [**breaking**] rebuild sapphire-timer on sapphire-framework 0.20, with timers over MCP
- preset-based timer on sapphire-framework

### Other

- release with release-plz
- Merge pull request #17 from sapphire-turtle/docs/mcp-any-client
- update to sapphire-framework 0.22
- note that any MCP client works, not only sapphire-agent
- unify the tagline — built on sapphire-framework, human-agent collaboration
