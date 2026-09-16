# LANE relhash — h4: module identity in `__rel`

Worktree `/Users/chrishafley/projects/sprefa-lanes/relhash`, branch `lane/relhash`,
base `86f20179`.

**If reality deviates from this brief, STOP and write it into REPORT.md. Do not
improvise, do not widen scope.**

## 0. First actions

1. `git rev-parse --short HEAD` must print `86f20179`. Else STOP.
2. `git rev-parse --abbrev-ref HEAD` must print `lane/relhash`. Else STOP.
3. `grep -c "arity-int" v6/prolog/lower.pl` must print `1`. Else STOP: the
   catalog contract has moved and the anchors below are stale.

`v6/tsv2` uses **pnpm**. Never npm, never yarn.

## 1. Files you own

| file | why |
|---|---|
| `v6/prolog/lower.pl` | the contract, the row builders, the hash |
| `v6/prolog/compile.pl` | threads the module name to lowering |
| `v6/prolog/analyze.pl` | the arity gate on the catalog rel |
| `v6/prolog/compile/test/plunit_tests.pl` | exact DDL strings and seed rows |
| `v6/tsv2/tests/catalogRows.test.ts` | row assertions |

Nothing else. **Do not touch** `v6/prolog/ARCH.pl`, any `conformance/fixtures/*.pl`,
any `compile/dl_view/*.dl6`, or anything under `out/` or `gen_emitted/`.

Receipt for that boundary, already verified: no conformance fixture and no
`.dl6` view names `__rel`, and `conformance/engine.pl` models no catalog rows.
So conformance, sweep and TEXT_DOOR verdicts must not move at all.

## 2. What lands

### A. `module_hash/2` in `lower.pl`

```prolog
%! module_hash(+ModuleName, -HashText) is det.
%   SHA-256 of the module name, truncated to 16 hex characters (64 bits).
%   One predicate so the function can be replaced in one clause later.
module_hash(ModuleName, HashText) :-
    crypto_data_hash(ModuleName, FullHash, [algorithm(sha256)]),
    sub_atom(FullHash, 0, 16, _, HashText).
```

`:- use_module(library(crypto))` at the top of the file, in the existing
use_module block. Verified present on this machine: `crypto_data_hash/3` loads.

### B. two more catalog columns

`catalog_ddl_contract/2` gains `module_id-int` and `h_id-text`, IN THAT ORDER,
AFTER `arity-int`. Nine columns total:

```
rel_id, parent_id, ordinal, local_name, kind, type_id, arity, module_id, h_id
```

Update, in lockstep:
- the INSERT column list in `catalog_row_ddl/3`
- `catalog_row_part/3`: `row/7` becomes `row/9`, and the format template gains
  `,~d,~w` where the `~w` takes an already-quoted SQL text literal for `h_id`
  (use the existing `sql_text_literal/2`, exactly as `local_name` does)
- `analyze.pl` `catalog_mentions_atom/1`: `functor(Atom, '__rel', 7)` becomes
  `functor(Atom, '__rel', 9)`, and its comment's "Arity 7" becomes "Arity 9"

### C. one `kind='module'` row, and every row points at it

Id order changes. The five primitives keep 1..5. **Id 6 is now the module row.**
Rels and their columns start at 7.

| row | rel_id | parent_id | ordinal | local_name | kind | type_id | arity | module_id | h_id |
|---|---|---|---|---|---|---|---|---|---|
| primitive | 1..5 | 0 | 0 | text/int/float/bool/json | `primitive` | 0 | 0 | 0 | `''` |
| **module** | 6 | 0 | 0 | ModuleName | `module` | 0 | 0 | 6 | `module_hash(ModuleName)` |
| rel | 7.. | **6** | 0 | Name | `rel` | 0 | RelArity | 6 | see below |
| column | .. | its rel's id | 1-based | ColumnName | `column` | TypeId | 0 | 6 | see below |

`parent_id` on a rel row changes from `0` to the module row id. That is the
nesting edge; the child-walk index already covers it.

`h_id` for a rel and for a column:

