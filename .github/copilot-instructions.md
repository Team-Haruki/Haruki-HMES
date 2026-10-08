# Haruki-HMES — GitHub Copilot Instructions

## Project Summary

Stateless SSE push gateway in Rust (axum 0.8 + tokio). Receives event
notifications from Toolbox, validates SSE clients against Cloud, and fans
events out to connected clients. No database. Everything is in-memory.

## Source Layout (flat files, never mod.rs)

| File | Responsibility |
|---|---|
| `src/main.rs` | Tokio entry point, graceful shutdown, Windows ANSI colour init |
| `src/lib.rs` | The only `pub mod` declarations (used by `main.rs` and `tests/`) |
| `src/config.rs` | `Config::from_env()` — all `HMES_*` env vars |
| `src/state.rs` | `AppState`, `Event`, `subscription_key`, `bearer_auth` |
| `src/cloud.rs` | `validate_with_cloud()` — async Cloud HTTP call |
| `src/handlers.rs` | `router()` and the axum route handlers |
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
- **No mod.rs:** new modules go in `src/foo.rs`; declare them only in `lib.rs`
  (`main.rs` uses the `haruki_hmes` library crate).

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
- The aggregate job **`CI OK`** is the single gate: it fails if any job above failed or
  was cancelled, `Docker tags` and the release gate wait for it, and it is the one check
  to mark as required. `main` currently has no branch protection or ruleset, so GitHub
  does not enforce it on merges.
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
- Third-party actions in caller-side custom steps are pinned to a full commit SHA with a
  `# vX.Y.Z` comment; Dependabot (`github-actions`) updates them and the template refs.
