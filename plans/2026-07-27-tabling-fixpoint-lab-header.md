# LAB HEADER: tabling_fixpoint — can SWI tabling replace the hand-rolled fixpoint?

Planner-seeded contract per the lab protocol (CLAUDE.md 2026-07-27). The lab runs
in a worktree, implements `v6/prolog/labs/tabling_fixpoint.pl` + `.md` there, and
dies on landing.

## The question

plans/research-swi/01-core.md verified (receipts, local swipl 10.0.2) that
monotonic tabling grows tables incrementally on assert and incremental tabling
propagates retraction correctly. The hand-rolled machinery it targets:
`plain_fixpoint/5` and `agg_loop/6` in v6/prolog/conformance/level_eval.pl, and
the departure tracking around `body_departed_ref/2` in engine.pl.

Verdict wanted, one of exactly two:
- **SHRINKS**: a tabled evaluator passes the ENTIRE fixture corpus with
  byte-identical per-tick delta lists, in fewer lines, at comparable or better
  speed. Report the LOC delta and which predicates die.
- **SHIFTS SEMANTICS**: some fixture's delta list differs. Then the hand-rolled
  loop encodes semantics tabling does not, and the lab's product is the precise
  list of WHICH fixtures diverge and WHY (that list is the spec of what our
  fixpoint does beyond naive least-fixpoint).

## Method (read-only against conformance/)

1. Read plans/research-swi/01-core.md first; reuse its verified incantations.
2. Build a PARALLEL evaluator in your lab file (a tabled variant of the level
   evaluation), consulting v6/prolog/conformance/{engine,body,level_eval}.pl
   read-only. You do not edit conformance/ — your harness runs every fixture
   in v6/prolog/conformance/fixtures/*.pl through BOTH evaluators and diffs
   (a) per-tick delta lists, (b) final rowsets, (c) tick counts.
3. Grade one PASS line per fixture per comparison; any diff prints both delta
   lists in full.
4. Add a retraction-heavy stress you write yourself (keyed replace churn +
   pure-retraction ticks + departure rules) since retraction propagation is
   incremental tabling's known hard case, and our engine had a
   pure-retraction-tick defect historically (fixed via reverse_preserving
   deletion; see chat_log 20260727.3 note).
5. Microbench: `time/1` both evaluators over the 3 heaviest fixtures, report
   inference counts, not wall-clock feelings.

## Hard constraints

- Stratified negation must keep its stratification (the engine moved to
  stratified eval after a joint-fixpoint unsoundness; tabling's WFS/answer
  semantics must not smuggle a different negation semantics past the fixtures —
  if tabling forces well-founded semantics somewhere, that is a SHIFTS finding,
  not a workaround target).
- findall never builds trigger lists (maplist); descriptive variable names; no
  single letters; banned words: provenance, substrate, load-bearing, regime; no
  em dashes.
- `swipl -q -l v6/prolog/labs/tabling_fixpoint.pl -g go -g halt` exits 0, only
  PASS lines. The .md carries the verdict, the LOC table, numbered ambiguities,
  and (if SHRINKS) the exact replacement plan for level_eval.pl as a diff
  sketch, not applied.

## File ownership

You own v6/prolog/labs/tabling_fixpoint.pl and .md in YOUR WORKTREE only
(create labs/ there; it is deleted in main on purpose). conformance/ and
plans/ are read-only to you. The coordinator distills and deletes on landing.
