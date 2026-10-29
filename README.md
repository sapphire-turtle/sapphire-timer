# sapphire-timer

A preset-based timer built on [sapphire-framework](https://github.com/sapphire-turtle/sapphire-framework) —
file-based, local-first, made for human-agent collaboration. Presets and time logs are Markdown files with
YAML front matter in a synced, searchable workspace; timers are driven over MCP, by an agent or any other client.

- **Presets** (`presets/<id>.md`): a named sequence of steps (e.g. Focus 25 min / Break 5 min)
  repeated for a number of cycles. The body is the preset's description.
- **Logs** (`logs/<id>.md`): one file per run — the steps as they ran and how the run ended
  (`completed`, `cancelled`, `replaced`), for looking back on later. The body is the run's comment: what the time
  went on, given when the timer starts or added later with `log_comment`.

```markdown
---
id: 012atvw
name: Pomodoro
cycles: 4
steps:
- label: Focus
  minutes: 25.0
- label: Break
  minutes: 5.0
---

One task, no context switching.
```

- **Timers** are not persisted and not synced. Each client has exactly one, seen by no one else:
  - over stdio (`sapphire-timer mcp`), the process that launched the subcommand;
  - over HTTP (`/mcp` on the app server), each external device, identified by its bearer token.

> **Works with any MCP client.** AI integration is a standard MCP server, so sapphire-timer is not tied to [sapphire-agent](https://github.com/sapphire-turtle/sapphire-agent) despite the shared name — any AI harness that can act as an MCP client (Claude Code, Claude Desktop, …) can use it.

## Installation

Install **one** of the two editions. Both put the same command, `sapphire-timer`, on
PATH: the desktop app's binary also runs every command of the headless one (`serve`,
`mcp`, `status`, …), so it is a superset of it.

| Edition | For | How |
|---|---|---|
| Desktop app (`sapphire-timer-desktop`) | Everyday use, with the GUI | Installer for Windows, macOS and Linux (in preparation) |
| Headless (`sapphire-timer-headless`) | Hosts and servers without a GUI | Install script, cargo-binstall or cargo |

Installing both leaves two `sapphire-timer` commands; `sapphire-timer status` warns
when the one on PATH is not the one you ran.

### Headless: install script

```sh
# Linux, macOS
curl -fsSL https://raw.githubusercontent.com/sapphire-turtle/sapphire-framework/main/install/install.sh | sh -s -- timer
```

```powershell
# Windows
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/sapphire-turtle/sapphire-framework/main/install/install.ps1))) timer
```

This installs `sapphire-timer` together with `sapphire-bridge`, which it syncs
through, into `~/.local/bin`. See the
[install script's README](https://github.com/sapphire-turtle/sapphire-framework/tree/main/install) for the options.

### Headless: cargo-binstall or cargo

```sh
cargo binstall sapphire-timer-headless
# or build it
cargo install sapphire-timer-headless
```

Either installs the `sapphire-timer` binary. (0.1 was published as the `sapphire-timer`
crate.)

## MCP

An agent launches the stdio server with the command both editions put on PATH:

```json
{
  "mcpServers": {
    "sapphire-timer": { "command": "sapphire-timer", "args": ["mcp"] }
  }
}
```

Tools: `timer_start`, `timer_start_preset`, `timer_cancel`, `timer_status`, `preset_add|get|update|delete|list`,
`log_get`, `log_list`, `log_comment`, plus the framework's read-only tools.

The calling client's timer is the resource `sapphire-timer://active` (JSON: the running run, if any, and the
last event with a sequence number). To be told when a step or run ends, open `subscriptions/listen` (MCP
2026-07-28) with `resourceSubscriptions: ["sapphire-timer://active"]`; each change arrives as
`notifications/resources/updated`, and the client reads the resource for what happened. A gap in `seq` means
an update was coalesced. Subscription streams are not resumable: after reconnecting, read the resource again.

With rmcp as the client, connect with `serve_with_lifecycle(…, ClientLifecycleMode::Discover | Auto)`:
rmcp's plain `serve` uses the legacy `initialize` handshake, and legacy sessions have no `subscriptions/listen`.

## Running

```sh
sapphire-timer serve                  # the app server (usually via `sapphire-timer service install`)
sapphire-timer workspace init <dir>   # pick the workspace that holds presets and logs
sapphire-timer mcp                    # MCP on stdio, for an agent to launch
```

The desktop app (`desktop/`) runs the same commands, and run bare it opens a window onto
the same server: the framework's Settings (workspace, sync, workgroup) and the Timers screen.

`/mcp` over HTTP is off unless the config directory's `config.toml` says `[http.mcp] enabled = true`
(`sapphire-timer config set http.mcp.enabled=true --restart`); it listens on `127.0.0.1:3175` by default.
A `server.toml` from an older version is moved into `config.toml` on the first start.

## Repository layout

```
sapphire-timer/
├── headless/                    # sapphire-timer-headless: the `sapphire-timer` binary without the GUI
├── desktop/                     # Desktop GUI (egui), which runs the same commands
└── crates/
    ├── sapphire-timer-cli/      # The command line both editions run
    ├── sapphire-timer-server/   # The app server (`sapphire-timer serve`)
    ├── sapphire-timer-core/     # Data model, Markdown store, and the timer itself
    └── sapphire-timer-mcp/      # The timer's server methods and the MCP tools that call them
```

## License

| Component | License |
|---|---|
| `sapphire-timer-headless` (`headless/`) | MIT OR Apache-2.0 |
| `sapphire-timer-cli`, `sapphire-timer-server`, `sapphire-timer-core`, `sapphire-timer-mcp` (`crates/`) | MIT OR Apache-2.0 |
| `sapphire-timer-desktop` (`desktop/`) | GPL-3.0-or-later, with an app store exception (`desktop/LICENSE.App-Store-Exception`) |

The desktop app's binary includes the MIT/Apache-2.0 code of the command line and the
server, so the binary as a whole is distributed under GPL-3.0-or-later (with the same
exception).
