# Architectural measures — round 1+2 review

Date: 2026-07-17. Source data: examples/measures-proto.dl run on two corpora.
Raw reports: session scratchpad r6-measures-round1.md (sprefa) and
r6-measures-round2-smashy.md (smashy).

## Verdict (2026-07-18)

**KEEP ALL measures** — fan_out, fan_in, blast, cycle_member. Rationale: these
are research instruments, not a lint surface pruned for signal-per-row. The
overlap found below (blast's top-10 duplicating fan_in almost 1:1 on both
corpora) is a documented DATA finding about how the two measures relate on
these corpora, not evidence that one is redundant in general — a corpus where
the call graph fans out differently (wide-shallow vs narrow-deep) could
separate them. Keeping both costs nothing beyond reach_from's runtime (764ms
on sprefa's own corpus, see receipt below) and gives a standing baseline to
notice when overlap DOESN'T hold.

Landed: `std/measures.dl` — the canonical, importable home for all four
measures, with RANK-based top-K views (`fan_out_top10`, `fan_in_top10`,
`blast_top10`, `cycle_member_top10`) replacing the round-1 absolute
thresholds per finding 2 below. `examples/measures-proto.dl` stays in place
unchanged as the round-1/2 receipt — it is what produced the raw numbers in
this document (its own scan + the n>40/n>60/n>300/n>5 cutoffs), and nothing
else in the repo references it, but it is the reproducible source of the data
above so it is not deleted.

Out of scope (rider): `dl q <verb>` wiring for fan-in/blast per-function
lookups depends on the turnkey query surface arc
(plans/2026-07-10-turnkey-query-surface.md), which is unbuilt. Follow-up once
that lands: `dl q fan-in <fn>` / `dl q blast <fn>` as noted in "next steps"
below — blast per-function is the pinned-endpoint closure read the engine
already supports today via `std/measures.dl`'s `reach_from`.

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
- Verdict (2026-07-18): KEEP. Ranking looks right on both corpora. `dl q`
  verb wiring is deferred to the turnkey query surface arc (out of scope
  here); `std/measures.dl`'s `fan_out`/`fan_out_top10` is usable today via
  `use "std/measures.dl".` on-demand.

## M1b fan_in (callers of a function; expensive-signature signal)

- sprefa top-10: TypeArena.get (406) / iter (374) / len (296), Response.ok
  (160), Db.conn (121), WatchGate.filter (117), Parser.next (103), push
  (103), walk (82), find (79).
- smashy top-10: v3D (4), the prefabs/_dsl.ts builder set (body/shape/node,
  3 each).
- Verdict (2026-07-18): KEEP. Both lists name the de-facto public interface
  of each codebase; this is the primary "expensive to change" signal.

## M2 blast (transitive dependents via reach_from)

- sprefa top-10 overlaps M1b's almost 1:1 (TypeArena.get 1016, iter 1008,
  len 787, verb_specs 538, find 536, Response.ok 536, ...). On smashy, same
  story at max=5.
- Data observation: at top-10 depth, blast re-ranks fan_in's names with
  bigger numbers on both corpora; it did not surface a target fan_in missed.
  Cost: 11.8x row inflation on the rust corpus (77k rows), trivial runtime.
- Verdict (2026-07-18): KEEP, despite the near-duplicate top-10. The
  overlap is a data finding about these two corpora's call graphs, not a
  structural reason blast is redundant — mid-list divergence (rank 10-50)
  was never examined, and a corpus with a different fan-out shape (wide
  orchestration layer vs deep call chains) could separate the two rankings.
  The lattice idiom (`reach_from`, `key(src,callee) merge(MinBy(d))`) is
  also the base `cycle_pair`/`cycle_member` build on, so keeping blast keeps
  that machinery exercised and documented in one place
  (`std/measures.dl`).

## M3 cycle_member (mutual reach; recursion knots)

- sprefa: 82 cycle_pair rows, 67 members, top-10 is a single file — the
  ts_flow_*/ts_lift_fn visitor cluster in src/graph/typegraph.rs, a true
  mutually-recursive AST walker (the "bless with a waiver" case).
- smashy: hard 0 at any depth in the TS scope.
- Verdict (2026-07-18): KEEP, as an occasional audit. The measure isolates
  real knots with zero noise; a cycle appearing where none existed before is
  a strong signal even though quiet corpora (smashy) produce nothing.

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

## Next steps

- [x] `std/measures.dl` with top-K views (RANK-based, no absolute
  thresholds) — landed 2026-07-18. Exports `fan_out`, `fan_in`,
  `reach_from`, `blast`, `cycle_pair`, `cycle_member`, plus
  `fan_out_top10`/`fan_in_top10`/`blast_top10`/`cycle_member_top10`. Receipt
  below (sprefa's own corpus).
- [ ] Two-corpus smoke in the oracle style (sprefa + smashy) for
  `std/measures.dl` — not run this pass; the receipt below is sprefa-only.
- [ ] (RIDER, out of scope this pass) `dl q fan-in <fn>` / `dl q blast <fn>`
  verbs via the turnkey query surface plan
  (plans/2026-07-10-turnkey-query-surface.md); blast per-function is the
  pinned-endpoint closure read the engine already supports via
  `std/measures.dl`'s `reach_from`. Blocked on that arc, which is unbuilt.
- [ ] A TS extraction-density fix upstream of any TS-corpus conclusions.

## std/measures.dl receipt (2026-07-18)

One-shot, hermetic (`SPREFA_CONFIG` pointed at an empty scratch toml,
`--no-daemon`, temp `--db`), run from the repo root against its own
`src/**/*.rs` corpus (157 files):

```
? fan_out_top10(caller, n).
tick_report                                97
tick_paths_with_policy                     72
run_daemon                                  38
run_lsp                                     39
declare_builtins                            30
parse_file                                  30
refresh_call_rels                           28
all_builtin_decls                           27
dispatch_root                               26
move_one_repo                               25
(10 rows)

? blast_top10(callee, dependents).
TypeArena.get      994
TypeArena.iter      945
TypeArena.len      759
Response.ok        541
verb_specs         540
find               537
Parser.next        418
is_daemon_internal 415
is_under_nested_repo 415
WatchGate.filter    414
(10 rows)
```

fan_in_top10 and cycle_member_top10 also ran clean (11 rows each — RANK ties
at the K=10 boundary; see std/measures.dl's top-K comment). Full tick:
`derived_ms=1306.7 slowest_rel=reach_from slowest_ms=764`, `trigger=full
reason=blank-slate` (first-run; expected on a fresh db). No typecheck or
engine-bail errors.
