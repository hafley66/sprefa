# Depth-lattice native walk + the two daemon-win fixes (A, B)

Base: current `main` (has `src/walk.rs::multi_source_walk` with depth + cap +
optional halt already generalized, and `try_native_halt_bfs` recognizing
`port_reach`). Do NOT rebase the old `codex/depth-walk` branch — its executor
predates the generalized `multi_source_walk` and is redundant. Start fresh.

Read `plans/2026-07-12-native-graph-walk-executor.md` first — its "NEXT",
"Design", "Recognizer changes", "KEY SIMPLIFICATION" sections ARE the spec for
the depth extension. This file adds the two fixes that make it a real daemon win.

## Part 1 — depth-lattice recognizer (the "NEXT" section of the sibling plan)
Generalize `try_native_halt_bfs` in `src/engine/derive.rs` (rename to
`try_native_walk` is fine) to also recognize:

    rel entry_reach_node(node: sym, d: int) key(node) merge(MinBy(d)).
    entry_reach_node(node, 0) <- entry_seed(node).
    entry_reach_node(to, d0 + 1) <- entry_reach_node(from, d0), flow_edge(from, to), d0 < 64.

    rel op_reach_node(op: text, n: sym, d: int) key(op, n) merge(MinBy(d)).
    op_reach_node(op, n, 0) <- op_reach_seed(op, n).
    op_reach_node(op, to, d0 + 1) <- op_reach_node(op, from, d0), flow_edge(from, to), d0 < 64.

Rules (from the sibling plan, verbatim intent):
- Head 2 or 3 cols. The DEPTH col = the one named in `merge(MinBy(dcol))`.
  Remaining non-key cols = optional tag + node.
  - 2-col + key+MinBy -> (node, depth), no tag   [entry_reach_node]
  - 3-col + key+MinBy -> (tag, node, depth)       [op_reach_node]
  - 2-col, no key      -> (tag, node), no depth    [port_reach, today]
- Recursive body: optional `!halt(mid,_)`, optional depth arith
  `head(tag?, from, d0), edge(from,to), d0 < CAP` producing `(…, to, d0+1)`.
  Parse CAP from the `d0 < N` Cmp; verify the head depth term is `d0 + 1`.
- Base rules produce (tag?, node, depth) directly (seed at depth 0, NO edge hop
  for these two — the base frontier is the seed nodes themselves at depth 0).
- KEY SIMPLIFICATION: node cols are `sym` (i64) — write the raw i64 cell, NO
  `_strings` decode, NO render loop. Only a `text` TAG (op) needs its cell. The
  current "head column is sym -> bail" guard must become "sym node col is fine,
  write i64 direct; sym/text tag keeps its cell".
- The `key(...) merge(MinBy(d))` head currently forces `naive_fallback` in
  `rebuild_derived` via the `m.key.is_some()` guard BEFORE the native dispatch.
  MOVE the native attempt ahead of that guard, else depth rules never reach it.
- Feed `multi_source_walk(adj, starts_with_depth, halt=None, cap=Some(64))`.
  BFS layer = min depth = what MinBy computes, for free.

Parity lever stays: `DL_NO_HALT_BFS=1` forces SQL. A recognizer MISS must fall
through to SQL, never silently change rows.

## Part 2 — Fix A: dedup rules by STRUCTURE, ignoring origin
Symptom: on the multi-root daemon the same rule loaded under N roots reads as N
distinct recursive rules (the recognizer's `dedup_by_debug` uses `Rule`'s Debug,
which includes `Rule.origin: Option<PathBuf>`), so the component looks like it
has >1 recursive rule and the recognizer bails to SQL. Native never fires.

Fix: dedup the component's rules by a key that EXCLUDES `origin` (and any other
source-location-only field). Options, pick the smallest correct one:
- a manual `(head, body, ...)` tuple key that omits origin, or
- clone + null out `origin` before the Debug-string dedup, or
- a dedicated `Rule::structural_key(&self) -> String`.
Add a test: two `Rule`s identical but for `origin` dedup to one; the recognizer
fires on a 2-root-loaded `port_reach`.

## Part 3 — Fix B: per-tick shared adjacency cache
Symptom: `entry_reach_node` reaches ~1266 of ~120k nodes. Native loads the full
`flow_edge` adjacency (~166k edges) to walk ~1000 — the load dominates and native
REGRESSES vs the output-sensitive SQL fixpoint (~30ms vs ~3ms). The load is the
same graph for `port_reach`, `entry_reach_node`, `op_reach_node` (all over
`flow_edge`), so pay it ONCE per tick and reuse.

Design (keep RSS controllable — this is the whole point):
- Cache key = (edge_rel_name, c0, c1, sym) — the `load_edges_keyed` args.
- Cache value = the returned `(adj, key2id, id2key, strings?)`. Hold ONE per tick.
- Lifetime = one tick. Invalidate/clear when the tick advances OR when the edge
  rel is dirtied (simplest correct: clear at tick start; a digest check is a
  bonus, not required). Do NOT hold it across ticks — that reintroduces the
  unbounded-RSS trap the whole arc is avoiding.
- Store it on the engine's per-tick scratch (find where tick-scoped state lives;
  do NOT add a process-global static). If no per-tick scratch struct exists,
  thread an `Option<AdjCache>` through the rebuild_derived call path rather than
  a `static`/`thread_local`.
- One graph load, three reach rules reuse it. Measure: entry_reach_node should
  drop from ~2.3-3.8s toward a few hundred ms once the load is amortized.

## Parity + measurement
- e2e: `entry_reach_node` and `op_reach_node` native vs `DL_NO_HALT_BFS=1`,
  ROW-IDENTICAL including the depth column. Add to `tests/it/halt_bfs.rs` (or a
  new `tests/it/depth_walk.rs`, register in `tests/it/main.rs`).
- Confirm `port_reach` still parity-passes (don't regress Part 1's sibling).
- Report before/after ms for entry_reach_node from the daemon perf log if
  reachable hermetically; otherwise report the microbench.

## Laws
- No `provenance`/`substrate`/`load-bearing`/`regime` identifiers (source/base/
  critical/mode).
- Descriptive rust var names, never single-letter.
- N+1: no per-row writes — the streaming 16k-chunk insert from `port_reach` is
  the model; depth rules write i64 sym cells + int depth, still chunked.
- File-size law: `src/engine/derive.rs` is already large — if a clean split
  presents itself keep it, but do NOT do a speculative refactor; land the
  feature. If a function would blow the 500-line hard cap, STOP and note it.
- Hermetic runs: `SPREFA_CONFIG=/nonexistent/x.toml DL_NO_DAEMON=1`, scratch --db.
- `git commit -n`, do NOT push. One commit per part (depth recognizer / fix A /
  fix B / tests) or a small cohesive set.

## Escape hatch
If a part can't be done within the laws, STOP that part and note why in the final
summary. Do not improvise around the parity lever or the RSS discipline.

## Final summary shape
Per-commit shas + what each did; the exact derive.rs symbols changed; test names
+ RED->GREEN evidence; entry_reach_node before/after timing; full-suite pass/fail
counts (max 2 full runs); anything skipped + why. Confirm parity lever intact and
no cross-tick adjacency retention.
