# LANE catarch REPORT: catalog arc

HEAD verified: `git rev-parse HEAD` printed `e3997cecd88322ae029255c5e3cc8402e433d122`, as the brief required.

## Validation 1: build-order graph

```
$ swipl -g go -t halt ARCH.pl
PASS  sugar_grounds_out
PASS  species_are_four
PASS  graphs_refine_ast
PASS  roadmap_is_total
PASS  construct_status_closed
PASS  construct_tier_known
PASS  covers_endpoints_ground
exit=0
```

## Validation 2: roadmap rows

```
$ swipl -q -l ARCH.pl -g roadmap -g halt | grep catalog
catalog_g1_producer         unbuilt
catalog_g2_oracle_parity    unbuilt
```

## The two task rows

From `v6/prolog/ARCH.pl:900-901`:

```prolog
task(catalog_g1_producer, unbuilt, []). % SCAFFOLDED (decision record rulings.pl:613 catalog_universe): catalog rows describe USER-PROGRAM rel decls, produced from the compiler's relplan/5 decl table, materialized into the COMPILED PROGRAM db through the same door __tick uses; the store spine was REJECTED because the fact plane and a compiled program are separate SQLite dbs with no ATTACH (scratchStore.ts:1-11). Landed as e3997cec: catalog_ddl_contract/2 + two []-returning stubs + the wired call site in lower_program/2 (lower.pl:3503). Shape = ONE table __catalog_rel(rel_id, parent_id, ordinal, local_name, kind, type_id), kind in {primitive, rel, column}; a column is a CHILD row of its rel, so it carries a type + annotation exactly as a rel can. Bill = 3 DDL statements per catalog-using program (CREATE TABLE, CREATE INDEX, one INSERT OR IGNORE carrying every row); across the 212 emitted modules catalog rows run 7/12/225 min/median/max incl the five primitives, and the seed adds 8.4%/14.6%/29.4% to the module's ddl array, which itself runs 714/2578/80198 bytes. Gated on program_uses_catalog/2 mirroring program_uses_tick/2 (analyze.pl:180); all 212 emitted modules stay byte-identical.
task(catalog_g2_oracle_parity, unbuilt, [catalog_g1_producer]). % conformance/ticklog.pl needs the same seed only once a FIXTURE derives from a catalog row; a DDL-time seed emits no delta at any tick, so g1 alone cannot diverge from the oracle. The first fixture whose rule reads a catalog row emits deltas the oracle never produces.
```

## Deviations

None against the brief. Two lane-internal notes:

- An edit-application error momentarily corrupted the neighbouring `ast_query_blob_door` row; it was restored and verified whole. The two new rows sit with the recent rows at `v6/prolog/ARCH.pl:900-901`, after `extract_md_html_query` and before `ast_query_blob_door`.
- The brief said "no dates invented"; the rows carry no dates. Commit `e3997cec` is cited because the brief supplied it.

Deliverables: `v6/prolog/ARCH.pl` (two rows) and `plans/2026-08-05-catalog-g1.md` (plan doc). Work left uncommitted.
