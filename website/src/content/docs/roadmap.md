---
title: Roadmap & Status
description: What's implemented today versus the PRD's phased scope.
---

Harbor's full requirements live in the PRD
([`docs/PRD.md`](https://github.com/ashinberish/harbor/blob/main/docs/PRD.md)
in the repo). This page tracks what's actually built.

## Phases

| Phase | Scope | Status |
| --- | --- | --- |
| 1 | Daemon + CLI MVP, process supervision | **Implemented** |
| 2 | Reverse proxy, ACME/TLS, config hot-reload | **Implemented** |
| 3 | Native OS service registration (systemd, launchd, Windows Service) | **Implemented** (this site documents it) |
| 4 | GUI (Tauri dashboard) | Not started |
| 5 | Auth hardening, metrics/alerting | Not started |

## Implemented (Phase 1)

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
- Management API bound to `127.0.0.1` by default, bearer-token auth on all
  routes except `/health` (FR25, FR26).
- Auto-recovery: previously running apps are restarted when the daemon
  restarts, and a live orphan process left behind by an unclean daemon exit
  is detected and reaped before its replacement is spawned, so recovery
  never leaves two copies of an app running (NFR3, G6).

## Implemented (Phase 2)

- Reverse proxy routing by domain (`Host`/SNI) and/or path prefix to an
  app's local port (FR8).
- WebSocket passthrough, verified with a real HTTP Upgrade handshake and a
  raw-byte tunnel test through the proxy, not just unit-level routing
  checks (FR11).
- HTTPS via `rustls`: a self-signed certificate per domain always, so HTTPS
  works immediately with no setup; automatic ACME (Let's Encrypt)
  issuance/renewal over HTTP-01 when a domain is publicly reachable
  (FR9). Verified against Let's Encrypt's real staging API up to the point
  that requires public DNS/reachability this dev environment doesn't have.
- HTTP→HTTPS redirect, on by default and configurable, correct even when
  the HTTPS listener isn't on the standard port 443 (FR10).
- Per-app access logs — method, path, status, latency (FR12).
- Config hot-reload: automatic via a filesystem watch on `apps_dir`, plus
  an explicit `harbor apply` (FR14).
- `harbor add|start|stop|restart|status|logs|remove|apply` CLI
  (FR16–FR20).

## Implemented (Phase 3)

- `harbor service install|uninstall|start|stop|status`, registering
  `harbord` with the OS's native service manager so it starts at boot
  without a terminal left open (FR7):
  - **systemd** (Linux) — generates a unit, drives it with `systemctl`.
  - **launchd** (macOS) — generates a plist, drives it with `launchctl`.
  - **Windows Service** — a real SCM-integrated service (the daemon
    responds to Stop control via the `windows-service` crate), not just
    an unmanaged process pointed at by `sc.exe create`.
- Graceful shutdown on SIGTERM/Ctrl+C (and SCM stop, on Windows), so
  supervised child processes actually get the cleanup pass that depends
  on Rust's normal drop glue running, instead of the process being killed
  outright.
- The systemd backend was verified in this sandbox as far as its own
  lack of a running systemd instance allows (unit generation, binary
  auto-location, graceful error handling, all confirmed for real). The
  Windows backend was cross-compiled, linked, and clippy-checked against
  a real `x86_64-pc-windows-gnu` target. The launchd backend was checked
  for correctness in isolation against `x86_64-apple-darwin` (pure
  `std`, no platform crate) since the full CLI can't be cross-compiled
  for macOS here. None of the three were exercised against a real
  systemd, launchd, or Windows SCM instance.

## Not yet implemented

- GUI (FR21–FR24, Phase 4).
- Log rotation, CPU/memory metrics, multi-user auth hardening (Phase 5).

## Open questions (from the PRD)

- Should Harbor support container-based apps (Docker) in a later phase, or
  stay bare-process only long-term?
- Single-operator vs. multi-user access model — revisit after Phase 1
  usage.
