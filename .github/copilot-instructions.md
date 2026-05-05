# Haruki-HMES — GitHub Copilot Instructions

## Project Summary

Stateless SSE push gateway in Rust (axum 0.8 + tokio). Receives event
notifications from Toolbox, validates SSE clients against Cloud, and fans
events out to connected clients. No database. Everything is in-memory.

## Source Layout (flat files, never mod.rs)

| File | Responsibility |
|---|---|
| `src/main.rs` | Tokio entry point, axum router, Windows ANSI colour init |
| `src/lib.rs` | `pub mod` declarations (required for `tests/`) |
| `src/config.rs` | `Config::from_env()` — all `HMES_*` env vars |
| `src/state.rs` | `AppState`, `Event`, `subscription_key`, `bearer_auth` |
| `src/cloud.rs` | `validate_with_cloud()` — async Cloud HTTP call |
| `src/handlers.rs` | axum route handlers |
| `src/logging.rs` | Custom `tracing` formatter with ANSI colours |
| `tests/integration.rs` | End-to-end HTTP tests against a live server |

## Mandatory Checks Before Committing

```bash
cargo clippy --locked --all-targets -- -D warnings  # zero warnings required
cargo test --locked                                  # all tests must pass
```

## Key Conventions

- **Logging:** `tracing::{info, warn, error}` with structured fields.
  Never `println!`.
- **Handlers:** return `impl IntoResponse`; JSON via `axum::Json(json!({...}))`.
- **State:** `Arc<AppState>` — never clone inner state. Never `.await` while
  holding the `Mutex` lock.
- **TLS:** `rustls` only — no `native-tls` dependency.
- **Input:** always `.trim()` strings received from HTTP requests.
- **No mod.rs:** new modules go in `src/foo.rs`; declare in `lib.rs` and `main.rs`.

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

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>
```
