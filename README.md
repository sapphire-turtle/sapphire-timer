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

## MCP

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

`sapphire-timer-desktop` (`desktop/`) is a window onto the same server: the framework's Settings
(workspace, sync, workgroup) and, for now, a placeholder Timers screen.

`/mcp` over HTTP is off unless `server.toml` says `[mcp] http = true`; it listens on `127.0.0.1:3175` by default.

## License

| Component | License |
|---|---|
| `cli/`, `crates/sapphire-timer-core/`, `crates/sapphire-timer-mcp/` | MIT OR Apache-2.0 |
| `desktop/` | GPL-3.0-or-later, with an app store exception (`desktop/LICENSE.App-Store-Exception`) |
