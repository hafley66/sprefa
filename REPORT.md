# relhash2 — h_schema and h_rule catalog columns

Commit: `catalog: h_schema and h_rule, the shape and derivation fingerprints`

## Delivered

`catalog_ddl_contract/2` grows `h_schema-text` and `h_rule-text` after `h_id-text`,
11 columns total. Per-row values:

| row kind | h_schema | h_rule |
|---|---|---|
| primitive | `''` | `''` |
| module | `''` | `''` |
| rel | `schema_hash` | `rule_hash` |
| column | `''` | `''` |

- `schema_hash/4`: canonical hash of `schema(Columns, ColumnTypes, KeyOrNone)`,
  taken straight off the `relplan/5` term `catalog_rel_rows` already destructures.
- `rule_hash/3`: `''` for a source rel; otherwise `msort`-ed rule bodies of every
  rule deriving the ref, canonicalized and truncated. Two identical bodies are two
  derivations: `msort` (not `sort`) keeps both, so the count participates.
- `canonical_hash_key/2`: `copy_term` then `numbervars` so variable identity is
  positional and the same shape hashes identically across runs and processes.
- `catalog_row_ddl/4` takes `Rules` (was an unused `Decls`); the single caller
  `lower_program/2` already destructures `prog(Decls, Rules)` and passes `Rules`.
- `rule_head_ref/2` was already exported by `analyze.pl` and already imported via
  `use_module(analyze)`; no import change was needed.
- `analyze.pl` `catalog_mentions_atom/1` arity 11, comment updated.

Files: `v6/prolog/lower.pl`, `v6/prolog/analyze.pl`,
`v6/prolog/compile/test/plunit_tests.pl`, `v6/tsv2/tests/catalogRows.test.ts`.

## Validation (all six)

| rail | required | actual |
|---|---|---|
| plunit | 355 / 355 | **355 / 355, exit 0** |
| conformance | 306 pass / 0 fail, UNCHANGED | **306 pass / 0 fail** |
| TEXT_DOOR | compiled=422 byte_identical=422 failures=0, UNCHANGED | **compiled=422 byte_identical=422 failures=0** |
| prolog-lint | findings=1 baseline=1, UNCHANGED | **findings=1 baseline=1 OK** |
| tsv2 | 150 pass / 0 fail, plus assertion | **150 pass / 0 fail / 2 skipped** |
| sweep | final_wrong=0, UNCHANGED | **final_wrong=0** |

## Stability receipt (section E, cross-process)

Program `stability`: `derived/1` derived by two identical-body rules
`(derived(X) <- src_a(X))` and `(derived(Y) <- src_a(Y))`, plus a catalog reader.
Compiled to `INSERT OR IGNORE INTO "__rel"`; the rel row's `h_rule` extracted:

- fresh process 1: `h_rule = db864b94bba545b7`
- fresh process 2: `h_rule = db864b94bba545b7`
- single-derivation control (one body): `h_rule = 18d0e857c8033dc4`

`db864b94bba545b7` is identical across two independent processes and differs from
the single-body hash, so `canonical_hash_key/2` is both process-stable and keeps
derivation multiplicity (`msort`). This is the same rel hash the plunit test
`catalog_h_rule_stable_and_distinguishes_derivation` pins within one process.

Within one process, `catalog_h_rule_stable_and_distinguishes_derivation` compiles
the same program twice and gets the same `h_rule`, and a body change
(`src_a(X)` -> `src_b(X)`) produces a different `h_rule`.

## DEVIATIONS

- Required rails all hold unchanged (conformance 306/0, TEXT_DOOR 422/422,
  lint 1/1, sweep final_wrong=0). No catalog output leaked into non-catalog
  programs.
- `sweep.sh` exits 1 in its informational stage 4: `manifest-reason-diff.ts`
  rejects a duplicate `enum_decl_variant_rows_round_trip_through_tag_view`
  already present twice in the committed `HEAD manifest.json`
  (`git show HEAD:.../out/manifest.json`). Pre-existing on base, not caused by
  this lane; the required `final_wrong=0` rail is unaffected.
- tsv2 reports 2 skipped (not the brief's 1): the extra skip is environmental
  (fresh `node_modules` after install). Pass = 150, fail = 0, plus the new
  `h_schema` assertion, all satisfied.
