# Haruki-HMES — Claude Agent Instructions

> This file is read automatically by Claude when working in this repository.
> Keep it accurate and concise — Claude reads it on every task.

## What This Project Is

A stateless SSE push gateway written in Rust (axum + tokio). It forwards
real-time birthday-material events to connected clients. No database, no
business logic — just in-memory fan-out and Cloud token validation.

## Source Layout (flat, no mod.rs)

```
src/main.rs      — entry point, router, graceful shutdown, Windows ANSI init
src/lib.rs       — pub re-exports of all modules (required for integration tests)
src/config.rs    — Config::from_env()
src/state.rs     — AppState, Event, subscription_key, bearer_auth
src/cloud.rs     — validate_with_cloud()
src/handlers.rs  — axum handlers
src/logging.rs   — tracing ColoredFormatter (ANSI, Windows-safe)
tests/integration.rs
```

**Adding a module:** create `src/foo.rs`, then add `pub mod foo;` to
`src/lib.rs` and `use haruki_hmes::foo;` (or re-export) in `src/main.rs`.

## Commands to Run After Every Change

```bash
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

Both must pass with zero warnings before committing.

## Code Conventions

- Use `tracing::{info, warn, error}` macros — never `println!` or `eprintln!`.
- Structured fields: `tracing::info!(key = %value, "message")`.
- Return `impl IntoResponse` from handlers; use `axum::Json(json!({...}))` for JSON bodies.
- Prefer `anyhow::Result` in non-handler async functions; use explicit status codes in handlers.
- `AppState` is wrapped in `Arc<AppState>` everywhere — do not clone the inner state.
- Keep the `Mutex` lock scope as short as possible; never `.await` while holding it.
- All string fields from HTTP input must be `.trim()`-ed before use.

## What NOT to Do

- Do not add `mod.rs` files.
- Do not add a database or any file-system persistence.
- Do not use `unwrap()` in non-test code except where panic is truly impossible.
- Do not change the HTTP route paths or JSON field names — they are part of the
  external API consumed by Cloud, Toolbox, and Client.
- Do not add `native-tls`; keep TLS via `rustls` only.

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

Co-authored-by: Claude Sonnet 4.6 <noreply@anthropic.com>
```
