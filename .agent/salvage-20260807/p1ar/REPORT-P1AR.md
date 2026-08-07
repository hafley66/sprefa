# P1-A-R report (condensed; delivered in-band 2026-08-07, harness blocked the file write)

Review commit 192ab517 on lane/p1a-ir-emit, merged to main via PR #11.
The amended §2.4 in plans/2026-08-07-plan-ir-offload-contract.md is the pinned
record; this file keeps the rulings and process findings.

## Verdict
- IR mirrors the SQL builders clause-for-clause (same compile_positive_uses/7,
  same bound); refusal path safe by construction: outside-grammar -> IR absent,
  never wrong.
- 9 deviations (not 8): all APPROVE; D8 approve-with-note.
- Both gaps CLOSED: arith/4 carries result type from arithmetic_result_type/4
  (int division vs CAST REAL now distinct terms); fixpointir/5 grows a storage
  table: relstorage(ref(Name,Arity), [colclass(Column, Type, StorageClass,
  Collation, Encoding)]), Encoding = direct | dict(TargetRel) — the slot
  task #4 reads/writes. Two new plunit tests pin both (368 total).
- Gates green, all under 10s: sweep 3.7s 210/211 wrong=0; byte-identity
  measured with fixpointIr key stripped: 0/212 modules moved; plunit 368;
  tsgo 0; ARCH pass.

## Rulings one-line each
- D1 Hop = LIST of arms: dred_hop_sql builds one arm per recursive rule; the
  draft's singular field drops arms on multi-rule heads.
- D2 stop(SeedProbe, HopProbe): three of four walks have seed probe != hop
  probe in level_dred_plan/4.
- D3 rel_or_retracted(Ref) source: retract seeds need rel UNION delta-minus or
  an executor under-deletes on same-tick double retraction.
- D4 wave(frontier): ping/pong is executor double-buffering, a walk property
  slot kept for phase-2 two-wavefront reads.
- D5 probe target ref_count: the expand walk dedups against __support_next,
  head|cone cannot spell it.
- D6 liveness(present|absent): names the probe instead of re-deriving from Sign.
- D7 all four walks always present, emit: null on dred/revive: only assert and
  expand carry _sequence obligations.
- D8 whole IR gated on DredPlan present (approve WITH NOTE): one fence over
  four walks withholds IR from non-monotone heads whose expand (from-scratch)
  path is expressible; recorded in §2.4 as a phase-2 decision (own fence for
  the expand walk).
- D9 runtime types.ts gets fixpointIr?: unknown only: IFixpointIr is P1-B's
  file by the ownership map.

## Process findings (coordinator debt)
1. P1-A shipped NO report: the "8 documented deviations" existed only in the
   coordinator's session summary; reviewer re-derived everything from the diff.
   Rail idea: a lane without REPORT at its stated path fails its gate.
2. Root-level REPORT.md name collision: repo-root REPORT.md is Lane C's,
   restored by 93e1df7f after 63294ee9 deleted it. Reports need lane-scoped
   names/paths.
3. INDEX.md regen in fresh worktrees drops rows pointing at gitignored bench
   artifacts; regenerates when they exist.

## Downstream contracts
- P1-B replaces unknown with IFixpointIr typed against amended §2.4.
- Task #4 interning writes storage[].columns[].encoding; dict(rel) is already
  what a ref(_) column produces today.
