# Continuation brief 2: list(T) value position — the entangled block

Contract:
`/Users/chrishafley/projects/sprefa/plans/2026-08-14-list-value-position.PLAN.md`
(main tree, absolute path). This file adds the previous lane's findings and
AMENDS the slicing: the remaining work lands as ONE commit, because fixture
`15_string_split.pl` exercises producer + consumer + oracle in one file and no
per-piece commit can hold all gates green.

## First action
```
git merge --ff-only 82452c52
```
Failure = STOP AND REPORT.

## State you inherit (commit 82452c52, "wip: slice 2 producer")

- Slice 1 landed earlier (74d05b48): entity mint carries `content TEXT UNIQUE`.
- Producer DONE: `registry.pl:291` = `typed([text,text], list(text))` /
  `split_list_intern`; `lower.pl` has `list_intern_statements` +
  `list_entity_id_lookup` (~:665-700); oracle `body.pl:186` has the
  `typed_scalar_value(split_list_intern, ...)` clause; plunit
  `split_lowers_to_the_interned_list_id` green.
- Gate state at your base: plunit 5 known-red + new test green, conformance
  421/0, sweep RED: `emitted_crash` x7, all
  `no such table: __gen__list_text_df210f232c1299bd` — fixture 15 still
  declares `json_list(text)` so nothing mints the table the emitted SQL now
  reads. This red is EXPECTED at your base and is yours to turn green.

## The one commit you land

1. **Fixture migration**: `fixtures/15_string_split.pl` columns
   `json_list(text)` -> `list(text)`; final states rewritten to decomposed
   entity + member rows.
2. **Consumer**: `decode(Parts, spread(...))` over a `list(T)`-typed source
   joins `__gen__list_<t>...__member` ordered by idx (the `lower.pl:714` throw
   for `list(text)` sources is the site to replace). Add the EXPLAIN-based
   test showing SEARCH on the member index (plan section 3.3).
3. **Oracle minting**: a rule-computed `list(text)` value mints entity+member
   rows and binds the list id. HARD CONSTRAINT the plan under-specified: sweep
   byte-diffs tick logs (`sweep.ts:232`), so the oracle's minted id must EQUAL
   the compiler's autoincrement `__id` — thread a mint state (id counter +
   content->id map, first-appearance order matching the compiler's scan order)
   through `level_closure -> plain_fixpoint -> eval_head` in `body.pl` +
   `level_eval.pl`, and return minted rows so downstream rules read them.
4. **Fixture 19**: `fixtures/19_list_value_position.pl` per plan section 3.4
   (round trip, two producers sharing one interned list_id, element type flows
   through spread, empty separator unchanged).

## Gates for the landing commit (all green, no exceptions)
```
cd v6/prolog/conformance && swipl -g go -t halt go.pl        # 421+N / 0
cd v6/tsv2 && bash scripts/sweep.sh                           # RUN wrong=0 emitted_crash=0
cd v6/prolog && swipl -q -l compile/test/plunit_tests.pl -g run_tests -g halt   # exactly the 5 known-red
bash v6/sprefa-engine-rs/grade.sh                             # report byte-clean delta with fixture names
```

## Ownership
Same as the plan section 5, PLUS `v6/prolog/level_eval.pl` and
`v6/prolog/conformance/engine.pl` if the mint-state thread requires it, PLUS
`fixtures/15_string_split.pl`. Forbidden list unchanged (never
`parse_dl_dcg.pl`, never fixtures/17_ 18_, never engine-rs src, never tsv2
sources). If the mint-state refactor cannot preserve an existing gate, STOP
and write the finding; do not force it.
