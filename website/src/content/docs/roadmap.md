---
title: Roadmap & Status
description: What's implemented today versus the PRD's phased scope.
---

Harbor's full requirements live in the PRD
([`docs/PRD.md`](https://github.com/ashinberish/harbor/blob/cl/lucid-babbage-3kdpkl/docs/PRD.md)
in the repo). This page tracks what's actually built.

## Phases

| Phase | Scope | Status |
| --- | --- | --- |
| 1 | Daemon + CLI MVP, process supervision | **Implemented** (this site documents it) |
| 2 | Reverse proxy, ACME/TLS, config hot-reload | Not started |
| 3 | Native OS service registration (systemd, launchd, Windows Service) | Not started |
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
- `harbor add|start|stop|restart|status|logs|remove` CLI (FR16–FR20).
- Management API bound to `127.0.0.1` by default, bearer-token auth on all
  routes except `/health` (FR25, FR26).
- Auto-recovery: previously running apps are restarted when the daemon
  restarts, and a live orphan process left behind by an unclean daemon exit
  is detected and reaped before its replacement is spawned, so recovery
  never leaves two copies of an app running (NFR3, G6).

## Not yet implemented

- Reverse proxy, domain/path routing, ACME/TLS, WebSocket passthrough
  (FR8–FR12, Phase 2).
- Config hot-reload / `harbor apply` (FR14, Phase 2).
- Native OS service registration for the daemon itself — Windows Service,
  systemd, launchd (FR7, Phase 3).
- GUI (FR21–FR24, Phase 4).
- Log rotation, CPU/memory metrics, multi-user auth hardening (Phase 5).

## Open questions (from the PRD)

- Should Harbor support container-based apps (Docker) in a later phase, or
  stay bare-process only long-term?
- Single-operator vs. multi-user access model — revisit after Phase 1
  usage.
