# flow parity upgrade brief (codex terra): value-plane flow port over the new df records

The portable port landed (v6/dl/fixtures/flagship-flow.dl6, flagship-flow.sh,
flagship-flow-classify.py; merge d322c93f) with every gap named. The extractor
lane then closed those gaps at the record level (merge fb7c362a):
`record=param family=df` (span, pos; receiver -1), `record=arg family=df`
(call span, pos, arg span), Kotlin `--resolve` dispatch, flat
`owner_start`/`owner_end` on `sig`, `caller_site_start/end` on
`resolved_edge`. Your lane rewrites the v6 leg from callable-plane
approximation toward the real value-plane union and shrinks the classifier's
expression-gap bucket. Scout map: plans/2026-07-29-flow-interproc-scout.md.

## Deliverables
1. **df ingestion**: extraction hosts feeding df node/edge/param/arg records
   into v6 rels (the extraction-fork precedent: sh hosts over the extract
   CLI, declared outputs = named projection over flat JSONL fields; see how
   flagship-callgraph.dl6 and flagship-flow.dl6 spell their hosts today).
2. **flow_edge as the real union**: df direct edges ∪ positional arg->param
   hop (df_arg slot = df_param slot, receiver -1, pinned per call site via
   caller_site span) ∪ ret->call_res hop, following std/flow.dl's rules
   (read it; the scout doc maps each). Where a hop is inexpressible in the
   current compiled subset, NAMED STOP with the refusal text, never an
   approximation presented as the hop.
3. **flow_param_type live**: join resolved callees to sig slots via the flat
   owner fields. **flow_node_type**: df param nodes + df_param pos + sig.
4. **Rig upgrade**: flagship-flow.sh runs the same four dumps; the classifier
   keeps its three buckets but now must PROVE match rows exist where inputs
   align (a nonzero match column on flow_edge at minimum — the all-gap table
   was honest for disjoint planes and is a failure now). Print the before/
   after counts table in the run output.
5. At least 2 promoted oracle-graded conformance fixtures for the new joins
   (arg->param hop shape, sig-owner join shape).

## Laws
- Files: v6/dl/fixtures/flagship-flow.dl6, v6/tsv2/scripts/flagship-flow*.{sh,py},
  NEW conformance fixture files + their generated out/ artifacts, and nothing
  else. v6/prolog/compile/**, v6/prolog/{analyze,lower,emit_ts} etc. are OWNED
  BY ANOTHER RUNNING LANE — a needed compiler change is a NAMED STOP.
- Corpus stays the pinned rust corpus the rig already builds; do not grow it.
- Hermetic v5 leg unchanged. No new deps. Descriptive variable names;
  vocabulary law (rxjs/prolog/SQL words only).
- Smallest correct: if the per-site pin needs identity decisions, prefer the
  spelling that grades, state the alternative in a comment.

## Validation (report exact counts)
- swipl v6/prolog/conformance/go.pl — 0 findings (156 + yours).
- Sweep BOTH modes — identical growth only, zero wrong, zero movement in
  pre-existing buckets.
- bash v6/tsv2/scripts/flagship-flow.sh — exit 0, 0 unclassified, and the
  new match-count assertions green.
- TEXT_DOOR receipt still 0 failures.

## Final summary shape
Base sha; per-deliverable outcome; the four-query before/after count table
(v5 / v6 / match / gap per rel); named stops with refusal texts; fixture list.
