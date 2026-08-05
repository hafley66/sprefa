# VERDICT — step g catalog universe: A (store spine) vs B (parser rel-decls)

## TOC
- Scope and the two options
- Option A: files, LOC, gate, perf
- Option B: files, LOC, gate, perf
- Less code / cleaner
- A-vs-B table
- Verdict

## Scope

Step g materializes `__catalog_rel` / `__catalog_instance` into the store.
Question: what is the catalog universe, i.e. what produces the rows.

- Option A = store spine: the 9 tables / 37 columns already transcribed by the
  step-a facts (`table/2`, `column/6`). Facts are the only producer.
- Option B = parser rel-decls: the compiler's declaration table (registry.pl
  `surface/5` construct inventory, parse_dl.pl's `prog(Decls, Rules)` rel
  decls) is the producer; the catalog covers every declared rel, not just the
  spine.

This picks the step-g universe only. The module-catalog ruling's "generalize
the emitter into `__catalog_rel` rows" is a later step, distinct here.

## Option A — store spine

Files touched:

| file | change | LOC |
|---|---|---|
| `v6/prolog/compile/3a_spine_schema_facts.pl` | add derived views `catalog_rel/4` + `catalog_instance/3` over `table/2`+`column/6` | ~25 |
| `v6/prolog/compile/5_emit_catalog_rows.pl` | new emitter, checked-in whole-file seed | ~45 |
| `v6/sprefa-store/src/spine.rs` | add `__catalog_rel`/`__catalog_instance` to defs vec (:408-417) | ~10 |
| `v6/sprefa-store/js/src/engine/spine.ts` | add two names to `table_names()` (:103) | ~2 |
| `v6/tsv2/tests/spineSchema.test.ts` | extended asserted-equal gate (pattern `bopCommandInventory.test.ts:58-59`) | ~15 |

Total: ~90 (matches PLAN.md ladder row g).

Gate: `just tsv2-test`, two asserts — (1) generated seed == 9 rel + 37 column
transcription of `salvage:SEED-INVENTORY.md:3`; (2) forall-check every
`column/6` has exactly one `kind=column` row and every `table/2` one `kind=rel`
row.

Perf note: 46 `__catalog_rel` rows (9 rel + 37 column) and 9
`__catalog_instance` rows (one per non-parameterized table). A 46-row fixed
table; the child-of self-join is a merge on the `parent_id` int index, single
scan, no growth with repo count.

## Option B — parser rel-decls

Files touched:

| file | change | LOC |
|---|---|---|
| `v6/prolog/compile/registry.pl` | extend surface to expose the rel-decl table as a catalog producer (surface/5 currently 45 fact rows, 57 total) | ~40 |
| `v6/prolog/compile/5_emit_catalog_rel_decls.pl` | new emitter walking the compiler decl table, arbitrary rel/N + columns + kinds + generic args | ~90 |
| `v6/prolog/compile/parse_dl.pl` | surface `prog(Decls, Rules)` rel-decl walk for the producer bridge | ~40 |
| `v6/sprefa-store/src/spine.rs` | `__catalog_rel`/`__catalog_instance` in defs vec | ~10 |
| `v6/sprefa-store/js/src/engine/spine.ts` | `table_names()` | ~2 |
| `v6/tsv2/tests/*.test.ts` | asserted-equal gate over the full declared surface + refusal fixtures (param rels, no-cols rels) | ~35 |

Total: ~215.

Gate: emitted seed equals the compiler's own printed decl inventory (registry
surface count + every rel-decl), forall-check every declared rel has one
`kind=rel` row and every declared column one `kind=column` row; plus refusal
suite for parameterized / zero-column rels. Runs from the compiler step plus
`just tsv2-test`.

Perf note: universe scales with the language surface, not the store. Corpus
dl6 fixtures already carry 2-26 rels per file (comment-prod.dl6: 26); at
500-repo scale the catalog grows with declared rels, and the producer must run
per programmed surface rather than once against a fixed spine. Query cost is
still an int-index join, but rows grow and the emitter has to be re-run when
any program declares a new rel.

## Less code / cleaner

Option A is less code and cleaner. It has one fixed producer (the step-a
facts), a bounded universe (the 9 tables / 37 columns the store already
materializes), a single asserted-equal transcription with no second source of
truth, and it matches both PLAN.md section 7 and the ruling's step-g scope
("rides on the type-IR MVP"). Option B drags the full 57-construct registry
surface and the parser's rel-decl walk into the catalog producer, creating two
sources of truth (facts and the compiler's runtime decl table) and a second
producer that must handle arbitrary rel/N shapes, generated instances, and
refusal cases before step g can land. Option B is the ruling's later
"generalize" step, not step g; forcing it in now front-loads the parameterized
and no-column cases the plan explicitly defers.

## A-vs-B

| axis | A (store spine) | B (parser rel-decls) |
|---|---|---|
| LOC | ~90 | ~215 |
| files touched | 5 | 6 |
| beauty | one fixed producer, no second source of truth | two producers, parser surface coupled in |
| universe | fixed 9 tables / 37 cols | scales with declared rels across all programs |
| rows materialized | 46 catalog_rel + 9 catalog_instance | grows with surface; per-program re-run |
| query cost | int-index same-size merge | int-index join but larger + per-surface regen |
| gate | asserted-equal seed + forall; `just tsv2-test` | asserted-equal decl inventory + refusal suite; compiler step + `just tsv2-test` |

## Verdict

Option A for step g. Less code, a single producer and a single source of
truth, and it lands within the ruling's MVP-scoped step. Option B is the
ruling's post-g generalization, additive on top of A later.
