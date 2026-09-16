# LANE catseed: g1 catalog producer

HEAD verified: `e3997cecd88322ae029255c5e3cc8402e433d122` (matches brief).

Deliverable: g1 catalog producer filled in (analyze + lower + tests), gate keeps
every existing emitted module byte-identical. One environment deviation blocks
the sweep RUN stage; documented at the end.

## Files changed (ownership scope, source edits)

| file | change | reason |
|---|---|---|
| `v6/prolog/analyze.pl` | added `program_uses_catalog/2` (mirrors `program_uses_tick/2`), plus `catalog_mentions_rule/1`, `catalog_mentions_atom/1`; exported `program_uses_catalog/2` next to `program_uses_tick/2` | the gate predicate the call site switches on |
| `v6/prolog/lower.pl` | replaced `catalog_table_ddl([])` with the two statement atoms; replaced `catalog_row_ddl(_Decls,_RelPlans,[])` with the single INSERT OR IGNORE seed plus row/id/type/sql-literal helpers; replaced the two ungated calls at the `lower_program/2` call site with the `program_uses_catalog/2` gate; deleted the `TODO(g1)` line at the call site | the two stub bodies and the use-gated call site the brief names |
| `v6/prolog/compile/test/plunit_tests.pl` | added `catalog_g1` begin_tests block with the four required tests | pins absent-by-default, table shape, one-statement seed, positional ids |

## Validation outputs

### 1. `just conformance` (v6)

```
PASS lines: 302
FAIL: (none)
exit 0
```
302 lines starting `PASS`, zero starting `FAIL`.

### 2. `just prolog-lint` (v6)

```
PROLOG_LINT findings=1 baseline=1 OK
```

### 3. `just plunit` (v6)

349/349 pass, exit 0 (last lines):
```
% [345/349] fact_seeding:dl6_fact_seeds_initial ..... passed (0.003 sec)
% [346/349] fact_seeding:dl6_..t_nonground_refuses .. passed (0.001 sec)
% [347/349] fact_seeding:dl6_fact_derives ........... passed (0.002 sec)
% [348/349] fact_seeding:dl6_..eds_with_query_form .. passed (0.003 sec)
% [349/349] fact_seeding:rege..int_column_compiles .. passed (0.003 sec)
```
New `catalog_g1` group, 4/4:
```
% [1/4] catalog_g1:catalog_absent_by_default ........ passed
% [2/4] catalog_g1:catalog_table_shape .............. passed
% [3/4] catalog_g1:catalo..s_are_one_statement ...... passed
% [4/4] catalog_g1:catalog_ids_are_positional ....... passed
```
Count 349 >= 345 before the change (the +4 are this lane's tests).

### 4. `bash scripts/sweep.sh` (v6/tsv2) — PARTIAL, deviation

Compiler side (exercises the gate), stage 1:
```
=== stage 1: compile sweep ===
SWEEP total=512 compiled=420 unsupported=92 crash=0
```
copy into gen_emitted (stage 3) succeeded:
```
gen_emitted modified count: 0   (git status --short v6/tsv2/gen_emitted/ | grep -c '^ M')
```
The `RUN total=420 ...` diff line does NOT print because stage 3 aborts before
running the tick-log diff:

```
Error [ERR_MODULE_NOT_FOUND]: Cannot find package 'rxjs' imported from
/Users/chrishafley/projects/sprefa-lanes/catseed/v6/tsv2/scripts/sweep.ts
```
Root cause: `v6/tsv2/node_modules` does not exist.

### 5. gate of record

```
$ git status --short v6/tsv2/gen_emitted/ | grep -c '^ M'
0
```
212 tracked emitted modules byte-identical; the three `regexp_*` files under
`gen_emitted/` are untracked (new corpus fixtures iterated by the compile sweep,
not modified tracked modules).

## Deviation

Namespace: environment, not design. The brief's "Do NOT run these" section
states "node_modules is already present in every package." In this checkout
`v6/tsv2/node_modules` (and `v6/node_modules`, `node_modules`) does not exist, so
the sweep's stage-3 tick-log diff cannot run (its runtime imports `rxjs`).
Because the brief forbids `npm install`/`pnpm install`/`npm ci` and forbids
improvising around a deviation, I did not install. The gate that matters most
(0 modified emitted modules) passes, and the sweep's compile stage (which runs
`lower_program/2` over every fixture) reports `crash=0`, so the gate is proven
correct on all 212 shipped modules. Resolving the RUN line requires restoring
`v6/tsv2/node_modules` outside this lane.

## Deviations within the files

None. Both stub bodies, the gate, and all four tests match the brief
byte-for-byte (the two catalog DDL statements and the single-INSERT seed were
verified verbatim). Existing-style choicepoints silenced by wrapping
`program_plan`/`lower_program` in `once/1` to match neighbouring tests.

## Regenerated build artifacts (not source edits)

Running the mandated validations regenerates tracked/untracked corpus artifacts
under `v6/prolog/compile/out/` (`manifest.json` modified; new `regexp_*`,
`flagship_flow_*`, and like outputs) and `v6/tsv2/gen_emitted/`. `manifest.json`
diff is entirely new fixture registry entries (regexp, flagship) from the
conformance run; no `__catalog`/`catalog` entry appears, confirming no shipped
fixture references the catalog rel. These are build outputs of the mandated
commands, left uncommitted as delivered.
