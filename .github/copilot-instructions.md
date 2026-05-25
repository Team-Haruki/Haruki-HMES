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
  - Claude (any 4.x): `Co-authored-by: Claude Opus 4.7 <noreply@anthropic.com>` (substitute the actual model, e.g. `Claude Sonnet 4.6`, `Claude Haiku 4.5`)
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

Use the standardized workflow layout in `.github/workflows`:

- `ci.yml` runs on `main` pushes, pull requests targeting `main`, and manual dispatch.
- Rust CI order: `cargo fmt --all -- --check`, `cargo check --locked --all-targets`, `cargo clippy --locked --all-targets -- -D warnings`, then `cargo test --locked`.
- `release.yml` is the standard release build entrypoint. It runs on `v*` tags and manual dispatch, builds release artifacts, uploads them with `actions/upload-artifact`, and publishes GitHub Release assets on tag pushes.
- `docker.yml` is the standard Docker entrypoint. It runs on `main` pushes, `v*` tags, PRs that touch Docker/build inputs, and manual dispatch. PRs build only; non-PR runs push GHCR images with lowercase image names and Docker metadata tags.

Workflow maintenance rules:

- Keep workflow filenames and top-level names aligned: `CI`, `Release`, `Docker`, and optional package-specific names.
- Use `actions/checkout@v6`, `actions/setup-go@v6`, `actions/upload-artifact@v7`, `actions/download-artifact@v8`, `softprops/action-gh-release@v3`, and current Docker actions (`setup-buildx@v4`, `login@v4`, `metadata@v6`, `build-push@v7`).
- Keep `permissions` minimal: `contents: read` for CI/Docker build-only work, `contents: write` for release publishing, and `packages: write` only when pushing container images.
- Use workflow `concurrency` keyed by workflow name and ref, with release jobs using `release-${{ github.ref_name }}` and `cancel-in-progress: false`.
- Do not reintroduce legacy workflow names such as `rust-ci.yml`, `build.yml`, `release-build.yml`, `docker-build.yml`, or `docker-release.yml` unless a package-specific workflow already exists and is intentionally preserved.
