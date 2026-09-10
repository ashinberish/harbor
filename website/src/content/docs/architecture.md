---
title: Architecture
description: How the Harbor daemon supervises processes, proxies traffic, and recovers from crashes.
---

## Workspace layout

```
crates/
├── harbor-core    # shared config types, runtime auto-detection, API DTOs
├── harbor-daemon  # harbord binary — supervisor + reverse proxy + management API
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
5. If the reverse proxy is enabled: provisions TLS certificates, binds the
   HTTP and HTTPS listeners, and (if configured) starts the ACME
   issuance/renewal loop.
6. Starts watching `apps_dir` for config changes.
7. Starts the Axum management API bound to `bind_addr` (`127.0.0.1:4780`
   by default).

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

## The reverse proxy

The proxy is a hand-rolled `hyper` 1.x service, not axum — routing needs
direct access to the raw request/connection to support WebSocket upgrades,
which don't fit neatly through axum's extractor model. Both the plain-HTTP
and TLS listeners run the same connection handler
(`proxy::serve_one_connection`); the TLS listener just wraps each accepted
`TcpStream` in a `tokio_rustls::TlsAcceptor` first.

**Routing (FR8).** There's no separate routing table — every request asks
`Supervisor::resolve_route(host, path)` to scan the live app-config map
directly. This means config hot-reload and proxy routing need no
coordination with each other: the moment `apps_dir` is reloaded, the very
next request sees the new routes. An exact `Host`/SNI match against an
app's `domain` always wins; otherwise the longest matching `path_prefix`
wins. No match is a `404`.

**WebSocket passthrough (FR11).** On an `Upgrade: websocket` request, the
handler opens a dedicated connection to the target app (bypassing the
normal per-request path, which doesn't preserve the raw connection), sends
the upgrade request, and — if the app responds `101 Switching Protocols` —
spawns a task that waits for both the client's and the upstream's
`hyper::upgrade::on()` futures to resolve, then splices raw bytes between
them with `tokio::io::copy_bidirectional` until either side closes. This
was verified with a hand-rolled RFC 6455 handshake and a raw-byte echo
tunnel test, not just unit tests of the routing logic.

**HTTP→HTTPS redirect (FR10).** The plain-HTTP listener 301-redirects to
`https://<host>:<https-port>/<path>` when `https_redirect` is on and an
HTTPS listener is actually running — including the port when it isn't the
standard 443, since Harbor's own default (`8443`) isn't. The one exception
is `/.well-known/acme-challenge/*`, always answered directly so ACME
HTTP-01 validation works regardless of the redirect setting.

**Access logs (FR12).** Every proxied request appends one line — method,
path, status, latency — to `logs/<app>.access.log`, whichever app's route
served it.

## TLS and ACME

`tls::CertStore` implements rustls's `ResolvesServerCert`, picking a
certificate by SNI from an in-memory map built from
`certs/<domain>/{cert.pem,key.pem}`. Every domain gets a **self-signed**
certificate generated on first use (via `rcgen`) regardless of ACME
settings — HTTPS always works, even offline or before DNS is set up. A
fixed default certificate (for `localhost`) covers requests with no SNI or
an unrecognized one, so path-prefix-routed apps (which don't need a domain
at all) stay reachable over HTTPS too.

When `proxy.acme.enabled` is set, a background task (`acme.rs`, built on
`instant-acme`) requests a real certificate per domain over HTTP-01:
create/reuse an ACME account, create an order, fetch the HTTP-01 challenge
token, serve its key authorization at
`/.well-known/acme-challenge/<token>` (via a shared map the plain-HTTP
listener consults), signal readiness, poll for validation, then finalize
and store the issued cert — overwriting the self-signed one in
`CertStore` in place, with no listener restart. A domain stays on
self-signed if ACME issuance fails for any reason (no public DNS yet,
validation unreachable, rate-limited, ...); the loop just retries roughly
every 24 hours skipping domains that don't need it yet. Renewal is a fixed
60-day threshold against `acme-meta.json`'s issuance timestamp, not the
certificate's actual `notAfter` — close enough for Let's Encrypt's normal
90-day lifetime without adding an X.509-parsing dependency.

This was tested against Let's Encrypt's real **staging** API (the default
`directory_url`) as far as this sandboxed environment allows: account
creation, order creation, authorization retrieval, and challenge placement
all succeed against the live server; validation itself fails only because
nothing in this dev environment is publicly DNS-resolvable and
reachable on port 80 — exactly the condition a real deployment needs to
satisfy for ACME to work at all. On that expected failure, the code
correctly falls back to the working self-signed certificate rather than
taking the domain's HTTPS down.

Binding `http_bind_addr`/`https_bind_addr` to the standard `80`/`443`
needs a privilege Harbor doesn't request for you: on Linux, grant it with
`setcap 'cap_net_bind_service=+ep' /path/to/harbord` or a systemd unit's
`AmbientCapabilities=CAP_NET_BIND_SERVICE`; on Windows, an admin can grant
a service account that right. Harbor defaults to `8080`/`8443` precisely
so it runs without any of that out of the box.

## Config hot-reload

`Supervisor::reload_configs()` re-scans `apps/*.toml` and updates the live
config map — the same method both paths below call:

- **Automatic (FR14).** `watch.rs` uses the `notify` crate (inotify on
  Linux, kqueue on macOS/BSD, `ReadDirectoryChangesW` on Windows) to watch
  `apps_dir`, debouncing bursts of filesystem events (editors/`cp` produce
  several per save) by 300ms before reloading.
