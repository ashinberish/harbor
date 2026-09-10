---
title: CLI Reference
description: Every harbor subcommand and flag.
---

The `harbor` CLI talks to the `harbord` daemon over its localhost HTTP API,
authenticating with the token at `~/.harbor/token` (or `$HARBOR_HOME/token`).
The daemon must already be running.

## `harbor add`

Register an app from a project directory.

```sh
harbor add <path> [--name <name>] [--runtime <runtime>] [--command <command>] \
  [--port <port>] [--domain <domain>] [--path-prefix <prefix>] [--restart <policy>]
```

| Flag | Description |
| --- | --- |
| `<path>` | Project directory (required). Must exist; resolved to an absolute path. |
| `--name` | App name. Defaults to the directory's basename. |
| `--runtime` | Overrides auto-detection. One of `python`, `node`, `dotnet`, `java`, `rust`, `custom`. |
| `--command` | Overrides the guessed launch command, e.g. `--command "python3 app.py"`. Split on whitespace — for anything more complex, edit the app's TOML file directly. |
| `--port` | Port the app listens on. Required for the reverse proxy to route to it. |
| `--domain` | Routes requests by `Host` header/SNI to this app (FR8) and provisions a TLS certificate for the domain. |
| `--path-prefix` | Routes requests whose path starts with this prefix to this app, prefix stripped before forwarding (FR8). |
| `--restart` | Restart policy: `never`, `on-failure` (default), or `always`. |

## `harbor start` / `stop` / `restart`

```sh
harbor start <app>
harbor stop <app>
harbor restart <app>
```

`start` and `stop` are idempotent — starting an already-running app or
stopping an already-stopped one succeeds without error. `restart` stops the
app, waits for it to fully exit, then starts it again.

## `harbor status`

```sh
harbor status [app]
```

With no argument, lists every registered app. With an app name, shows just
that app. Columns: name, runtime, state (`running` / `stopped` /
`restarting` / `crashed`), PID, port, restart count, and domain.

## `harbor logs`

```sh
harbor logs <app> [-f | --follow] [--lines <n>]
```

Prints the app's captured stdout/stderr (stdout to stdout, stderr to
stderr). `--lines` controls how much tail history to fetch (default `100`).
`-f`/`--follow` polls the daemon once a second and prints new lines as they
arrive.

## `harbor remove`

```sh
harbor remove <app>
```

Stops the app if it's running, then deletes its config and log files.

## `harbor apply`

```sh
harbor apply
```

Reloads app configs from disk immediately and reports how many were
loaded. The daemon also watches `apps_dir` and applies changes
automatically (FR14) — `apply` is for confirming a change landed
synchronously, or as a fallback if the watch isn't running.
