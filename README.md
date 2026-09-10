# Harbor

Harbor is a cross-platform, language-agnostic application hosting platform:
one daemon, one CLI, one config format for running Python/Node/.NET/Java/Rust
apps with process supervision, instead of stitching together IIS+ARR+NSSM
(Windows) or nginx+systemd (Linux/macOS) by hand. See
[`docs/PRD.md`](docs/PRD.md) for full requirements and phased scope.

This repository currently implements the **Phase 1 scope**: a daemon + CLI
process-supervision MVP. The reverse proxy, ACME/TLS, GUI, and native OS
service registration described in the PRD are later phases and not yet
implemented (see [Status](#status) below).

## Workspace layout

- `crates/harbor-core` — shared types: app/global config (TOML), runtime
  auto-detection, HTTP API request/response DTOs.
- `crates/harbor-daemon` (`harbord` binary) — process supervisor + localhost
  management API (Axum). Owns starting/stopping/restarting managed apps,
  capturing their logs, and auto-recovering them after a daemon restart.
- `crates/harbor-cli` (`harbor` binary) — CLI that talks to the daemon over
  HTTP: `add`, `start`, `stop`, `restart`, `status`, `logs`, `remove`.

## Building

Requires a Rust toolchain (stable).

```sh
cargo build --workspace
cargo test --workspace
```

## Running

Start the daemon (foreground; install as a native service is Phase 3):

```sh
cargo run -p harbor-daemon
```

By default it stores config, logs, and state under `~/.harbor` (override
with `HARBOR_HOME`) and binds its management API to `127.0.0.1:4780` (see
`~/.harbor/harbor.toml`). On first run it generates a random bearer token at
`~/.harbor/token`, which the CLI reads automatically to authenticate.

In another terminal, manage apps with the CLI:

```sh
# Add an app — runtime and launch command are auto-detected from the
# project directory, or override with --runtime / --command (FR1, FR2)
cargo run -p harbor-cli -- add ./my-api --name my-api

# Lifecycle (FR17)
cargo run -p harbor-cli -- start my-api
cargo run -p harbor-cli -- restart my-api
cargo run -p harbor-cli -- stop my-api

# Status and logs (FR18, FR19)
cargo run -p harbor-cli -- status
cargo run -p harbor-cli -- logs my-api -f

# Remove (FR20)
cargo run -p harbor-cli -- remove my-api
```

App configs are plain TOML under `~/.harbor/apps/<name>.toml` and are safe
to hand-edit or version-control (FR13, FR15); restart the app (or the
daemon) to pick up manual edits, since hot-reload (FR14) isn't implemented
yet.

## Status

Implemented (Phase 1):

- Runtime auto-detection for Python/Node/.NET/Java/Rust project markers,
  with manual override via `--runtime`/`--command` (FR1, FR2).
- Start/stop/restart/status for managed apps, via CLI and the daemon's HTTP
  API (FR3).
- Configurable restart policy (`never`/`on-failure`/`always`) with
  exponential backoff (FR4).
- Per-app environment variables and working directory (FR5).
- stdout/stderr log capture to per-app files with a tail/follow CLI view;
  no rotation yet (FR6, partial).
- Declarative per-app TOML config plus a global daemon config (FR13, FR15).
- `harbor add|start|stop|restart|status|logs|remove` CLI (FR16–FR20).
- Management API bound to `127.0.0.1` by default, bearer-token auth on all
  routes except `/health` (FR25, FR26).
- Auto-recovery: previously-running apps are restarted when the daemon
  restarts, and a live orphan process left behind by an unclean daemon exit
  is detected and reaped before its replacement is spawned, so recovery
  doesn't leave two copies of an app running (NFR3, G6).

Not yet implemented (later phases per the PRD):

- Reverse proxy, domain/path routing, ACME/TLS, WebSocket passthrough
  (FR8–FR12, Phase 2).
- Config hot-reload / `harbor apply` (FR14, Phase 2).
- Native OS service registration for the daemon itself — Windows Service,
  systemd, launchd (FR7, Phase 3).
- GUI (FR21–FR24, Phase 4).
- Log rotation, CPU/memory metrics, multi-user auth hardening (Phase 5).
