---
title: Configuration
description: The TOML schema behind Harbor's config, and where files live.
---

Harbor's config is plain, human-editable TOML (PRD FR13/FR15) — safe to
read, hand-edit, or check into version control. Edits take effect the next
time the app or daemon is (re)started; hot-reload isn't implemented yet.

## Where files live

Everything lives under `$HARBOR_HOME`, which defaults to `~/.harbor`:

```
~/.harbor/
├── harbor.toml           # global daemon config
├── token                 # management API bearer token (0600 on Unix)
├── apps/
│   └── <name>.toml       # one file per app — the AppConfig below
├── state/
│   └── <name>.state.json # desired run state + last known pid (internal)
└── logs/
    ├── <name>.out.log    # captured stdout
    └── <name>.err.log    # captured stderr
```

Only `apps/*.toml` is meant to be version-controlled. `state/` and `logs/`
are runtime-generated and daemon-internal.

## Global config — `harbor.toml`

```toml
bind_addr = "127.0.0.1:4780"
```

| Field | Default | Description |
| --- | --- | --- |
| `bind_addr` | `127.0.0.1:4780` | Address the management API listens on. Per FR25, this should stay bound to localhost — the API has no built-in TLS, and binding it elsewhere means relying on the bearer token alone for auth. |

## App config — `apps/<name>.toml`

```toml
name = "my-api"
path = "/home/me/projects/my-api"
runtime = "python"
command = ["python3", "main.py"]
port = 8000
restart_policy = "on-failure"
restart_backoff_seconds = 1
restart_max_backoff_seconds = 30

[env]
LOG_LEVEL = "info"
```

| Field | Required | Description |
| --- | --- | --- |
| `name` | yes | App name; must be unique. Also the config filename (`<name>.toml`) and log filename prefix. |
| `path` | yes | Absolute path to the project directory. |
| `runtime` | yes | One of `python`, `node`, `dotnet`, `java`, `rust`, `custom`. Auto-detected by `harbor add` unless overridden. |
| `command` | yes | Launch command as an argv array — first element is the executable. |
| `working_dir` | no | Working directory for the process. Defaults to `path`. |
| `port` | no | Informational in Phase 1; will be used for reverse-proxy routing in a later phase. |
| `domain` | no | Same caveat as `port`. |
| `env` | no | Extra environment variables passed to the process (merged with the daemon's own environment). |
| `restart_policy` | no | `never`, `on-failure` (default), or `always`. |
| `restart_backoff_seconds` | no | Base backoff before the first restart attempt. Default `1`. |
| `restart_max_backoff_seconds` | no | Cap on the exponential backoff between restart attempts. Default `30`. |

## Restart policy and backoff

When a managed process exits, Harbor decides whether to restart it based on
`restart_policy`:

- **`never`** — the app stays stopped, regardless of exit code.
- **`on-failure`** (default) — restarted only on a non-zero exit.
- **`always`** — restarted even on a clean exit.

Each restart attempt waits `restart_backoff_seconds * 2^restart_count`,
capped at `restart_max_backoff_seconds`. A deliberate `harbor stop` during
that backoff window cancels the pending restart.
