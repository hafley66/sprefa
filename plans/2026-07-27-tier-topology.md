# v6 language tiers: what to build orthogonally, in topological order

Source: 2026-07-27 corpus survey of all 163 v5 .dl programs (14,082 lines,
1,451 rel decls) + the boiling surface (v6/prolog/labs/LANG.md). Status:
draft; five labs in flight will amend (shell_stream, merge_family,
astgrep_patterns, check_eventing, AUDIT).

## The corpus verdict that orders everything

| feature | files (of 163) | tier |
|---|---|---|
| scan (file enumeration -> rels) | 135 | T1 |
| ? queries (snapshot asks) | 128 | T0 |
| negation ! | 112 | T0 |
| count/max/min/sum aggregation | 63/11/15/5 | T0 |
| severity diagnostics (diag rows) | 54 | T3 |
| closure / scc / node2vec | 37 / 13 / 7 | T2 |
| ast / comment / jsonp / json / sg extract | 35/33/14/10/1 | T1 |
| @async effects / sh | 12 / 7 | T5 |
| @next carries (-> keys/edges in v6) | 6 | T4 |
| clock / every | 5 / 2 | T4 |

~90% of real usage is the TIMELESS static-analysis fragment. The temporal
machinery that dominated this week's design discussion serves the daemon
minority. Build order must deliver the timeless fragment first and bolt time
on without disturbing it.

## Tiers (each orthogonal; arrows = depends-on)

T0 RELATIONAL CORE (timeless)
  enum/struct + typed rel cols + Key(Type) as schema; level rules `<-`;
  stratified negation; aggregation with grouping; facts; snapshot asks.
  Lowering EXISTS: js engine v1 lowerSql (strata, semi-naive, agg) + emit_ts.
  Checkers: HM/enum typing, exhaustiveness, stratification.
  <- nothing

T1 EXTRACTION + BIND (world-fed corpus)
  bind mechanism (link-time protocols); scan as a bound enumeration rel;
  quoted extraction DSLs (regex/json/comment now; sg/ast via grammar import);
  grammar-import (node-types.json -> con facts, typed CST).
  Checkers: quoted-DSL parse/check obligations, pattern-vs-grammar.
  <- T0

T2 GRAPH OPERATORS (on-disk algorithms)
  closure/scc (node2vec later) as operators over edge rels; recursion is
  already T0; these are the disk-backed algorithm library (the reason
  datalog is here). RAM law: operators stream from sqlite, never resident.
  <- T0

T3 DIAGNOSTICS PRODUCT (convention, not syntax)
  diag named-column shape (path/line/severity/code/msg/hint), gates
  (error = exit 2), ratchets (keyed max-allowed + level violation rule).
  Mostly library + CLI; the 54-file payoff.
  <- T0 (+T1 in practice)

T4 EDGE TIME (first temporal tier)
  `<+` edge rules; Key(Type) runtime semantics (replace, -old/+new);
  tick transaction; count-IVM (port task exists); pre; retention bounds.
  Checkers: jointly-semidet-per-key-per-tick, causality, retention.
  <- T0 (engine: count-IVM port)

T5 EFFECTS
  adorned world rels (signature arrow), envelope enums, demand rows +
  content addressing + edge-salt, shell bind, STREAMING effects (lab in
  flight decides surface), checkout-style demand sinks.
  <- T1 (bind), T4 (edges/ticks)

T6 ASKS + MODES
  tail asks; (cardinality, lifetime) mode analysis; dominance/scopes
  (switch_map); LSP/hook subscription loop (lab in flight).
  <- T4, T5

T7 LAZINESS + SUB GRAPH
  magic demand rows; sub paths on disk; teardown = range-DELETE; node
  sharing (hash-consing); SWR caching as a library pattern.
  <- T5, T6 (sub_graph_disk task)

T8 STORAGE LOWERING (user-parked, orthogonal)
  1-1 type->table; interning (term -> hash -> surrogate int); dense rowids
  for graph algorithms; auto junction tables; retention pruning as the RAM
  budget. Feeds T2 at scale.
  <- T0 (parked note in plans/2026-07-27-surface-boil.md)

T9 OPTIMIZER
  purity split, island partition, rw sets, thread schedule, pushdown, cost
  model (existing ARCH task rows).
  <- most of the above

## Orthogonality claims to hold the line on

1. T0 programs never mention time; adding T4 must not change any T0
   program's meaning (levels stay levels).
2. T1 extraction is bind-swappable: same program, canned rows in tests.
3. T2 operators are rel-in/rel-out; no syntax beyond an operator position.
4. Mode/lifetime (T6) is analysis only: no program text changes, only
   rejections and CLI warnings.
5. T8 changes storage, never semantics.

## Implementation reality (what exists today)

- T0: js engine v1 84/84 + emit_ts.pl green. Closest to done.
- T1: v5 rust extraction is the solved layer (per ARCH tech roles: rust =
  extraction home); grammar-import is the new part (lab in flight).
- T4: count-IVM measured in rust store (beat DRed 4-5x); port task open.
- T2: v5 closure/scc exist SQL-side; carry format decision to T8.
- T5-T7: labs + design only.

Shortest path to a running v6 ghcacher: T0 (have) + minimal T1 bind + T4
edges/keys + T5 shell effect. Shortest path to v6 replacing a v5 lint rail:
T0 + T1 + T3 only, no temporal tier at all.
