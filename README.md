# Harbor

Harbor is a cross-platform, language-agnostic application hosting platform:
one daemon, one CLI, one config format for running Python/Node/.NET/Java/Rust
apps with process supervision and a reverse proxy, instead of stitching
together IIS+ARR+NSSM (Windows) or nginx+systemd (Linux/macOS) by hand. See
[`docs/PRD.md`](docs/PRD.md) for full requirements and phased scope, or the
docs site at **https://ashinberish.github.io/harbor/** for guides and
reference.

This repository currently implements **Phase 1 and Phase 2** of the PRD:
process supervision plus a reverse proxy with TLS/ACME and config
hot-reload. Native OS service registration and the GUI are later phases and
not yet implemented (see [Status](#status) below).

## Workspace layout

- `crates/harbor-core` — shared types: app/global config (TOML), runtime
  auto-detection, HTTP API request/response DTOs.
- `crates/harbor-daemon` (`harbord` binary) — process supervisor, reverse
  proxy (HTTP/HTTPS with WebSocket passthrough), TLS certificate management
  (self-signed + ACME), and the localhost management API (Axum).
- `crates/harbor-cli` (`harbor` binary) — CLI that talks to the daemon over
  HTTP: `add`, `start`, `stop`, `restart`, `status`, `logs`, `remove`,
  `apply`.

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

By default it stores config, logs, certs, and state under `~/.harbor`
(override with `HARBOR_HOME`), binds its management API to
`127.0.0.1:4780`, and starts the reverse proxy on `0.0.0.0:8080` (HTTP) and
`0.0.0.0:8443` (HTTPS) — non-privileged ports by default so it runs without
elevated permissions; point them at 80/443 in `~/.harbor/harbor.toml` for a
real deployment. On first run it generates a random bearer token at
`~/.harbor/token`, which the CLI reads automatically to authenticate.

In another terminal, manage apps with the CLI:

```sh
# Add an app — runtime and launch command are auto-detected from the
# project directory, or override with --runtime / --command (FR1, FR2)
cargo run -p harbor-cli -- add ./my-api --name my-api --port 8000 --domain my-api.example.com

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

Once an app has a `--port` and a `--domain` and/or `--path-prefix`, the
daemon's reverse proxy routes requests to it automatically over both HTTP
and HTTPS — a self-signed certificate is generated on the fly per domain,
upgraded transparently to a real one if ACME is enabled and the domain is
publicly reachable (see the docs site's Configuration page).

App configs are plain TOML under `~/.harbor/apps/<name>.toml` and are safe
to hand-edit or version-control (FR13, FR15). The daemon watches that
directory and picks up changes automatically (FR14); run `harbor apply` for
an explicit, synchronous reload instead of waiting on the watch.

## Status

Implemented (Phase 1 + Phase 2):

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
- `harbor add|start|stop|restart|status|logs|remove|apply` CLI (FR16–FR20).
- Management API bound to `127.0.0.1` by default, bearer-token auth on all
  routes except `/health` (FR25, FR26).
- Auto-recovery: previously-running apps are restarted when the daemon
  restarts, and a live orphan process left behind by an unclean daemon exit
  is detected and reaped before its replacement is spawned, so recovery
  doesn't leave two copies of an app running (NFR3, G6).
- Reverse proxy routing by domain (`Host` header, SNI) and/or URL path
  prefix to an app's local port, with WebSocket passthrough (FR8, FR11).
- HTTPS via `rustls`: a self-signed certificate per domain by default, and
  automatic ACME (Let's Encrypt) issuance/renewal over HTTP-01 when
  configured with a real, publicly-reachable domain (FR9).
- HTTP→HTTPS redirect, on by default and configurable (FR10).
- Per-app access logs (method, path, status, latency) (FR12).
- Config hot-reload: the daemon watches `apps_dir` and applies changes
  automatically, plus an explicit `harbor apply` (FR14).

Not yet implemented (later phases per the PRD):

- Native OS service registration for the daemon itself — Windows Service,
  systemd, launchd (FR7, Phase 3).
- GUI (FR21–FR24, Phase 4).
- Log rotation, CPU/memory metrics, multi-user auth hardening (Phase 5).
