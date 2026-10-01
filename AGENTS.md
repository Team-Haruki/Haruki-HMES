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
  main.rs      — entry point, router, graceful shutdown, Windows ANSI init
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
    ci.yml      — CI: shared-template callers (rust-ci, sonar, docker, actionlint)
                  + the `CI OK` aggregate check
    release.yml — Release: gate → binaries (5 targets) + Docker promote → GitHub
                  Release on v* tags; manual dispatch = dry run
Dockerfile      — cargo-chef (rust:alpine) builder → alpine:3.21 runtime
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
   is safe as long as Cloud keeps pending-event data. Never add a database
   or any file-system persistence.
6. **Frozen external contract.** HTTP route paths and JSON field names are
   consumed by Cloud, Toolbox and Client — do not rename or move them.
7. **rustls only.** Never pull in `native-tls`.

## Commands

```bash
# Format check
cargo fmt --all -- --check

# Type-check
cargo check --locked

# Lint (warnings are errors in CI)
cargo clippy --locked --all-targets -- -D warnings

# Test
cargo test --locked

# Release build
cargo build --locked --release
```

Run the clippy and test commands after every change; both must pass with
zero warnings before committing.

## Code Conventions

- Use `tracing::{info, warn, error}` macros — never `println!` or `eprintln!`.
- Structured fields: `tracing::info!(key = %value, "message")`.
- Return `impl IntoResponse` from handlers; use `axum::Json(json!({...}))` for
  JSON bodies.
- Prefer `anyhow::Result` in non-handler async functions; use explicit status
  codes in handlers.
- `AppState` is wrapped in `Arc<AppState>` everywhere — do not clone the inner
  state.
- Keep the `Mutex` lock scope as short as possible; never `.await` while
  holding it.
- All string fields from HTTP input must be `.trim()`-ed before use.
- No `unwrap()` in non-test code except where a panic is truly impossible.

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `HMES_ADDR` | — | Full listen address (overrides HOST+PORT) |
| `HMES_HOST` | `0.0.0.0` | Bind host |
| `HMES_PORT` | `7910` | Bind port |
| `HMES_INTERNAL_TOKEN` | — | Bearer token for `/internal/*` routes |
| `HMES_CLOUD_INTERNAL_BASE_URL` | — | Cloud base URL for validation |
| `HMES_CLOUD_INTERNAL_TOKEN` | — | Bearer token sent to Cloud |
| `HMES_CLOUD_TLS_SKIP_VERIFY` | `false` | Skip Cloud TLS verification for controlled internal/test environments |
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

## Git commits

All commit subjects must follow:

```text
[Type] Short description starting with capital letter
```

Allowed types:

| Type      | Usage                                                 |
|-----------|-------------------------------------------------------|
| `[Feat]`  | New feature or capability                             |
| `[Fix]`   | Bug fix                                               |
| `[Chore]` | Maintenance, refactoring, dependency or build changes |
| `[Docs]`  | Documentation-only changes                            |

Rules:

- Description starts with a capital letter.
- Use imperative mood: `Add ...`, not `Added ...`.
- No trailing period.
- Keep the subject at or below roughly 70 characters.
- **Agent attribution uses the standard Git `Co-authored-by:` trailer in the commit body, not a free-form `Agent:` line.** This makes GitHub render the co-author avatar on the commit page. The trailer must be on its own line, separated from the subject by a blank line, in the form `Co-authored-by: <Display Name> <email>`. Suggested values per agent:
  - Claude (any model): `Co-authored-by: Claude Fable 5 <noreply@anthropic.com>` (substitute the actual model, e.g. `Claude Opus 4.7`, `Claude Sonnet 4.6`, `Claude Haiku 4.5`)
  - Codex: `Co-authored-by: Codex <noreply@openai.com>`
  - Copilot: `Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>`

Examples from this repo's history:

```text
[Feat] Add HMES_CLOUD_TLS_SKIP_VERIFY and fix Dockerfile
[Fix] Lowercase Docker image name for GHCR compatibility
[Chore] Rewrite HMES in Rust
[Docs] Update birthday monitor rollout status
```

## GitHub Actions workflows

CI reuses the shared templates in
[`seiunx-dev/ci-templates`](https://github.com/seiunx-dev/ci-templates) at `@v1`.
The files in `.github/workflows` are thin callers:

- `ci.yml` (`CI`) runs on `main` pushes, pull requests targeting `main`, and manual
  dispatch: `rust-ci` (fmt, clippy `-D warnings`, tests under `cargo llvm-cov`) →
  `sonar` (scans the uploaded coverage; skipped green on Dependabot/fork PRs), plus
  `docker` and `actionlint`.
- `docker` does not wait for the tests. PRs build only; on `main` it runs in parallel
  with `rust-ci` and pushes the immutable `ghcr.io/team-haruki/haruki-hmes:sha-<full sha>`
  and `:sha-<7 chars>` as soon as the build finishes. The `Docker tags` job
  (`docker-retag.yml`, after `CI OK`) then moves `:main` to that digest without
  rebuilding, so `:main` only follows commits whose `CI OK` passed and lags the `:sha-*`
  tags until then.
- The aggregate job **`CI OK`** is the only required status check.
- `release.yml` (`Release`): bump the version in `Cargo.toml` in a PR → merge and wait
  for `CI OK` on `main` → push the tag `v<version>`. `release-gate` refuses a tag that
  differs from `Cargo.toml` and waits for `CI OK` on the tagged commit; then the
  binaries are built (tags only; five targets, assets
  `haruki-hmes-v<version>-<target-triple>.tar.gz` / `.zip`), the `main` image
  `:sha-<sha>` is promoted (re-tagged, not rebuilt) to `:<version>`, `:<major>.<minor>`
  and `:latest`, and the GitHub Release is published with `SHA256SUMS-<tag>.txt`.
  Manual dispatch is a dry run: it builds the binaries and publishes nothing.

Workflow maintenance rules:

- Use the shared templates first. Add custom jobs or steps only when a template
  genuinely cannot meet the project's needs, keep them in the thin caller files, and
  add a comment explaining why.
- Template bugs and missing features are fixed upstream in `seiunx-dev/ci-templates`
  (new `v1.x.y` tag), not worked around here.
- Keep top-level `permissions: contents: read`; grant `packages: write` / `contents: write`
  only on the job that needs it.
- Do not suppress `githubactions:S7637` (full-SHA pins) in `sonar-project.properties`: the
  template's `sonar.yml` already ignores it for the `@v1` references.
- Third-party actions in caller-side custom steps are pinned to a full commit SHA with a
  `# vX.Y.Z` comment; Dependabot (`github-actions`) updates them and the template refs.
