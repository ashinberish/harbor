# Product Requirements Document: Harbor

**Status:** Draft v1
**Author:** Ashin
**Last updated:** September 10, 2026

---

## 1. Summary

Harbor is a cross-platform, language-agnostic application hosting platform that replaces the combination of IIS + ARR + NSSM (or nginx + systemd/PM2 on Linux) with a single unified daemon, CLI, and GUI. It lets a developer point Harbor at a Python, Node.js, .NET, Java, or Rust project and have it running behind a reverse proxy with SSL, process supervision, and monitoring — on Windows, macOS, or Linux — without hand-wiring separate tools per OS and per language.

## 2. Problem Statement

Hosting multiple polyglot applications on a single machine today requires stitching together several disconnected tools:

- **Process supervision** differs per OS: NSSM on Windows, systemd units on Linux, launchd plists on macOS.
- **Reverse proxying and SSL** are handled separately: IIS + ARR on Windows, nginx/Caddy on Linux/macOS.
- **Each language runtime** has its own launch conventions (venvs, `npm start`, `dotnet`, JARs) that have to be manually scripted per app.
- None of the above share a config format, a CLI, or a dashboard — troubleshooting means checking IIS logs, NSSM service status, and app-specific logs in three different places.

This is exactly the pattern seen in existing deployments (e.g. a FastAPI app run as an NSSM Windows service behind IIS with ARR reverse proxy, manual URL Rewrite rules, and manual SSL cert binding) — functional, but manual, OS-specific, and not repeatable across machines or team members.

## 3. Goals

