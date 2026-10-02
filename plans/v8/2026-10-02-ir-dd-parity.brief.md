# Brief: ir:dd parity with the Rust evaluator (step 1 of `2026-10-02-engine-parity.report.md`)

Goal: `DL8_ENGINE=ir:dd cargo test --workspace --no-fail-fast -- --skip _7_book --test-threads=1`
has zero failures, with results identical to the Rust evaluator. Same for `ir:sqlite` after.

Read first: `plans/v8/2026-10-02-engine-parity.report.md` (the 14 failures and their first errors),
`plans/v8/2026-10-02-two-engines.plan.md` (where this goes: ir engines only).

## Setup

- Your cwd is a fresh git worktree of sprefa. `sqlite_ivm` is a gitignored symlink in the main
  checkout; create it in your worktree: `ln -s /Users/chrishafley/projects/sqlite_ivm sqlite_ivm`.
- sqlite_ivm changes (engine bugs, floats in `ivm_ir`): make them in a sqlite_ivm worktree,
  `git -C /Users/chrishafley/projects/sqlite_ivm worktree add .boop-worktrees/fix/ir-parity -b fix/ir-parity`,
  and point the symlink there while working. Report both branches.

## Do

1. The 14 failures, by group: harness tests that pin Rust (make them compare ir:dd/ir:sqlite to the
   Rust reference rather than refuse), floats through `ivm_ir`, comptime effect answer counts,
   runtime host (timer, git_refs, second-run), rxjs emit Path rows.
2. Where ir and Rust disagree, Rust is the reference unless the frozen golden says otherwise; report
   any case where Rust disagrees with a golden.
3. No new recursion. No eager string structure. No silent fallbacks: every unsupported shape is a
   named diagnostic.

## Laws

- One cargo process at a time. `uptime` first; wait if load > 12. `-j 4`. `--test-threads=1` on
  dl8-spawning tests.
- Plain `cargo` (rcargo). `[profile.dev.package."*"] opt-level = 3`, `[profile.dev] opt-level = 1`.
- No `json_*(` SQL. No book edits. Never `git commit -n`. No merge, no push.
- Goldens only via `oracle/refreeze.py` and the regen script; never hand-edit expected files.
- Kernel change (new kernel relation): stop and report with dl7 examples.
- Scoped green commits per failure group. End commit messages with
  `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`.

## Validation

```
DL8_ENGINE=ir:dd cargo test --workspace --no-fail-fast -- --skip _7_book --test-threads=1
DL8_ENGINE=ir:sqlite cargo test --workspace --no-fail-fast -- --skip _7_book --test-threads=1
cargo test --workspace --no-fail-fast -- --skip _7_book --test-threads=1
```

## Receipt

```
status: done | blocked
sprefa branch + sha:
sqlite_ivm branch + sha: (or none)
commits: <sha subject>
validation: <command + result line each; every failure with its message>
rust-vs-golden disagreements: <list or none>
next: <one action>
```
