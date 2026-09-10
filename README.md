# Harbor

Harbor is a cross-platform, language-agnostic application hosting platform:
one daemon, one CLI, one config format for running Python/Node/.NET/Java/Rust
apps with process supervision and a reverse proxy, instead of stitching
together IIS+ARR+NSSM (Windows) or nginx+systemd (Linux/macOS) by hand. See
[`docs/PRD.md`](docs/PRD.md) for full requirements and phased scope, or the
docs site at **https://ashinberish.github.io/harbor/** for guides and
reference.

This repository currently implements **Phases 1–4** of the PRD: process
supervision, a reverse proxy with TLS/ACME and config hot-reload, native OS
service registration, and a desktop GUI. Phase 5 (log rotation, metrics
retention, multi-user auth hardening) is the remaining later phase (see
[Status](#status) below).

## Workspace layout

- `crates/harbor-core` — shared types: app/global config (TOML), runtime
  auto-detection, HTTP API request/response DTOs.
- `crates/harbor-daemon` (`harbord` binary) — process supervisor, reverse
  proxy (HTTP/HTTPS with WebSocket passthrough), TLS certificate management
  (self-signed + ACME), and the localhost management API (Axum). Runs as a
  plain foreground process, or as a native OS service (Windows Service via
  the `windows-service` crate; systemd/launchd manage it directly).
- `crates/harbor-cli` (`harbor` binary) — CLI that talks to the daemon over
  HTTP: `add`, `start`, `stop`, `restart`, `status`, `logs`, `remove`,
  `apply`, plus `service install|uninstall|start|stop|status` for OS
  service registration.
- `gui/` — desktop GUI (Tauri v2 + React/TypeScript): a live app dashboard,
  an add-app wizard, a streaming log viewer, and a config editor. See
  [Running the GUI](#running-the-gui) below.

## Building

Requires a Rust toolchain (stable).

```sh
cargo build --workspace
cargo test --workspace
```

## Running

Start the daemon in the foreground (for OS service installation, see
[Running as a service](#running-as-a-service) below):

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

## Running as a service

`harbor service install` registers `harbord` with the OS's native service
manager so it starts at boot without a terminal left open (FR7):

```sh
# Linux (systemd) — system-wide by default, needs root; --user for a
# per-user service instead
sudo harbor service install
# macOS (launchd) — same system/--user split
sudo harbor service install
# Windows (Services / SCM) — run from an elevated (Administrator) prompt
harbor service install
```

It finds the `harbord` binary automatically (next to the `harbor` binary
running the command), starts the service immediately, and `harbor service
status`/`stop`/`start`/`uninstall` manage it afterward. See the docs site's
Architecture page for how each platform's backend works.

## Running the GUI

The GUI talks to the same management API as the CLI (it reads
`~/.harbor/token` itself, server-side — the token never reaches the
webview), so start the daemon first, same as above. Then, from `gui/`:

```sh
npm install
npm run tauri dev
```

Requires a Rust toolchain, Node.js, and (on Linux) the Tauri/webkit2gtk
system packages — see [Tauri's prerequisites
guide](https://tauri.app/start/prerequisites/). `npm run tauri build`
produces a native installer/bundle instead.

The dashboard lists managed apps with live status, PID, CPU%, memory, and
restart count (polled every 2s); each row has start/stop/restart/remove
actions plus buttons that open a streaming log viewer and a config editor.
"+ Add App" opens a two-step wizard: pick a directory (with a "Detect
runtime" button backed by the same auto-detection as `harbor add`), then
configure name/runtime/command/port/domain/restart policy/env vars.

## Status

Implemented (Phases 1–4):

- Runtime auto-detection for Python/Node/.NET/Java/Rust project markers,
  with manual override via `--runtime`/`--command` (FR1, FR2).
- Start/stop/restart/status for managed apps, via CLI and the daemon's HTTP
  API (FR3).
- Configurable restart policy (`never`/`on-failure`/`always`) with
  exponential backoff (FR4).
- Per-app environment variables and working directory (FR5).
- stdout/stderr log capture to per-app files with a tail/follow CLI view;
  no rotation yet (FR6, partial).
- Native OS service registration: `harbor service install` — a systemd
  unit on Linux, a launchd job on macOS, a real Windows Service (via the
  `windows-service` crate, not just an unmanaged wrapped process) on
  Windows (FR7).
- Declarative per-app TOML config plus a global daemon config (FR13, FR15).
- `harbor add|start|stop|restart|status|logs|remove|apply|service` CLI
  (FR16–FR20).
- Management API bound to `127.0.0.1` by default, bearer-token auth on all
  routes except `/health` (FR25, FR26).
- Auto-recovery: previously-running apps are restarted when the daemon
  restarts, and a live orphan process left behind by an unclean daemon exit
  is detected and reaped before its replacement is spawned, so recovery
  doesn't leave two copies of an app running (NFR3, G6). The daemon itself
  also shuts down cleanly on SIGTERM/Ctrl+C (or an SCM stop on Windows)
  rather than being killed outright, so this cleanup actually runs.
- Reverse proxy routing by domain (`Host` header, SNI) and/or URL path
  prefix to an app's local port, with WebSocket passthrough (FR8, FR11).
- HTTPS via `rustls`: a self-signed certificate per domain by default, and
  automatic ACME (Let's Encrypt) issuance/renewal over HTTP-01 when
  configured with a real, publicly-reachable domain (FR9).
- HTTP→HTTPS redirect, on by default and configurable (FR10).
- Per-app access logs (method, path, status, latency) (FR12).
- Config hot-reload: the daemon watches `apps_dir` and applies changes
  automatically, plus an explicit `harbor apply` (FR14).
- CPU% and memory-per-app sampling in the daemon (`sysinfo`, refreshed
  every 2s), exposed via the status API and consumed by both `harbor
  status` and the GUI dashboard (FR21).
- Desktop GUI (Tauri v2 + React/TypeScript, `gui/`): live dashboard with
  start/stop/restart/remove (FR21), an add-app wizard with runtime
  auto-detection (FR22), a streaming log viewer (FR23), and a config editor
  with client- and server-side validation (FR24). The Rust side of the app
  talks to the daemon's HTTP API directly — the frontend only calls
  `invoke()`, so the bearer token never reaches the webview.

Not yet implemented (later phase per the PRD):

- Log rotation, metrics retention/history, multi-user auth hardening
  (Phase 5).

**A note on cross-platform verification**: this project was built and
tested on Linux. The systemd backend was verified as far as this sandbox
allows (unit-file generation and graceful error handling are exercised for
real; there's no running systemd instance to actually register with). The
Windows backend (daemon SCM integration and the CLI's service registration)
was cross-compiled, linked, and clippy-checked against a real
`x86_64-pc-windows-gnu` target — genuine `.exe` output — but never run on
actual Windows. The launchd backend was checked for correctness in
isolation against `x86_64-apple-darwin` (it only uses `std`, no
platform-specific crates), but the full CLI can't be cross-compiled for
macOS here because an unrelated dependency needs a real macOS SDK. None of
the three service backends have been exercised against a real systemd,
launchd, or Windows SCM instance — treat them as implemented-and-checked,
not field-tested.

**A note on GUI verification**: this sandbox has no interactive display, so
the GUI was built and tested headlessly — `cargo build`/`clippy` on the
Tauri backend, `tsc`/`vite build` on the frontend, then the actual app
launched under a virtual X server (Xvfb) and driven end-to-end with
`xdotool` against a real running `harbord` and a real test app: adding an
app through the wizard (including runtime auto-detection), watching its
status/PID/CPU/memory update live via polling after starting it from the
CLI, viewing its real stdout log, editing and saving its config (verified
against the on-disk TOML), and removing it — all screenshotted at each
step to confirm the UI rendered and updated correctly. It has not been run
on a real desktop session on any platform, and the app icon is still
Tauri's scaffold default rather than a Harbor-specific one.
