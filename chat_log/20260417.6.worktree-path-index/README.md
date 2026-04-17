# Session 6 — Worktree-backed path index

Motivation: `fs()` over swc takes 14s per rule. `git2::Tree::walk` is O(tree
objects); 4 rules re-walk the same 83k objects 4 times. Real fix: materialize
each referenced rev once, index paths once, query forever.

## Goals

- `fs()` steady-state: <100ms per rule on swc (target), <10ms on cached index
- Cold path: one `git worktree add` + one `git ls-tree` per (repo, rev)
- Zero extra walks on repeated rules, repeated runs, or LSP reparses
- Bounded rev materialization — wildcard revs (`rev(*)`, `rev(**)`) rejected
  at parse with a diagnostic pointing at the rev op

## DAG

```
6a  rev wildcard ban                       (independent; sharpens spec)
6b  WorktreeProvisioner                    (independent; new module)
6c  PathIndex trait + SqliteStore impl     (independent; new trait + impl)
6d  GitBlobReader 3-phase files()          ← needs 6b, 6c
6e  Config surface + workspace wiring      ← needs 6b, 6c, 6d
6f  Invalidation hooks (moving refs)       ← needs 6c (watcher → drop_rev)
6g  Smoke + benchmarks                     ← needs 6d, 6e
```

Parallelizable: 6a / 6b / 6c land in any order. 6d is the fusion point. 6e
is the last wiring pass. 6f and 6g close the loop.

## Task docs

- [6a_rev_wildcard_ban.md](6a_rev_wildcard_ban.md) — parse-time rejection
- [6b_worktree_provisioner.md](6b_worktree_provisioner.md) — lazy `git worktree add`
- [6c_path_index_trait.md](6c_path_index_trait.md) — trait + SQLite impl
- [6d_reader_three_phase_branch.md](6d_reader_three_phase_branch.md) — `files()` rewrite
- [6e_config_and_wiring.md](6e_config_and_wiring.md) — `RuntimeConfig` + ctor threading
- [6f_invalidation_hooks.md](6f_invalidation_hooks.md) — watcher → `drop_rev`
- [6g_smoke_and_bench.md](6g_smoke_and_bench.md) — perf acceptance

## Invariants preserved

- Ops own everything — `fs` op untouched, diagnostic additions stay in `rev`
- Content contract unchanged — WT bytes are just a faster `reader.bytes()`
- Reader stack shape unchanged — provisioner is a sidecar, not a layer

## Out of scope

- Trigram content index (Zoekt's territory; not needed yet)
- `git archive`-based extraction (WT fits current read pattern better)
- Cross-process locking of the WT cache dir (single-daemon assumption holds)
- LRU eviction of WT cache (tracked; defer until dogfood fills disk)
