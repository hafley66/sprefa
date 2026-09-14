# Brief: hafley-observe and tracing spans in sqlite_ivm

## 1. Job
`sqlite_ivm` has zero `tracing` calls and no `hafley-observe` dependency; every other Rust crate in the org has both. Add the dependency, initialise it once at extension load, and put spans on the maintenance path so "why is it slow, what was it doing" is answerable from the trace. No behaviour change; every existing test stays byte-identical.

## 2. Base and first action
- Base sha: `BASE_SHA` (`origin/main`). Branch `chore/sqlite-ivm-observe-20260914`, worktree under `/Users/chrishafley/projects/sprefa-wt/`.
- FIRST command: `git merge --ff-only BASE_SHA`. Failure = stop and report.
- Commits end with `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`. Commands run from `sqlite_ivm/`.

## 3. Ownership
You own `sqlite_ivm/Cargo.toml`, `sqlite_ivm/Cargo.lock`, `sqlite_ivm/src/3_extension.rs`, `sqlite_ivm/src/1_maintenance.rs`, `sqlite_ivm/src/1a_relational.rs` (span attributes only, no logic), `sqlite_ivm/README.md` (one paragraph). Forbidden: everything else. Never spawn subagents.

## 4. What exists
| thing | where |
|---|---|
| how v8 depends on it: `hafley-observe = { version = "0.1", registry = "hafley" }`, init in `main` with `Config::from_env` | `v8/Cargo.toml:12`, `v8/src/bin/dl8.rs` `main` |
| the crate | `hafley-rs/crates/hafley-observe` (sibling link at the sprefa root) |
| extension entry point | `sqlite_ivm/src/3_extension.rs` |
| maintenance entry, per-source-row dispatch, fixpoint rounds | `sqlite_ivm/src/1_maintenance.rs`, `1a_relational.rs` `fn fixpoint`, `fn rounds` |
| eprintln law | `CLAUDE.md`: none in `src/**` |

## 5. Design (decided)
- Init once, guarded by `std::sync::Once`, in the extension's `sqlite3_extension_init`; the subscriber is `hafley_observe::init(Config::from_env("sqlite_ivm", ...))`; default filter `warn`, so a user who loads the extension sees nothing unless `RUST_LOG`/the observe env says otherwise.
- Spans: `maintain` (view name, source table, delta sign), `fixpoint` (view, rows in, rounds), `round` (round index, rows written). Fields are integers and names, never row contents.
- The extension feature must not pull tracing-subscriber into the non-extension library build if that changes `cargo test` timing by more than noise; measure `cargo build --release --features extension` before and after and put both in the PR.

## 6. Validation
```bash
cd sqlite_ivm && cargo test --locked && cargo clippy --locked --all-targets -- -D warnings && grep -rn "eprintln!" src | wc -l
RUST_LOG=sqlite_ivm=debug cargo test --locked --test 4_features recursion_statement_count 2>&1 | grep -c "fixpoint"   # > 0
```

## 7. Reporting
PR title `chore(sqlite_ivm): hafley-observe with maintenance spans`. Then `boop beep --no-wait --as <your-lane-name> sprefa-coordinator "ivm observe: PR #<n>, tests <pass>/<total>, clippy 0, build delta <ms>"`. Blocked: same command, stop. One lane, one task.
