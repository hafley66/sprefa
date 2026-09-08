---
created: 2026-09-08
updated: 2026-09-08
type: bug
status: open
priority: normal
related: ['@extract-reach-entry-sink']
labels:
- pkg:extract
- size:med
---

# exact mode: rust-analyzer skip on a crate dir that indexes fine by hand; --scip-index ignored; stale index reused silently

## Description

## Receipt

`cd hafley-rs/crates/boop-store && extract --family scip .` returns one row in 0 s:

```
{"record":"scip_skip","lang":"rust","bin":"rust-analyzer","reason":"failed","detail":"scip indexer failed: 8: __pthread_joiner_wake"}
```

`rust-analyzer scip . --output x.scip` in the same directory succeeds in 15 s (3.1 MB index, `rust-analyzer 1.100.0-nightly 2026-08-28`). Its stderr ends with a benign `Duplicate symbol: rust-analyzer cargo boop-store 0.0.2 crate/` and `Generating SCIP finished`. The `8: __pthread_joiner_wake` line extract reports is a backtrace frame, so the indexer either got killed (budget) or its exit was misread.

Two more findings from the same session:

- `extract --family scip . --scip-index FILE` ignores the supplied index and rebuilds. The index only enters through `--resolve --project-root . --scip-index FILE PATH...` or `--scip-facts`. Either accept it in exact mode or say so in `--help`.
- `extract --family scip .` at the hafley-rs workspace root reused an index built 2026-05-18 with rust-analyzer 1.97 (`"reused":true`, 130 documents) against September sources with no staleness signal. A `scip_index` row carrying the index mtime and a `stale` flag when any document's file is newer would make reuse visible.

## Acceptance Criteria

- [ ] `extract --family scip .` in `hafley-rs/crates/boop-store` emits scip_* rows, no `scip_skip`
- [ ] a `scip_skip reason=failed` detail carries the indexer's exit status and last non-backtrace stderr line
- [ ] `--scip-index` under `--family scip` is honoured or rejected with a message
- [ ] `scip_index` reports index mtime and a stale flag

## Comments

### 2026-09-08T17:40:29Z · @chris

Root cause, reproduced by hand: RUST_SPEC staging is Staging::Always, so rust-analyzer runs over a staged copy holding only the crate's .rs files and Cargo.toml. A workspace member manifest (boop-store: edition.workspace = true) then fails cargo locate-project --workspace ('failed to find a workspace root'), rust-analyzer's metadata load panics, and the reported detail is the last backtrace frame (8: __pthread_joiner_wake). Same on main e2cea7786. Fix options: stage from the cargo workspace root when locate-project names one above the requested root, or copy the root Cargo.toml + Cargo.lock into the stage, and report the first ERROR line rather than the last stderr line.

### 2026-09-08T17:45:56Z · @chris

Second cause, workspace root, main e2cea7786: the stage dir is persistent (T/sprefa-scip-stage-rust-analyzer-<hash>, kept for target/ warmth). hafley-rs had crates/css on Sep 5; it is gone from the repo now, but prune_unstaged removes only files, so the stage still holds an empty crates/css/src. cargo's members = ["crates/*"] then fails: 'failed to load manifest for workspace member .../crates/css'. Detail reported was again the last backtrace frame. Fix: prune empty directories after pruning files (or rebuild the stage when the root's Cargo.toml set changed), and surface the first 'error:' line. Workaround that worked today: SPREFA_SCIP_INDEX=<hand-built index> extract --family scip . streams 69,151 rows from a fresh rust-analyzer 1.100 index.
