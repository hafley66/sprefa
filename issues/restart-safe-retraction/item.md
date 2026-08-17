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
lane_seq: 20
collision: [source-bind-runtime, store-schema]
---

# Restart-safe deletion projection for SourceBind

## Description

## Goal
Restart-safe deletion projection. The identity store is durable, but SourceBind keeps authored file/span/extraction rows in in-memory maps needed to construct later retractions. Make deletion projection survive a restart.
## Where to put it
- v6/sprefa-engine-rs/src/source_bind/_1_runtime.rs — the in-memory map that holds rows for later retraction.
- Identity store durability lives in v6/sprefa-store; reconcile the in-memory maps against it so a restart rebuilds the retraction set.
## Perf gate
- v6/sprefa-engine-rs: just dd-grade / just rust-grade (retract arm graded against the oracle tick log)
- v6/justfile: just memory-soak (assert/retract churn; memory, sqlite page count, statements/tick stay flat)
## Implementation Notes
On restart the retraction projection must be reconstructible from the durable identity store alone, not from the lost in-memory maps.

## Comments

### 2026-08-17T13:12:20Z · @stale-grader

Verdict: COMPLETE against the Goal, landed as PR #346 off 10166672f.

Rescued from the stranded worktree /private/tmp/sprefa-restart-safe-retraction, branch fix/restart-safe-retraction, commit 0a3b8a9f4 (135 commits behind main, never PR'd). Nothing on origin/main had superseded it: `SourceBind` still held `sources` / `contents` / `spans` / `extracted` in-memory maps at v6/sprefa-engine-rs/src/source_bind/_1_runtime.rs and read them back to build deletion arrivals.

What lands: the four maps are deleted; authored file, span and extraction arrivals become durable rows in `_source_bind_receipt_v1` inside the identity store's own database file, keyed on the source host's dense `rev_file_id` with an `ordinal` preserving addition order. `take` reads then clears a batch in one transaction, so a replacement always projects its deletions before its additions.

Changed from the stranded commit, both defects against standing laws:
- The write was one INSERT per authored fact, each taking its ordinal from a subquery over its own insert target, each in its own implicit transaction. A statement reading its own INSERT target costs a transient ephemeral write per row (.claude/skills/sqlite-costs), and a per-row implicit commit fsyncs the identity store once per fact, so an extraction with a thousand facts paid a thousand commits. Now one transaction, one ordinal read, one `prepare_cached` insert across the batch, spans grouped by `rev_file_id` first.
- The receipt connection had no `busy_timeout` while the identity store holds its own connection to the same file.
The commit's rustfmt churn in hosts.rs, change_facts.rs, dep_resolve.rs and incremental.rs (which was the whole cherry-pick conflict) is dropped.

Gates, measured in the worktree, each named gate twice:
- cargo test --offline: 93 passed / 0 failed / 1 ignored, both runs.
- just rust-grade: graded=452 byte-clean=335, exit 1 — byte-identical to a 10166672f baseline measured in the same worktree (same single ratchet name `recursive_closure_passes_both_build_guard_arms`).
- just dd-grade: graded=245 byte-clean=173, exit 1 — identical to the same baseline; peak_rss 6-7 MB against ceiling 8.
- just memory-soak: statements_per_tick_flat PASS (max 39 in both quarters over 2501 ticks), rss_flat PASS, heap_used_flat PASS, dbstat_available PASS. sqlite_page_count_flat FAILs and is the allowlisted pre-existing red at .github/CI-KNOWN-RED.md:31 with the same numbers.
- clippy --lib --bins and --test _0_source_bind with -D warnings: clean. cargo fmt --check: clean on every touched file.

Fail-first receipt: `persistent_receipts_retract_after_runtime_restart_before_replacement_additions` DROPS the first `SourceBind` and opens a second one on the same store; it asserts the deletion set is row-equal and order-equal to the first process's additions, that every deletion precedes every addition, per-rel deltas non-empty on both signs, stale specifiers gone, and that re-asking the same content answers zero arrivals.

Named remainder, not blocking: the receipt table lives in sprefa-engine-rs on a second connection to the identity store's file. Its natural home is the `sprefa-source-identity-store` crate (a different repository), where it could carry `REFERENCES rev_file(rev_file_id)` under that store's `foreign_keys = ON` and share one connection. There is also no statement-count test on the receipt write, because `ReceiptStore` is private with no counting seam.

/private/tmp/sprefa-restart-safe-retraction is left on disk for Chris to prune.
