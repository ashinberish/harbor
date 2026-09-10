---
title: Configuration
description: The TOML schema behind Harbor's config, and where files live.
---

Harbor's config is plain, human-editable TOML (PRD FR13/FR15) — safe to
read, hand-edit, or check into version control. The daemon watches
`apps/*.toml` for changes and applies edits automatically (FR14); run
`harbor apply` for an explicit, synchronous reload instead of waiting on
the watch.

## Where files live

Everything lives under `$HARBOR_HOME`, which defaults to `~/.harbor`:

```
~/.harbor/
├── harbor.toml              # global daemon config
├── token                    # management API bearer token (0600 on Unix)
├── apps/
│   └── <name>.toml          # one file per app — the AppConfig below
├── state/
│   └── <name>.state.json    # desired run state + last known pid (internal)
├── certs/
│   ├── acme-account.json    # ACME account credentials (internal)
│   └── <domain>/
│       ├── cert.pem         # self-signed or ACME-issued certificate
│       ├── key.pem          # its private key
│       └── acme-meta.json   # issuance time, for renewal scheduling
└── logs/
    ├── <name>.out.log       # captured stdout
    ├── <name>.err.log       # captured stderr
    └── <name>.access.log    # proxied requests to this app
```

Only `apps/*.toml` is meant to be version-controlled. `state/`, `certs/`,
and `logs/` are runtime-generated and daemon-internal.

## Global config — `harbor.toml`

```toml
bind_addr = "127.0.0.1:4780"

[proxy]
enabled = true
http_bind_addr = "0.0.0.0:8080"
https_bind_addr = "0.0.0.0:8443"
https_redirect = true

[proxy.acme]
enabled = false
email = "you@example.com"
directory_url = "https://acme-staging-v02.api.letsencrypt.org/directory"
```

| Field | Default | Description |
| --- | --- | --- |
| `bind_addr` | `127.0.0.1:4780` | Address the management API listens on. Per FR25, this should stay bound to localhost — the API has no built-in TLS, and binding it elsewhere means relying on the bearer token alone for auth. |
| `proxy.enabled` | `true` | Whether to start the reverse proxy listeners at all. Process supervision and the management API work independently of this. |
| `proxy.http_bind_addr` | `0.0.0.0:8080` | Plain-HTTP listener. Non-privileged by default so `harbord` runs without elevated permissions; point this at `:80` for a real deployment (see [Architecture](/harbor/architecture/) for how to grant that without running as root). |
| `proxy.https_bind_addr` | `0.0.0.0:8443` | TLS listener. Same non-privileged-port reasoning; point at `:443` for production. |
| `proxy.https_redirect` | `true` | Redirect plain HTTP to HTTPS (FR10), except for ACME HTTP-01 challenge requests, which are always answered directly regardless of this setting. |
| `proxy.acme.enabled` | `false` | Request certificates from a real ACME server instead of relying on the always-on self-signed fallback (FR9). Requires the domain to already be publicly resolvable and reachable on `http_bind_addr`'s port from the internet — HTTP-01 validation happens over plain HTTP even when `https_redirect` is on. |
| `proxy.acme.email` | — | Contact email for the ACME account. Most ACME servers, including Let's Encrypt, require this when `acme.enabled` is true. |
| `proxy.acme.directory_url` | Let's Encrypt **staging** | The ACME server's directory URL. Defaults to staging rather than production so testing doesn't risk hitting Let's Encrypt's production rate limits — switch to `https://acme-v02.api.letsencrypt.org/directory` deliberately once you're ready for a real, trusted certificate. |

## App config — `apps/<name>.toml`

```toml
name = "my-api"
path = "/home/me/projects/my-api"
runtime = "python"
command = ["python3", "main.py"]
port = 8000
domain = "my-api.example.com"
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
| `port` | no | Local port the app listens on. Required for the reverse proxy to route to it at all — an app with no `port` is supervised but never proxied. |
| `domain` | no | Routes requests whose `Host` header (or TLS SNI) matches this exactly to the app (FR8). Also provisions a certificate for this domain — self-signed always, ACME-issued too if `proxy.acme.enabled`. |
| `path_prefix` | no | Routes requests whose URL path starts with this prefix to the app, with the prefix stripped before forwarding (FR8). Independent of `domain` — set either, both, or neither. Useful for local testing without owning a domain, or for path-based multi-app routing under one domain. |
| `env` | no | Extra environment variables passed to the process (merged with the daemon's own environment). |
| `restart_policy` | no | `never`, `on-failure` (default), or `always`. |
| `restart_backoff_seconds` | no | Base backoff before the first restart attempt. Default `1`. |
| `restart_max_backoff_seconds` | no | Cap on the exponential backoff between restart attempts. Default `30`. |

If both `domain` and `path_prefix` route to different apps for the same
request, an exact `domain` match always wins; among `path_prefix` matches,
the longest prefix wins.

## Restart policy and backoff

When a managed process exits, Harbor decides whether to restart it based on
`restart_policy`:

- **`never`** — the app stays stopped, regardless of exit code.
- **`on-failure`** (default) — restarted only on a non-zero exit.
- **`always`** — restarted even on a clean exit.

Each restart attempt waits `restart_backoff_seconds * 2^restart_count`,
capped at `restart_max_backoff_seconds`. A deliberate `harbor stop` during
that backoff window cancels the pending restart.
