# lab-dl6-adapters-dir-pro4 (pass 1 of 2; a coordinator design review follows) [pro4 arm: identical brief, second model; suffix every PR title with " (pro4)"]

You are lane `lab-dl6-adapters-dir-pro4`. Coordinator is `sprefa-coordinator`.
Base sha 9e4b468157bb2a189960b8ec69daad10af372862. Branch `lab/dl6-adapters-dir-pro4`.
FIRST ACTION: `git merge --ff-only 9e4b468157bb2a189960b8ec69daad10af372862`; on failure STOP and hail.

## Defect (measured 2026-08-24, resident relaunch crash 1)
`cd v6/dl/ghcache && dl6 run ghcache.dl6` (bare filename, no directory) panics/errs in `point_at_adapters`, `v6/sprefa-engine-rs/src/bin/dl6.rs:434-450`. Cause: `Path::new("ghcache.dl6").parent()` returns `Some("")`, so `directory` is the empty path and `"".canonicalize()` fails with "read the adapters directory ". The `unwrap_or_else(|| PathBuf::from("."))` arm never fires because parent is `Some`, not `None`. Same shape for `args.adapters` given as a bare filename.

## Fix (exact)
In `point_at_adapters`, after computing `directory`, map an empty path to `.`:
```rust
let directory = if directory.as_os_str().is_empty() { PathBuf::from(".") } else { directory };
```
Apply once, covering both arms (compute the candidate first, then normalize). No other behavior change. Keep the existing error context string.

## Test (fail-pre-fix, required)
Add ONE test to `v6/sprefa-engine-rs/tests/dl6_run.rs`, following the file's existing `Command::new(dl6).arg("run")` shape (see lines 112, 129, 166 for the pattern and how the binary path and fixtures are resolved). The test:
- copies a small existing fixture `.dl6` (pick one already used by that file, e.g. the one at line 146 `query_order_tail.dl6`) into a tempdir,
- runs `dl6 run <bare filename>` with `current_dir(tempdir)`,
- asserts exit success and that stderr does not contain `read the adapters directory`.
Test header comment states the fail-first receipt: the pre-fix error text, verbatim, and this brief's crash context (bare `dl6 run ghcache.dl6` on 2026-08-24). Run it BEFORE the fix and paste the failing output into the PR body; then after the fix.

## Commands
- `cd v6/sprefa-engine-rs && cargo test --release --test dl6_run` (background, `timeout 600`, tail the log; never foreground-wait over 10 s)
- full engine battery: `cd v6/sprefa-engine-rs && cargo test --release` (background, timeout 900) expected 0 failed
- `bash v6/sprefa-engine-rs/grade.sh` expected `graded=445 byte-clean=341`

## Deliverables
- The fix, the test, a `docs/failure-modes.md` entry (next free number; format: incident, RCA, fail-pre-fix test, rail, entry; copy the shape of entry 92).
- Commit, push, PR to main titled `fix(dl6): a bare-filename source resolves its adapters directory to "."`. Body: fail-pre-fix output, post-fix output, gate numbers.

## Yield results over time (mandatory)
1. after the test fails pre-fix: `boop beep hail sprefa-coordinator --from lab-dl6-adapters-dir-pro4 --body "pre-fix red: <error line>"`
2. after the fix turns it green: hail the test name + pass count.
3. done: hail PR number + grade numbers.
If reality deviates from this brief (parent() is not `Some("")`, the test cannot be made to fail pre-fix, grade.sh is not 445/341 on base), STOP and hail the exact text. Do not improvise.

## You own
`v6/sprefa-engine-rs/src/bin/dl6.rs` (the one function), `v6/sprefa-engine-rs/tests/dl6_run.rs` (one added test), `docs/failure-modes.md` (one appended entry). Forbidden: everything else, including src/*.rs outside bin/dl6.rs and v6/prolog/**.

## Style laws (CLAUDE.md)
No `eprintln!`; tracing only. No em dashes. Banned words: provenance, substrate, load-bearing, regime, ground truth, refusal, support, honest. Comments state only constraints the code cannot show; no change-log narrative in code comments. Commit message: imperative, one line + body with receipts.
