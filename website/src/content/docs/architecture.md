---
title: Architecture
description: How the Harbor daemon supervises processes and recovers from crashes.
---

## Workspace layout

```
crates/
├── harbor-core    # shared config types, runtime auto-detection, API DTOs
├── harbor-daemon  # harbord binary — the process supervisor + management API
└── harbor-cli     # harbor binary — talks to harbord over HTTP
```

`harbor-core` has no daemon- or CLI-specific code: both binaries depend on
it so the on-disk TOML schema and the HTTP request/response types can never
drift apart.

## The daemon (`harbord`)

On startup, `harbord`:

1. Resolves `$HARBOR_HOME` (default `~/.harbor`) and creates its
   subdirectories.
2. Loads (or writes a default) `harbor.toml`, and generates a bearer token
   at `token` on first run.
3. Loads every `apps/*.toml` into memory.
4. For each app whose last known desired state was "running", spawns it —
   this is auto-recovery after a daemon restart (see below).
5. Starts the Axum HTTP server bound to `bind_addr` (`127.0.0.1:4780` by
   default).

### Process supervision

Each running app is owned by one Tokio task (`run_app_supervised`). The
task spawns the child process with stdout/stderr redirected to
`logs/<name>.{out,err}.log`, then `select!`s between the child exiting on
its own and a stop signal sent through an internal control channel by
`harbor stop`/`restart`:

- **Deliberate stop** — the task kills the child and exits; no restart.
- **Child exits on its own** — the task consults `restart_policy`
  (`never` / `on-failure` / `always`) to decide whether to restart, and if
  so, sleeps for an exponentially increasing backoff
  (`restart_backoff_seconds * 2^restart_count`, capped at
  `restart_max_backoff_seconds`) before spawning again.

App state (`stopped` / `running` / `restarting` / `crashed`), PID, restart
count, and last exit code live in an in-memory map guarded by a mutex, and
are what `GET /apps` reports.

### Auto-recovery and orphan reaping

Whether an app *should* be running is persisted separately from its
declarative config, in `state/<name>.state.json` — this keeps runtime
detail out of the version-controlled TOML. `harbor start`/`stop` update
that file; a fresh daemon instance reads it on boot to decide what to
restart (PRD NFR3, G6).

That file also records the last known PID. If a previous daemon instance
was killed uncleanly (e.g. `SIGKILL`), its supervised child processes are
orphaned but keep running under the OS. Without a check, a freshly started
daemon would auto-recover the same app by spawning a *second* concurrent
copy. Instead, before spawning, each supervised task checks whether the
recorded PID is still alive and, if so, terminates it first — so recovery
always converges on exactly one running instance per app.

## The management API

Axum-based HTTP API, bound to localhost by default (FR25) and requiring a
bearer token on every route except `/health` (FR26):

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/health` | Unauthenticated liveness check. |
| `GET` | `/apps` | List all apps and their status. |
| `POST` | `/apps` | Add an app (runs runtime auto-detection if `runtime` is omitted). |
| `GET` | `/apps/:name` | Status for one app. |
| `DELETE` | `/apps/:name` | Remove an app. |
| `POST` | `/apps/:name/start` | Start an app. |
| `POST` | `/apps/:name/stop` | Stop an app. |
| `POST` | `/apps/:name/restart` | Restart an app. |
| `GET` | `/apps/:name/logs?lines=N` | Tail captured stdout/stderr. |

The CLI is a thin client over this API — the GUI planned for a later phase
is expected to be another client of the same surface (PRD G4).

## What's not here yet

The reverse proxy, ACME/TLS termination, and native OS service registration
described in the PRD are later phases and don't exist in this codebase yet
— see [Roadmap & Status](/harbor/roadmap/).