```prolog
%! rel_h_id(+ModuleHash, +LocalName, +Arity, -HashText) is det.
rel_h_id(ModuleHash, LocalName, Arity, HashText) :-
    format(atom(Key), '~w/~w/~w', [ModuleHash, LocalName, Arity]),
    module_hash(Key, HashText).
```

A column's `h_id` uses its OWN local name and arity 0, so
`rel_h_id(ModuleHash, ColumnName, 0, H)`. A primitive's `h_id` is the empty
atom `''`.

### D. where ModuleName comes from

`catalog_row_ddl/3` becomes `catalog_row_ddl/4`, taking ModuleName as its new
FIRST argument. Its one caller in `lower.pl` threads it through from
`lower_program/2`, which takes it from the Plan.

In `compile.pl`, the plan already carries the program name: `program_plan/2`
matches `fixture(Name, ...)`. Pass **that `Name`** as ModuleName. Do not invent
a path lookup, and do not read the filesystem: a term-door fixture has no file,
and one input keeps both doors identical.

### E. tests

`plunit_tests.pl`: update the exact `CREATE TABLE` string (two more columns, and
both appear in the `PRIMARY KEY (...)` list), and the exact seed-row list. The
seed rows now start:

```
(1,0,0,'text','primitive',0,0,0,''),
... primitives 2..5 ...
(6,0,0,'<program name>','module',0,0,6,'<16 hex chars>'),
(7,6,0,'__rel','rel',0,9,6,'<16 hex>'),
(8,7,1,'rel_id','column',2,0,6,'<16 hex>'),
...
```

Compute the real hex by running the compiler, then paste the actual values. **Do
not invent hashes.** If a hash in your test does not match what the compiler
emits, the test is wrong, not the compiler.

Add ONE new test: two rel names that differ only by module produce **different**
`h_id` values. Build two plans with different fixture names, extract each seed
statement, and assert the `h_id` for the same local name differs.

`catalogRows.test.ts`: update the DDL string and any row literal; add one query
asserting `SELECT h_id FROM "__rel" WHERE kind='module'` returns exactly one
non-empty row.

## 3. Validation, run all six, paste output into REPORT.md

```bash
cd v6/prolog/compile   && swipl -q -l test/plunit_tests.pl -g run_tests -g halt
cd ../conformance      && swipl -q -l go.pl -g go -g halt
cd ..                  && bash compile/scripts/text_door_receipt.sh
cd .                   && bash tools/prolog-lint.sh
cd ../tsv2             && pnpm test
cd .                   && bash scripts/sweep.sh
```

| rail | required |
|---|---|
| plunit | 353 / 353 (352 + your one new test) |
| conformance | **306 pass / 0 fail, UNCHANGED** |
| TEXT_DOOR | **compiled=422 byte_identical=422 failures=0, UNCHANGED** |
| prolog-lint | **findings=1 baseline=1, UNCHANGED** |
| tsv2 | 149 pass / 1 skip / 0 fail (plus your added assertions) |
| sweep | **final_wrong=0, UNCHANGED** |

**CRITICAL.** Conformance and TEXT_DOOR must not move by even one. Nothing
outside the catalog names `__rel`, so any movement there means the change leaked
into programs that never use the catalog. STOP, do not commit, report which
program moved.

## 4. Style laws

- A comment states only a constraint the code cannot show. **Max 2 consecutive
  comment lines in new code**; a hook rejects the third.
- Banned in prose AND identifiers: `provenance`, `substrate`, `load-bearing`,
  `regime`.
- No em dashes.
- Descriptive variable names, never single-letter.
- Follow each file's existing style over any general preference.
- Construct names use only rxjs, prolog, or SQL words.

## 5. Deliverable

1. ONE commit on `lane/relhash`, subject:
   `catalog: module identity, one module row and an h_id per rel`
2. `REPORT.md` with the commit sha, all six validation outputs, the actual
   emitted seed-row line for the module row, and a `DEVIATIONS` section. If
   there are no deviations write exactly `none`, and make sure that agrees with
   your hail.
3. **Final action, do not skip**: this harness emits no completion event.

```bash
bus hail --to fable-main --from relhash \
  --body "relhash done: <sha>; plunit <n>, conformance <n>/0, text_door <n>/<n>, sweep final_wrong=<n>; module row: <the emitted line>; deviations: <none|summary>"
```