- **Explicit.** `POST /reload`, exposed as `harbor apply`, for a
  synchronous confirmation or as a fallback if the watch isn't running.

New or edited apps are picked up this way; an app whose file is deleted
out-of-band keeps running under its last-loaded config until explicitly
`harbor remove`d — reload only applies additions and edits, matching
FR14's "apply config changes," not "detect manual file deletion."

## Graceful shutdown

`harbord`'s entire startup sequence lives in one `run_daemon(shutdown)`
function taking a future that resolves when it's time to stop — normally
Ctrl+C or SIGTERM (`main.rs` installs both via `tokio::signal`), or an SCM
stop notification when running as a Windows service (see below). Catching
SIGTERM rather than leaving the OS's default disposition (immediate kill)
matters concretely: supervised child processes are only reliably cleaned
up via `kill_on_drop` when Rust's normal drop glue runs on the way out of
`main()`, which an unhandled signal bypasses entirely — you'd see orphaned
app processes and have to lean on the reaping logic above instead of a
clean stop.

## Native OS service registration

`harbor service install` (FR7) registers `harbord` with the OS's service
manager so it starts at boot without a terminal open — replacing the
NSSM/systemd-unit-by-hand setup the PRD's problem statement describes.
Each backend lives in `harbor-cli/src/service/`, dispatched by
`target_os` at compile time:

- **systemd (Linux)** — generates a `.service` unit (`ExecStart` pointing
  at the located `harbord` binary) and shells out to `systemctl`.
  `--user` installs to `~/.config/systemd/user/` and uses `systemctl
  --user` instead of `/etc/systemd/system/` + root; systemd manages an
  arbitrary process fine either way, no special cooperation from `harbord`
  needed.
- **launchd (macOS)** — generates a `.plist` (LaunchDaemon at
  `/Library/LaunchDaemons/` by default, LaunchAgent at
  `~/Library/LaunchAgents/` with `--user`) and drives it with the modern
  `launchctl bootstrap`/`bootout`/`kickstart`/`print` subcommands. Same
  story as systemd: no cooperation needed from the daemon itself.
- **Windows Service** — the outlier. Windows' Service Control Manager
  expects the process to actively participate in its protocol (register a
  control handler, report `Running`, report `Stopped` before exiting) —
  a plain console app registered with `sc.exe create` gets killed rather
  than stopped cleanly. So `harbord` itself has a `#[cfg(windows)]`
  `winservice` module: `harbor service install` passes `--windows-service`
  in the registered command line, and `main()` checks for that flag before
  doing anything else, handing control to `windows_service::service_dispatcher`
  instead of running `run_daemon` directly in the foreground. The SCM's
  synchronous stop callback (delivered on its own OS thread, not inside
  Tokio) is bridged into the async `shutdown` future `run_daemon` expects
  via a blocking channel receive inside `spawn_blocking`. The CLI side uses
  the `windows-service` crate's `ServiceManager` to create/delete/start/stop
  the service.

All three backends locate the `harbord` binary automatically (a binary
named `harbord`/`harbord.exe` next to the running `harbor` CLI — true for
both a cargo build and a typical installed layout), overridable with
`--binary`.

**Verification note.** This was all built and tested on Linux, where the
container's PID 1 isn't systemd — `systemctl` itself can't run here, so
the systemd backend was verified as far as that allows: unit-file content
generation, binary auto-location, and graceful error propagation when
`systemctl` is unreachable, all confirmed for real, but not an actual
`systemctl start`. The Windows backend (both `winservice.rs` in the
daemon and the CLI's `windows.rs`) was cross-compiled, linked, and
clippy-checked against a real `x86_64-pc-windows-gnu` target — genuine
`.exe` output — but never run against a real SCM. The launchd backend was
checked for correctness in isolation against `x86_64-apple-darwin` (it's
pure `std::process`/`std::fs`, no platform-specific crate), since the full
CLI can't be cross-compiled for macOS here — an unrelated dependency
(reqwest's TLS backend) needs a real macOS SDK to cross-compile that this
sandbox doesn't have.

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
| `GET` | `/apps/:name/config` | Full declarative config for one app (used by the GUI's config editor). |
| `PUT` | `/apps/:name/config` | Validate and persist an edited config. |
| `POST` | `/reload` | Explicit config reload (`harbor apply`). |

The CLI and the GUI are both thin clients over this API (PRD G4) — neither
talks to the supervisor directly.

## The GUI (`gui/`)

A Tauri v2 app: a Rust backend (`gui/src-tauri`) wrapping the same
management API with one `tauri::command` per endpoint above, and a React
+ TypeScript frontend (`gui/src`) that only calls `invoke()`. Keeping the
HTTP client in Rust rather than the webview means the bearer token (read
from `~/.harbor/token`, same as the CLI) never reaches frontend JS, and
sidesteps any CORS concerns.

- **Dashboard** — the app table, polling `list_apps` every 2s.
- **Add-app wizard** — calls the local `detect_runtime` command (the same
  `harbor_core::detect` logic the daemon uses) before submitting `add_app`.
- **Log viewer** — polls `get_logs`, tabbed stdout/stderr with auto-scroll.
- **Config editor** — `get_app_config`/`update_app_config`, with the same
  shape of validation (non-empty path/command, port range, restart-backoff
  ordering) checked client-side before the server's own validation runs.
