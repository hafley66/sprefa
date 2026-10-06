# Brief: fix the duplicate-work ledger (two lanes), type-check only

User (2026-10-06): "fix them all with just type checking, i want 1 run of tests by you (the
coordinator)". Ledger: `plans/v8/2026-10-06-duplicate-work-audit.findings.md` (fix numbers below
refer to its ranked list; read each row's evidence and safety condition there).

## Lane R (Rust side, sprefa): branch `perf/dupwork-rust`

Fixes 1 (relocate once per part session), 2 + 7 (delta-driven round closure scans and settle-delta
decode), 3 (decode watermark + persistent decoded set), 5 (pass known fingerprints), 6 (adopt pool
part without re-lowering), 8 (seed frontier from new seeds/terms only), 10 (narrow_unions without
the recording compile pass), 11 (skip conversion/check/lowering when rules, seeded keys, served
relations and shapes are unchanged).
Files: `src/_6_eval/_9b_ir_eval.rs`, `src/_4_comptime/*`, `src/_6_eval/_9a_ir_lower.rs` only where
10 needs it. Stay out of `sqlite_ivm`.

## Lane E (engine/IR side): branch `perf/dupwork-engine`

Fixes 4 (one unsealed macro-rule install serving the library round and the cone), 9 (leave() the
in-loop collection instead of an outer rebuild), 12 (dedup compose terms/texts by value), 13
(hash-cons Let strata by body / adornment key without level), 14 (hash-cons source-free nodes in
compose), 15 (arrange outside the LetRec and enter the arrangement), 16 (memo antijoin sides per
(stream, key)).
Files: `sqlite_ivm/crates/ivm-ir`, `crates/ivm-dd`, sprefa `src/_1_macrotime/*`, `src/_8_driver/*`,
lowering in `_9a_ir_lower.rs` for 13. Stay out of `_9b_ir_eval.rs` except the call site 4 needs.

## Rules for both lanes

- Type-check only: `cargo check -j 4 --workspace --tests` (sprefa; and in the sqlite_ivm clone for
  lane E). Do NOT run tests, suites, censuses or the binary. The coordinator runs the tests once
  after merging both lanes.
- One commit per fix, message names the fix number and the ledger row's safety condition.
- Shared `ivm_ir` / Engine trait stays engine-neutral; ivm-sqlite must still compile.
- No caching (persistent per-session state that the engine owns is fine; nothing that skips work
  across processes). No recursion in kernel Rust, no silent fallbacks, no `json_*(`, no book edits,
  no tree-wide formatting, never `git commit -n`, no merge, no push.
- Commits end with
  `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_019jL2FmxduyQQ9kpmQYBKj7`.

## Receipt

```
status: done | blocked
sprefa branch + sha:
sqlite_ivm branch + sha + clone path (lane E):
per fix: <n: commit, what changed, anything skipped and why>
cargo check: <result line>
```
