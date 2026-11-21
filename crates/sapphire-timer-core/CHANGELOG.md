# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.1](https://github.com/sapphire-turtle/sapphire-timer/compare/sapphire-timer-core-v0.3.0...sapphire-timer-core-v0.3.1) - 2026-11-21

### Other

- update Cargo.toml dependencies

## [0.3.0](https://github.com/sapphire-turtle/sapphire-timer/compare/sapphire-timer-core-v0.2.2...sapphire-timer-core-v0.3.0) - 2026-11-20

### Breaking

- Times are `jiff::Timestamp`, not chrono's `DateTime<FixedOffset>`: chrono is unmaintained (RUSTSEC-2026-0543). `LogEntry`, `StepLog`, `Run` and `Event` change type; `time` has the helpers (`now`, `local`, `local_date`, `parse`, `format`).

### Changed

- Logs are written as before, in this host's offset (`2026-10-28T09:00:00+09:00`), and releases on chrono still read them. A time without an offset is read in this host's time zone, where it made the log unreadable before.
- `Timers::list_logs` takes a run's date in this host's time zone, not in the offset it was recorded in.
- sapphire-framework 0.29, corundum-framework 0.13.

## [0.2.2](https://github.com/sapphire-turtle/sapphire-timer/compare/sapphire-timer-core-v0.2.1...sapphire-timer-core-v0.2.2) - 2026-11-06

### Changed

- sapphire-framework 0.28 (sapphire-turtle/sapphire-framework#402).

## [0.2.1](https://github.com/sapphire-turtle/sapphire-timer/compare/sapphire-timer-core-v0.2.0...sapphire-timer-core-v0.2.1) - 2026-11-04

### Other

- update Cargo.toml dependencies

## [0.2.0](https://github.com/sapphire-turtle/sapphire-timer/compare/sapphire-timer-core-v0.1.0...sapphire-timer-core-v0.2.0) - 2026-11-03

### Changed

- **Breaking:** sapphire-framework 0.26

## [0.1.0](https://github.com/sapphire-turtle/sapphire-timer/releases/tag/sapphire-timer-core-v0.1.0) - 2026-10-25

### Added

- [**breaking**] rebuild sapphire-timer on sapphire-framework 0.20, with timers over MCP

### Other

- release with release-plz
