# Lane: type-info IR breakdown (planning only, NO production code)

Worktree /Users/chrishafley/projects/sprefa-plan-typeir, branch
plan/type-ir-breakdown, base 2eceb836. FIRST action: `git merge --ff-only
2eceb836` — failure = STOP, write PLAN2.md saying so. Deliverable: PLAN2.md at
the worktree root. Do NOT commit, do NOT write outside this worktree, do not
spawn anything. If reality contradicts this brief, STOP that item and record
the contradiction; never improvise around it.

## The ruled design you are breaking down (do not re-litigate)

1. Type universe = VALUES (structural shapes, jsonschema-native) + ARROWS
   (Input->Output; a host signature requestCols->responseCols; openapi is the
   arrow printer). Nothing else.
2. Shapes live as prolog fact terms: table/2 + column facts (~60 LOC), one
   emitter per language dialect. Generics = parameterized terms (list(T)):
   monomorphize by unification for sql/jsonschema, print natively for ts/rust.
3. NEW ruling (2026-08-03): identity layer = SCIP symbol strings. scip.proto is
   at ./scip.proto (fetched today, 962 lines): SCIP carries NO structural type
   payload (Signature.text is display text, :236-249; Relationship edges
   :465-502; Kind labels :412-420), so SCIP symbols are the ID COLUMN of the
   facts, never the shape. Facts keyed by SCIP symbols join against the
   existing v5 scip index for use-site queries.
4. NEW ruling (2026-08-03, owner verbatim): "a type ir needs most common
   subset of each type system with way to indicate lang specific features down
   the road as foresight." Design consequence, binding: the CORE fact
   vocabulary is the INTERSECTION — only shapes every target dialect prints
   natively (scalars, rows/records, optionality, list(T)). Language-specific
   features are TAGGED EXTENSION facts (a namespaced term like
   lang_ext(ScipSymbol, Lang, FeatureTerm)) that core printers IGNORE by
   contract and only the owning dialect consumes; an unknown extension must
   never break another printer (prove with a test in the ladder). Precedents
   to cite: jsonschema x-* extensions, typespec decorators. Your fact schema
   (deliverable 3) must show the extension seam explicitly, and the ladder
   must include the ignore-unknown-extensions test.

## Grounding (read these, cite file:line in the plan)

- ./prior-plans/PLAN.md + RESEARCH.md + plan-notes/ — the two convergent
  schemagen MVP plans from 2026-08-02 (facts -> emitter -> spine.ts marker
  section -> staleness test, ~190 LOC, MVP gate at step 4, DDL parity step 5).
- v6/sprefa-store/js/src/engine/spine.ts:63-102 (StringsRow etc, the emit
  target marker zone) and v6/sprefa-store/js/src/engine/types.ts:80-95
  (NodeRow/EdgeRow — NOT in spine.ts; a prior lane verified this correction).
- v6/sprefa-store/src/spine.rs:314-417 (sea-orm tables; DDL lives in STORE).
- Emitter precedents: v6/prolog/compile/2_emit_cli_inventory.pl,
  v6/prolog/emit_openapi.pl, 1_emit_registry_docs.pl (marker-section
  replace_generated_section move).
- The alloy lab (decl/ref/check/refuse-before-render/derived imports) is
  RECEIPT-VERIFIED; its files sit in the sprefa-lab-alloy worktree at
  /Users/chrishafley/projects/sprefa-lab-alloy/v6/prolog/labs/alloy_semantics/
  (read-only). Its check/render split is the pattern the emitter follows.
- v5 SCIP surface: plans/2026-07-11-scip-atlas.md and grep the v5 src/ for
  scip to locate the index/rels actually available (VERIFY, do not assume).

## What PLAN2.md must contain

1. Step ladder from zero to: (a) MVP (facts + ts emitter + marker section +
   staleness test), (b) +SCIP id column with the exact symbol-string format
   chosen per the grammar in scip.proto (justify each descriptor part),
   (c) +rust emitter parity, (d) +arrows (one host signature rendered to
   openapi via the existing emit_openapi.pl seam), (e) +generics
   (list(T) monomorphize for sql/jsonschema). Each step: files touched, LOC
   estimate, the exact gate command, and what earlier step it can break.
2. Falsification probes you RAN (read-only) with receipts: the spine.ts marker
   zone line numbers today, the emit precedent predicate names/arity today,
   what the v5 scip index actually contains. Every grounding claim above that
   you could not verify gets flagged UNVERIFIED, not repeated.
3. The fact schema, final: table/2, column/N (state N and every column,
   reconciling the kimi column/8 vs flash leaner-columns fork from
   prior-plans — pick one, one sentence why), plus the scip_symbol column.
4. ARCH-style task rows (task/5 shape used in v6/prolog/ARCH.pl — read 3 rows
   there for the format) proposing this ladder, statuses all unbuilt.
5. Open questions that block nothing vs block a step, separated.

Style: no em dashes; never provenance/substrate/load-bearing/regime;
descriptive names; terse tables over prose.
