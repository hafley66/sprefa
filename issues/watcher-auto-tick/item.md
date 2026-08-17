---
created: 2026-08-14
updated: 2026-08-17
type: task
status: open
priority: high
epic: v5-behavioral-parity
labels:
- parity
- v6
- size:med
lane: engine-source-bind
lane_seq: 30
collision: [source-bind-runtime, engine-driver]
---

# Watcher events auto-enter the engine tick schedule

## Description

## Goal
Watcher events enter the engine tick schedule automatically, so worktree/commit edits drive SourceBind arrivals with no manual restart. Parity with V5's live watcher.
## Where to put it
- v6/sprefa-engine-rs/src/source_bind/_1_runtime.rs — the SourceBind runtime tick path (397 lines; split if this grows the file past ~500).
- v6/sprefa-engine-rs/src/hosts.rs — follow the SoopyFilesExecutor pattern (name + execution tag match, no child spawn).
- v6/sprefa-engine-rs/src/driver.rs — the tick schedule the watcher feeds.
Keep AGENTS.md laws: no god files (split past ~500 lines), one concern per module.
## Perf gate
- v6/justfile: just watch-scale (coalescing correctness + cost, 100/1000 files)
- v6/justfile: just scale-floor (arrivals at scale, stmts/tick flat)
## Implementation Notes
Duplicates / identical re-saves / deletes must coalesce; correctness gated, cost reported (watch-scale does not gate wall ms).

## Comments

### 2026-08-17T13:12:20Z · @stale-grader

Verdict: the tick path is COMPLETE and landed as PR #345 off 10166672f; the SCHEDULE half is still open and cannot close in this crate yet.

Rescued from the stranded worktree /private/tmp/sprefa-watcher-auto-tick, branch feature/watcher-auto-tick, commit bca0d7ae4 (135 commits behind main, never PR'd). It cherry-picked onto main with no conflict; nothing on origin/main had superseded it.

What lands: one debounced Soopy source watcher per registered worktree in the new v6/sprefa-engine-rs/src/source_bind/_2_watch.rs, keyed on `WorktreeId` so linked checkouts cannot feed each other's ticks; `request_for_deltas/2` turns one coalesced Soopy batch into one source-host identity request, and an empty batch answers None and creates no tick; `SourceBind::run_watch_tick` routes it through the ordinary identity, extraction and tick path, no second door; `driver.rs drive_watch_tick` is the entry point. `watch_git` registers the watcher BEFORE the baseline snapshot, so a mutation racing startup is still visible in the first receipt. Reviewed on the way through: `watch_git` had copied `register_git`'s whole body; it now calls it.

Gates, measured in the worktree:
- cargo test --offline: 93 passed / 0 failed / 1 ignored, twice.
- just rust-grade: graded=452 byte-clean=335, exit 1 — byte-identical to a 10166672f baseline measured in the same worktree.
- just watch-scale (named on this card): correct=true both cells, duplicate/stale/missing all 0; 100 files 53 ms / 1000 files 434 ms, write_amplification 1.19, sql_statements 203 and 1328.
- clippy --lib --bins and --test _0_source_bind with -D warnings: clean. cargo fmt --check: clean on every touched file.

Correctness receipt: `watcher_changes_enter_one_source_bind_tick_and_match_disk` walks create, identical re-save (asserts NO tick is produced), content change, rename (asserts exactly one Del and one Add), delete, then checks the final `file` rows against disk. That is the card's coalescing requirement.

On the two perf gates this card names: just watch-scale (v6/justfile:350) and just scale-floor (v6/justfile:382) both run v6/tsv2/scripts/*. They grade the TypeScript runtime and cannot grade this Rust path at all; watch-scale is reported above only as a no-regression receipt. The engine-rs gate is just rust-grade.

Remainder, why the card stays open: `drive_watch_tick` is pull-shaped and its only caller is the test, because sprefa-engine-rs ships no long-running binary — src/bin/emit_rust_harness.rs is the only one. "Watcher events auto-enter the tick schedule with no manual restart" needs a process that owns the loop, and its CPU/IO budget under the nothing-seizes-the-machine law. That belongs to the rust serve arc, not to this crate today.

/private/tmp/sprefa-watcher-auto-tick is left on disk for Chris to prune.