- **G1**: One tool to add, run, and manage apps written in Python, Node.js, .NET, Java, and Rust.
- **G2**: One tool to reverse-proxy those apps with automatic SSL (ACME/Let's Encrypt).
- **G3**: Identical experience on Windows, macOS, and Linux — same config format, same CLI commands.
- **G4**: Both a CLI (for scripting/automation/CI) and a GUI (for day-to-day management and visibility) backed by the same daemon API.
- **G5**: Declarative, human-editable configuration that can be version-controlled.
- **G6**: Auto-recovery — crashed apps restart automatically per configurable policy.

### Non-Goals (v1)

- Multi-machine orchestration or clustering (this is a single-host tool, not a Kubernetes replacement).
- Built-in CI/CD pipelines (Harbor runs what's already built/published; building is out of scope).
- Multi-tenant / multi-user access control (single operator assumed for v1).
- Container runtime support (Docker/Podman) — v1 targets bare-process hosting only.

## 4. Target Users

- **Primary**: Ashin, self-hosting multiple polyglot services (FastAPI APIs, internal tools, side projects) on a personal or small-team server, currently doing this manually via IIS/NSSM.
- **Secondary**: Small dev teams or solo developers who want IIS-like reliability without being tied to Windows, or who want nginx/systemd-like control without hand-writing config per app.

## 5. Functional Requirements

### 5.1 Process Supervision
- FR1: Add an app by pointing Harbor at a directory; auto-detect runtime from project files (`requirements.txt`/`pyproject.toml` → Python, `package.json` → Node, `.csproj`/`.sln` → .NET, `pom.xml`/`build.gradle` → Java, `Cargo.toml` → Rust).
- FR2: Manual override of detected runtime and launch command.
- FR3: Start, stop, restart, and view status of any managed app via CLI and GUI.
- FR4: Configurable restart policy per app: never, on-failure, always, with backoff.
- FR5: Environment variable and working-directory configuration per app.
- FR6: Log capture (stdout/stderr) per app with rotation and a tail/stream view.
- FR7: Harbor daemon itself installs as a native OS service (Windows Service, systemd unit, launchd agent) so it survives reboots.

### 5.2 Reverse Proxy
- FR8: Route incoming requests by domain and/or path to the correct local app port.
- FR9: Automatic SSL certificate provisioning and renewal via ACME.
- FR10: HTTP → HTTPS redirect by default, configurable.
- FR11: WebSocket passthrough support.
- FR12: Basic request logging and per-app access logs.

### 5.3 Configuration
- FR13: Single declarative config file (TOML) per app, plus a global Harbor config.
- FR14: Config changes applied via hot-reload where possible; explicit `harbor apply` command otherwise.
- FR15: Config is diffable/version-controllable (plain text, no binary state required for reproducibility).

### 5.4 CLI
- FR16: `harbor add <path> [--runtime] [--port] [--domain]`
- FR17: `harbor start|stop|restart <app>`
- FR18: `harbor status [app]`
- FR19: `harbor logs <app> [-f]`
- FR20: `harbor remove <app>`

### 5.5 GUI
- FR21: Dashboard listing all apps with live status, CPU/memory, restart count.
- FR22: Add-app wizard using the same auto-detection as the CLI.
- FR23: Streaming log viewer per app.
- FR24: Config editor with validation before apply.

### 5.6 Security
- FR25: Management API (used by both CLI and GUI) bound to localhost by default.
- FR26: Token-based auth required for any remote/network access to the management API.
- FR27: No management endpoint reachable through the public reverse-proxy surface by default.

## 6. Non-Functional Requirements

- **NFR1 — Cross-platform**: Daemon and CLI build and run identically on Windows 10/11, macOS (Apple Silicon + Intel), and major Linux distros.
- **NFR2 — Footprint**: Daemon idle memory usage comparable to or lower than nginx + a supervisor process combined.
- **NFR3 — Reliability**: Daemon crash or restart must not require manually restarting managed apps — supervisor state is persisted and recovered.
- **NFR4 — Startup time**: Managed app should be reachable through the proxy within a few seconds of `harbor start`.
- **NFR5 — Observability**: All app and daemon logs accessible from both CLI and GUI without needing OS-specific log viewers (Event Viewer, journalctl, etc).

## 7. Proposed Architecture (Summary)

- **Daemon**: single Rust binary (Axum/Tokio), owns process supervision, reverse proxy (built on `hyper`/`tower` or embedding Pingora), TLS via `rustls` + `instant-acme`, and the config store.
- **CLI**: thin Rust binary talking to the daemon over a local HTTP/Unix-socket/named-pipe API.
- **GUI**: Tauri app (Rust + React/TypeScript) consuming the same daemon API as the CLI.
- **Config**: TOML files per app plus global settings, parsed via `serde`.

*(Full architecture detail maintained separately; this PRD focuses on requirements and scope.)*

## 8. Milestones / Phased Scope

| Phase | Scope |
|---|---|
| Phase 1 | Daemon + CLI MVP, single OS (Windows), single runtime (Python), no TLS |
| Phase 2 | Add Node/.NET/Java/Rust adapters, ACME/TLS, config hot-reload |
| Phase 3 | Cross-platform service registration (systemd, launchd) + OS installers |
| Phase 4 | GUI (Tauri dashboard) on top of stable daemon API |
| Phase 5 | Auth hardening, metrics/alerting |

## 9. Success Metrics

- Time to go from "app on disk" to "app live behind HTTPS" (target: under 2 minutes via CLI for a supported runtime).
- Number of manual steps eliminated vs current IIS/NSSM/ARR workflow (target: zero manual IIS console or NSSM CLI steps for a supported runtime).
- Daemon uptime / auto-recovery success rate for crashed apps.

## 10. Risks / Open Questions

- **Runtime auto-detection ambiguity**: Java and Node projects especially can have multiple valid build/run shapes; needs a clear override path (FR2) rather than relying on heuristics alone.
- **Service registration complexity**: Windows Service Control Manager, systemd, and launchd have meaningfully different semantics (install, permissions, log redirection); this is the highest-risk engineering area for Phase 3.
- **Security surface**: a daemon that spawns processes and proxies public traffic needs a serious security review before any non-localhost use — auth design (FR26) should not be deferred past Phase 2.
- **Open**: Should Harbor support container-based apps (Docker) in a later phase, or stay bare-process only long-term?
- **Open**: Single-operator vs multi-user access model — revisit after Phase 1 usage.

## 11. Out of Scope for v1 (Recap)

- Multi-machine clustering/orchestration
- Built-in CI/CD
- Multi-tenant access control
- Container runtime support
