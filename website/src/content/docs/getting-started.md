---
title: Getting Started
description: Build Harbor and run your first app.
---

Harbor is a Rust workspace with three crates: `harbor-core` (shared types),
`harbor-daemon` (the `harbord` binary), and `harbor-cli` (the `harbor`
binary). This page builds them from source — there are no published
binaries yet.

## Prerequisites

- A stable Rust toolchain (install via [rustup](https://rustup.rs))

## Build

```sh
git clone https://github.com/ashinberish/harbor
cd harbor
cargo build --workspace
cargo test --workspace
```

## Start the daemon

```sh
cargo run -p harbor-daemon
```

By default the daemon stores app configs, logs, certs, and runtime state
under `~/.harbor` (override with the `HARBOR_HOME` environment variable)
and binds its management API to `127.0.0.1:4780`. On first run it writes a
random bearer token to `~/.harbor/token`, which the CLI reads
automatically — you don't need to handle it yourself. It also starts the
reverse proxy on `0.0.0.0:8080` (HTTP) and `0.0.0.0:8443` (HTTPS) —
non-privileged ports by default so no elevated permissions are needed; see
[Configuration](/harbor/configuration/) to point them at 80/443 instead.

Leave the daemon running in this terminal; everything below uses a second
terminal to talk to it through the CLI. For a real deployment, run
`harbor service install` instead of a foreground `cargo run` — it
registers `harbord` with your OS's service manager so it starts at boot
(see the [CLI Reference](/harbor/cli-reference/#harbor-service)).

## Add and run your first app

Point Harbor at any project directory. It auto-detects the runtime from
marker files (`requirements.txt`/`pyproject.toml` for Python,
`package.json` for Node, `.csproj`/`.sln` for .NET, `pom.xml`/`build.gradle`
for Java, `Cargo.toml` for Rust) and picks a best-effort launch command,
both of which you can override.

```sh
cargo run -p harbor-cli -- add ./my-api --name my-api --port 8000 --domain my-api.localtest.me
cargo run -p harbor-cli -- start my-api
```

Adding `--port` and `--domain` (or `--path-prefix`) makes the app
reachable through the reverse proxy immediately — try
`curl -k https://my-api.localtest.me:8443/` (the `-k` is because the
certificate is self-signed until you enable ACME with a real domain).

Check on it:

```sh
cargo run -p harbor-cli -- status
cargo run -p harbor-cli -- logs my-api -f
```

Stop it when you're done, or remove it entirely:

```sh
cargo run -p harbor-cli -- stop my-api
cargo run -p harbor-cli -- remove my-api
```

## Next steps

- [CLI Reference](/harbor/cli-reference/) — every command and flag.
- [Configuration](/harbor/configuration/) — the TOML schema behind each app.
- [Architecture](/harbor/architecture/) — how the daemon supervises processes
  and recovers from crashes.
