# Architectural measures — round 1+2 review

Date: 2026-07-17. Source data: examples/measures-proto.dl run on two corpora.
Raw reports: session scratchpad r6-measures-round1.md (sprefa) and
r6-measures-round2-smashy.md (smashy). Verdicts are OPEN — each measure ends
with the question the data poses; decisions belong to the user.

## Method

- Measures are plain dl rules over built-in family rels (call_edge only, so
  far). Transitive reach uses the depth-lattice idiom
  (`reach_from(src, callee, d) key(src, callee) merge(MinBy(d))`, cap 32)
  because the engine bans unpinned closure reads.
- Runs are `dl --no-daemon --db <VACUUM-INTO snapshot> <file.dl>` from the
  corpus root (the scan root is the cwd; `--db` carries no root).
- The program needs at least one scan rule scoped like the corpus's own rails;
  as of c3c587c9 a scan-less program warns and skips instead of wiping the
  snapshot's file scope.

## Corpus stats

| | sprefa (rust, src/**/*.rs) | smashy (ts, content/src/**/*.ts) |
|---|---|---|
| files | 153 | 68 |
| call_def | 5,088* | 71 |
| call_edge | 6,564 | 38 |
| edges/def | 2.34 (matched scope) | 0.54 |
| reach_from rows | 77,763 (11.8x edges) | 43 (1.13x) |
| full run wall | 4.73s | 0.32s warm |

*pre-narrowing count from the full snapshot; the measures scope is src/.

## M1a fan_out (calls out of a function; orchestrator signal)

- sprefa top-10: tick_report (91), tick_paths_with_policy (70), run_lsp (44),
  run_daemon (38), dispatch_root (32), parse_file (32), refresh_call_rels
  (31), declare_builtins (29), move_one_repo (27), refresh_type_rels (26).
  These are exactly the known orchestrators (tick.rs and daemon.rs are also
  the two biggest change-cost sites in the friction inventory).
- smashy top-10: tetromino (10), attachedCell (5), falconDiveKnockback (4),
  circleWindow/capsuleWindow (4) — content-definition builders, not engine
  orchestrators; max n is 10 vs sprefa's 91.
- Open verdict: the ranking looks right on both corpora. Question: is a
  fan_out list actionable enough to keep as a standing `dl q` verb, or only
  as an on-demand exploration?

## M1b fan_in (callers of a function; expensive-signature signal)

- sprefa top-10: TypeArena.get (406) / iter (374) / len (296), Response.ok
  (160), Db.conn (121), WatchGate.filter (117), Parser.next (103), push
  (103), walk (82), find (79).
- smashy top-10: v3D (4), the prefabs/_dsl.ts builder set (body/shape/node,
  3 each).
- Open verdict: both lists name the de-facto public interface of each
  codebase. Question: keep as the primary "expensive to change" verb?

## M2 blast (transitive dependents via reach_from)

- sprefa top-10 overlaps M1b's almost 1:1 (TypeArena.get 1016, iter 1008,
  len 787, verb_specs 538, find 536, Response.ok 536, ...). On smashy, same
  story at max=5.
- Data observation: at top-10 depth, blast re-ranks fan_in's names with
  bigger numbers on both corpora; it did not surface a target fan_in missed.
  Cost: 11.8x row inflation on the rust corpus (77k rows), trivial runtime.
- Open verdict: kill as a standing measure (fan_in suffices at this depth),
  keep the lattice idiom documented for seeded/pinned reach questions? Or
  keep because mid-list divergence (rank 10-50) was not examined?

## M3 cycle_member (mutual reach; recursion knots)

- sprefa: 82 cycle_pair rows, 67 members, top-10 is a single file — the
  ts_flow_*/ts_lift_fn visitor cluster in src/graph/typegraph.rs, a true
  mutually-recursive AST walker (the "bless with a waiver" case).
- smashy: hard 0 at any depth in the TS scope.
- Open verdict: the measure isolates real knots with zero noise. Question:
  keep as an occasional audit (a cycle appearing where none existed is a
  strong signal), even though quiet corpora produce nothing?

## Portability findings (round 2's deliverable)

1. The .dl program ported with ZERO structural edits — rels, arities, and
   query grammar were identical across a rust and a TS corpus.
2. Absolute thresholds do not port: sprefa's n>40/n>60/>300/>5 gave 0/0/0/0
   on smashy (max fan_out 10). A ~130x call_edge drop needed a ~10-20x
   threshold rescale. Fix shape for a std/ version: percentile or top-K
   views instead of absolute cutoffs.
3. The scan root is the cwd; `--db` carries no root. A measures run must
   `cd` into the corpus root and scope its scan like the corpus's own rails,
   or the snapshot narrows to the program's scope (618 -> 68 files on
   smashy). The no-scan wipe is guarded (c3c587c9); the PARTIAL-scope
   narrowing is inherent reconcile semantics and stays a documented
   sharp edge.
4. TS call-edge density is 4.3x lower than rust's at matched scope (0.54 vs
   2.34 edges/def) — consistent with the standing ledger note that TS
   dataflow/call extraction is sparse (class-method bodies emit nothing).
   Measures over TS undercount until that gap closes.

## If kept: next steps (not started)

- std/measures.dl with top-K views (no absolute thresholds), two-corpus
  smoke in the oracle style.
- `dl q fan-in <fn>` / `dl q blast <fn>` verbs via the turnkey query surface
  plan (plans/2026-07-10-turnkey-query-surface.md); blast per-function is the
  pinned-endpoint closure read the engine already supports.
- A TS extraction-density fix upstream of any TS-corpus conclusions.
