# Plan: SCIP oracle — daemon bypass + DAG layering (A), fn-level ingestion (B)

Two tracks off the 2026-06-26 self-xray work. Both serve "use sprefa's own
tools, validated against the RA oracle, to find refactor init-places."

## Track A — daemon bypass + clean DAG layering (no build surgery; analysis)

### Problem
The daemon intermittently intercepts one-shot `dl <file>` runs even with an
isolated `--db`, serving its loaded program + cached DB. This *manufactured a
phantom cycle* `{daemon,engine,lib,lsp,tray}` in refactor-clusters.dl that does
not exist on a fresh DB (`multi_n=0`). One-shot oracle runs are untrustworthy
until this is fixed.

### A1. `--no-daemon` flag (XS)
- Signature: `main.rs` adds `#[arg(long = "no-daemon")] no_daemon: bool`; the
  runner's daemon-attach path (`[daemon] attach failed, falling back` branch)
  is skipped when set → forced in-process.
- Pseudo: `if !no_daemon { try attach } else { in_proc() }`.
- Alt: env `SPREFA_NO_DAEMON=1`. Flag is cleaner for `just` recipes.
- Update `just oracle` / `just profile` to pass `--no-daemon`.

### A2. DAG layering of `scip_edge` (the refactor module tiers)
The oracle file-graph is a clean DAG (0 cycles). A longest-path topological
layering assigns each file a tier = proposed module level.
- Signature (dl): `rel layer(file: file, tier: int)`.
- Pseudo (stratified fixpoint): `layer(f, 0) <- ra_edge(f, _), !ra_edge(_, f).`
  (sources = tier 0). `layer(f, t) <- ra_edge(p, f), layer(p, tp), t = tp + 1.`
  Needs aggregate-max-over-predecessors (Track A gap: no max agg over a join
  yet — use `count` proxy or add a `max` agg, christmas #12 window-agg family).
- Storage: derived rel, recomputed per tick. Read layering as the proposed
  hierarchy: tier 0 = leaves (ast.rs), top tier = entry points (main.rs/lib.rs).
- Validate: ast.rs lands tier 0 (fan-in 12 root); engine.rs mid; daemon/lib top.

## Track B — `scip_import` fn-level extension (the 100%-recall fn graph)

### Problem
`scip_edge` is file→file only (`scip_import.rs:50` emits `(relative_path,
def_path)`). Refactor moves happen at function granularity. Need fn→fn edges
from the SCIP occurrence ranges.

### B1. Extend `scip_import::rows` (the core change)
- Type: add to `ScipRows` a `fn_edges: Vec<(String, String)>` (caller_fn moniker → referenced symbol moniker).
- Storage/lifetime (pass 1, build per-file fn-def interval index):
  - `HashMap<relative_path, Vec<(Range, symbol)>>` — def occurrences whose
    `syntax_kind` is function/method (or symbol shape `…::fn()` / `…method()`).
    `occ.range` (scip::types::Range: start/end Position with line/col).
- Sequence (pass 2, resolve enclosing fn for each ref):
  - For each non-def occurrence in file F at range r: binary-search F's fn-def
    intervals for the one containing r → `caller_fn`. Referenced sym =
    `occ.symbol` (already resolved by RA). Emit `fn_edges.push((caller_fn, occ.symbol))`.
- Uniqueness: `HashSet<(String,String)>` dedup; sort.
- Wire: `refresh_rel("scip_fn_edge", &["caller","callee"], &rows.fn_edges)` in
  the SCIP refresh block (`engine.rs:3991`).

### B2. Moniker ↔ sprefa-sym mapping (for joining with `call_edge`)
- SCIP moniker: `rust-analyzer cargo sprefa-v5 0.1.0 engine/Engine().tick()`.
- sprefa sym: `sprefa::v5/src/engine.rs::method::Engine.tick`.
- Add `scip_moniker_to_sym()` (parse moniker segments → path + kind + name) OR
  compare purely in moniker space (simpler; skip the join, analyze scip_fn_edge
  standalone). Recommend moniker-space first; map only for cross-checks.

### B3. fn-level refactor analysis on the oracle
- `scc(scip_fn_edge)` → true fn-level mutual-recursion clusters (the move-blockers).
- pin Engine.tick moniker → fn-level `fan_out` via scip_fn_edge (the 100%-recall
  version of the 57-callee heuristic count); compare to diet `call_edge` count.
- fn-level `breaks_if_moved` via `closure(scip_fn_edge_rev)`.

## Sequencing
A1 first (unblocks trustworthy one-shot runs — every later measurement depends
on it). Then B1 (the high-value code change; fn graph unlocks the real refactor
analysis). A2 and B3 ride on top. B2 only if cross-tier comparison wanted.

## Risks
- A1: the daemon-attach logic may live in a crate boundary; confirm flag threads
  through cleanly.
- B1: occurrence ranges are 1-based lines in SCIP; sprefa's are 0-based in some
  rels — verify before interval match. Macro-generated fn defs may have no
  usable range (skip them).
- The `max` agg gap (A2) — confirm whether it exists before relying on layering.
