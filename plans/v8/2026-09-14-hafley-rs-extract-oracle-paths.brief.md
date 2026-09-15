# Brief: sprefa-extract oracle paths after the move to hafley-rs

## 1. Job
In `hafley66/hafley-rs`, `crates/sprefa-extract` still carries `v6/sprefa-extract/...` paths in nine test fixtures and two `golden_parity.rs` regeneration hints, so two tests fail after the move. Fix the paths, regenerate nothing by hand, make `cargo test -p sprefa-extract` green except the five tests that need `rust-analyzer` on PATH, and mark those five `#[ignore]` with the reason string naming the binary.

## 2. Base and first action
- Repo `hafley66/hafley-rs`, base sha `3ee65276` (`origin/main`). Branch `fix/extract-oracle-paths-20260914`, worktree under `/Users/chrishafley/projects/hafley-rs-wt/`.
- FIRST command: `git merge --ff-only 3ee65276`. Failure = stop and report.
- Commits end with `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`.

## 3. Ownership
`crates/sprefa-extract/tests/**`, `crates/sprefa-extract/schema/*.md`, `crates/sprefa-extract/reports/*.md`. Forbidden: `crates/sprefa-extract/src/**`, every other crate. Never spawn subagents.

## 4. What exists
| thing | where |
|---|---|
| the failing sites | `crates/sprefa-extract/tests/golden_parity.rs:446`, `:1877` |
| files carrying the old path | `git grep -ln "v6/sprefa-extract" crates/sprefa-extract` (nine `.v5.jsonl` fixtures, `tests/19_docs_lang_arms.rs`, three `schema/*.md`, two `reports/*.md`) |
| the receipt from the sprefa e2e lane: 916/939 pass, 7 fail: 2 move defects, 5 rust-analyzer missing | sprefa PR #754 body |

## 5. Design (decided)
- A `.v5.jsonl` fixture line that embeds an absolute or `v6/sprefa-extract`-relative path is rewritten to the crate-relative form `crates/sprefa-extract/...`; if the path is part of a hashed row identity, the test's normaliser strips the prefix instead and the fixture stays byte-identical (say which in the PR).
- The `#[ignore = "needs rust-analyzer on PATH"]` attribute goes on the five tests; a `cargo test -- --ignored` run on a machine with rust-analyzer stays the receipt.

## 6. Validation
```bash
cargo test --locked -p sprefa-extract 2>&1 | tail -3      # 0 failed
cargo clippy --locked -p sprefa-extract --all-targets -- -D warnings
```
Build over 60 s is reported with the number, never normalised.

## 7. Reporting
PR in hafley-rs titled `fix(sprefa-extract): oracle paths after the move`. Then `boop beep --no-wait --as <your-lane-name> sprefa-coordinator "extract paths: hafley-rs PR #<n>, tests <pass>/<total>, ignored 5"`. Blocked: same command, stop. One lane, one task.
