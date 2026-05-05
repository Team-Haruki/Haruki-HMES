# Haruki-HMES — Project Guide for AI Agents

## Overview

**Haruki-HMES** is a lightweight, stateless SSE push gateway written in Rust.
Its only role is to forward real-time birthday-material-monitor events to
connected clients. Subscription facts live in Cloud (PostgreSQL); filtered
short-lived payloads are stored in Toolbox Redis. HMES holds no persistent
state and must degrade gracefully — if HMES is down, uploads and normal bot
commands still succeed.

## Tech Stack

| Layer | Choice |
|---|---|
| Runtime | Tokio (multi-thread) |
| HTTP / SSE | axum 0.8 |
| HTTP client | reqwest 0.13 (rustls, no native TLS) |
| Logging | tracing + custom `ColoredFormatter` in `logging.rs` |
| Serialisation | serde\_json |
| Error handling | anyhow (binary paths only) |

## Repository Layout

```
src/
  main.rs      — entry point, router, graceful shutdown
  lib.rs       — re-exports all modules (needed for integration tests)
  config.rs    — Config::from_env(), all HMES_* env vars
  state.rs     — AppState, Event, subscription_key, bearer_auth
  cloud.rs     — validate_with_cloud() — calls Cloud validation endpoint
  handlers.rs  — axum handlers: healthz, sse, internal_event, close_subscription
  logging.rs   — tracing ColoredFormatter; call logging::init() once at startup
tests/
  integration.rs — full HTTP integration tests (spin up real axum server)
.github/
  workflows/
    ci.yml      — check + clippy + test on push/PR to main
    release.yml — cross-compile release binaries on v* tags
    docker.yml  — build & push multi-arch Alpine image on v* tags
Dockerfile      — two-stage: rust:alpine builder → alpine:3.21 runtime
```

## Key Design Rules

1. **No mod.rs.** All source files are flat under `src/`. Add a new module
   as `src/foo.rs` and declare it in both `src/lib.rs` and `src/main.rs`.
2. **Flat subscription state.** `AppState` uses a single `Mutex<Inner>` with
   a `HashMap<String, Subscription>`. Each subscription tracks one optional
   latest `Event` and a set of live SSE clients via `tokio::sync::watch`.
3. **Only-latest semantics.** When a new event arrives for a subscription,
   it overwrites any queued-but-not-yet-delivered event. Clients always
   receive the newest event, never a stale one.
4. **Close sentinel.** `close_subscription` sends `None` through each
   client's watch channel, causing the SSE stream to exit cleanly.
5. **No persistent storage.** HMES holds everything in memory; a restart
   is safe as long as Cloud keeps pending-event data.

## Commands

```bash
# Type-check
cargo check --locked

# Lint (warnings are errors in CI)
cargo clippy --locked --all-targets -- -D warnings

# Test
cargo test --locked

# Release build
cargo build --locked --release
```

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `HMES_ADDR` | — | Full listen address (overrides HOST+PORT) |
| `HMES_HOST` | `0.0.0.0` | Bind host |
| `HMES_PORT` | `7910` | Bind port |
| `HMES_INTERNAL_TOKEN` | — | Bearer token for `/internal/*` routes |
| `HMES_CLOUD_INTERNAL_BASE_URL` | — | Cloud base URL for validation |
| `HMES_CLOUD_INTERNAL_TOKEN` | — | Bearer token sent to Cloud |
| `HMES_USER_AGENT` | `Haruki-HMES` | User-Agent header for Cloud requests |
| `HMES_SSE_HEARTBEAT_SECONDS` | `15` | SSE keep-alive comment interval |
| `HMES_CLOUD_TIMEOUT_SECONDS` | `5` | Timeout for Cloud HTTP calls |

## HTTP Routes

| Method | Path | Auth | Description |
|---|---|---|---|
| `GET` | `/healthz` | — | Liveness probe |
| `GET` | `/sse` | Cloud-validated token | SSE stream for clients |
| `POST` | `/internal/events` | `HMES_INTERNAL_TOKEN` | Receive event from Toolbox |
| `POST` | `/internal/subscriptions/{id}/close` | `HMES_INTERNAL_TOKEN` | Force-close SSE connections |

## Testing Conventions

- Integration tests live in `tests/integration.rs`.
- Each test spins up a real `TcpListener` on `127.0.0.1:0` and an optional
  mock Cloud server.
- Use `tokio::time::timeout` to avoid hanging tests.
- After closing a subscription, assert the watch channel delivers `None`
  (sentinel) and then becomes closed.

## Git Commits

All commit subjects must follow:

```text
[Type] Short description starting with capital letter
```

Allowed types:

| Type | Usage |
|---|---|
| `[Feat]` | New feature or capability |
| `[Fix]` | Bug fix |
| `[Chore]` | Maintenance, refactoring, dependency or build changes |
| `[Docs]` | Documentation-only changes |

Rules:

- Description starts with a capital letter.
- Use imperative mood: `Add ...`, not `Added ...`.
- No trailing period.
- Keep the subject at or below roughly 70 characters.
- **Agent attribution uses the standard Git `Co-authored-by:` trailer in
  the commit body, not a free-form `Agent:` line.** This makes GitHub
  render the co-author avatar on the commit page. The trailer must be on
  its own line, separated from the subject by a blank line, in the form
  `Co-authored-by: <Display Name> <email>`. Suggested values per agent:
  - Claude (any 4.x): `Co-authored-by: Claude Opus 4.7 <noreply@anthropic.com>`
    (substitute the actual model, e.g. `Claude Sonnet 4.6`)
  - Codex: `Co-authored-by: Codex <noreply@openai.com>`
  - Copilot: `Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>`

Project examples:

```text
[Feat] Add sendBase64Image config support
[Fix] Normalize Haruki Cloud user agent version
[Chore] Move Rust modules to flat files
[Docs] Document full obfuscated release builds
```

Agent-authored commit example:

```text
[Docs] Add agent commit guidelines

Co-authored-by: Codex <noreply@openai.com>
```
