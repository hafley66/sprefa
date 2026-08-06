# brief: fuse the 3-way laziness impact duel into one doc pair

You are a mechanical fusion lane. The editorial decisions are ALL made
(an independent audit adjudicated every conflict); your job is assembly.
If reality deviates from this brief, STOP and write what you saw into
FUSE-REPORT.md; do not improvise, do not re-adjudicate.

Bounds: write ONLY /Users/chrishafley/projects/sprefa-impact-lazy/IMPACT.fused.md,
IMPACT.fused.visual.human.unga.md, and FUSE-REPORT.md. Everything else,
including the other two worktrees, is read-only. No git commands that
mutate. No subagents.

## Inputs
- BASE: ./IMPACT.md (the opus leg; this directory)
- UNGA BASE: /Users/chrishafley/projects/sprefa-impact-lazy-fable/IMPACT.visual.human.unga.md
- graft sources: /Users/chrishafley/projects/sprefa-impact-lazy-fable/IMPACT.md
  and /Users/chrishafley/projects/sprefa-impact-lazy-flash/IMPACT.md
- adjudication scratch: ./.audit-scratch/*.pl (cite as receipts where told)

## Assembly
IMPACT.fused.md = BASE restructured with the corrections and grafts
below. Keep BASE's section order and voice. Mark nothing as "grafted";
it reads as one document.

### Corrections to BASE first (receipt fixes, audit-verified)
1. `readBoundary` is at 1_incremental.ts:993, not :1030-1063 (:1034 is
   stageDepartures). Fix the two cites.
2. analyze.pl: `derived_refs/2` is :80, `program_refs/2` is :231; only
   `body_ref_uses/2` is at :104. Fix the triple cite.
3. The fifth query fixture: 4_struct_values.pl:421 (not :757); fixture
   name struct_host_output_schedule_answer_interned is right.
4. Strike "tsv2 imports exactly one thing from the store package":
   runtime/scratchStore.ts:14-15 imports open_db and SqlRunner,
   runtime/types.ts:4 imports five types. Keep the scope conclusion
   (store lowering is off the tsv2 emitted path), drop the quantifier.

### Grafts (each names its source doc section; lift, adapt tense, cite)
1. From fable §4: `2_demand_cone.pl` as a NEW shared module, argued from
   compile.pl:88-98 (1_host_expand.pl already shared compiler/oracle).
   This REPLACES base's analyze.pl+compile.pl placement; keep base's
   0_graph:collect_reachable/6 reuse note inside it.
2. From fable §2: "INGRESS IS NOT EVALUATION" as the section-opening
   framing sentence of §2, before the options.
3. From fable: F-fix-A (conformance expectations as implicit demand
   roots, harness-side, zero fixture edits) added as a FOURTH option in
   base's C1/C2/C3 fork, with base's C2 objection noted against it.
4. From fable §6: the revert-boundary sequencing overlay on the ladder
   (steps 1-3 trivially revertible, 4-5 the semantic commit, 6-8 where
   the machine quiets).
5. From fable §3: LANG.md:15-16 (external/register died; bind is the
   unbundled survivor) as evidence against a new keyword.
6. From flash §1: the final/2 precision correction: expectation filters
   an always-fully-computed union (engine.pl:604-608); the contract's
   "asserts the union of ALL rels" wording was imprecise.
7. From flash §5: "byte-identity is per surviving STATEMENT, not per
   module" as the §5 phrasing.
8. From flash: the rows(rel) pull-API meaning change (3_engine.ts:127-134
   returns live rows for an unqueried rel today; empty under pruning).
9. From flash §7: the UNKNOWN convention: unresolved items say UNKNOWN
   inline rather than hedging silently.
10. REPLACE base §3.3-3.4 entirely with the adjudicated three-spellings
    section, verbatim content:
    - Spelling A (level-plane accumulate, base's program): compiles
      clean; cost = scan_due is a growing maintained view, the pulse is
      its delta stream.
    - Spelling B (edge arms, bare atoms, fable's program): REFUSED,
      clock_path_conflict(pre_commit, gate_fire, 0, 1). Mechanism:
      3_clock_check.pl:129-138 — in a finalize-free edge arm every bare
      atom is a trigger, Grade=1 iff the source rel is edge-headed; the
      bare read of the edge-headed latch IS the +1.
    - Spelling C (edge arms, latch read as latest/1 sample): compiles
      clean.
    - Receipts: ./.audit-scratch/adjudicate.pl and sample.pl re-run the
      three programs through the real checker.
    - TWO DISTINCT HAZARDS, separate fork rows: (a) base's D-fork: the
      set-kind latch silently swallows a second pre-commit, program
      compiles, nothing complains (label vs refusal, unruled); (b)
      fable's C-fork: two offsets into one head from one origin,
      refused today; note the checker already emits
      not_provable(multi_trigger_batch_invariance(...)) on that arm.

### The reset fork (REPLACES every share-no-reset ruling claim)
The user ruled share-with-no-reset ONLY as the worked pre-commit
example's shape, never as a global default. Wherever BASE says "the
user's ruling" about reset behavior, rewrite to: OPEN FORK — reset
behavior for demanded sources in general: never-reset (warm forever) vs
rx-default reset-on-refcount-zero (cold on last reader) vs per-rel
declaration. Unruled; no recommendation.
The share() defect stays, reframed on its own merits: bare share() at
3_engine.ts:112 + upstream finalize at :104-111 flips running=false at
refcount zero, so submit() (:116-124) errors until re-subscribe; masked
today by the permanent subscription at 4_http.ts:164; becomes reachable
with per-query standing streams; the comment at 3_engine.ts:180-193
records a prior measured outage through this state. Ladder step 5 must
NOT prescribe resetOnRefCountZero:false as settled; it presents the
narrow fork "should running depend on ticks$ subscription at all".

### Kill list (must appear NOWHERE in the fused pair)
- the four struck receipts above in their wrong form
- "the canonical ruled composition, written naively, is REFUSED" as a
  general claim (only the bare-atom edge spelling is)
- edge_departure or "sampled current sets" as the +1 mechanism
- "2 term fixtures with query(" (5 fixtures, 2 files; ladder bill uses 5)
- flash's §3.3 dl program in any form (bare 0-arity atoms; the
  interval(1000,...) 1000-second misread; unbounded after_hook
  recursion; undeclared first_time)
- event_source(pre_commit). untyped decl surface
- bare "labs/rel_as_stream" path (it is v6/prolog/labs/rel_as_stream)
- any "share-no-reset is the user's ruling" phrasing

## IMPACT.fused.visual.human.unga.md
= UNGA BASE, plus: base unga's edge-vs-level "not equally lazy" panel,
base unga's silent-gate diagram, base unga's five closing questions, a
plain-words paragraph for the reset open fork, and a plain-words
three-spellings picture (one ascii diagram, three verdict lines). LAW:
plain words, ascii diagrams, ZERO citations, no file paths, no code
blocks beyond single dl question lines. Strip the three file names if
you lift prose that carries them.

## Style laws (both docs)
Banned words: provenance, substrate, load-bearing, regime. Construct
vocabulary rxjs/prolog/SQL only. No em dashes. dl snippets carry their
rx lowering and use descriptive variable names.

## FUSE-REPORT.md
List: each correction applied (old -> new), each graft with its
destination section, each kill-list item with 0 grep hits in the fused
pair (show the grep commands), line counts of both outputs.
