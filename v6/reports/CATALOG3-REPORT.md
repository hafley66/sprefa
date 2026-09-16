lane catalog3 step 3

## Gates (verbatim)

`cd v6 && just plunit`
```
489 tests, 0 failures (EXIT 0, **FAILED count = 0)
```
Verified my added groups: `catalog_g1` (15) and `catalog_plane_rail` (1) all pass.

`cd v6 && just conformance`  (the brief's `run.pl` form does not exist; `just conformance` is the gate)
```
PASS  tightened_baseline_catches_regrowth   (last of 330 lines)
PASS_lines=330  FAIL_lines=0  exit=0
```

`cd v6 && just text-door`
```
TEXT_DOOR compiled=231 byte_identical=231 failures=0
```

`cd v6/tsv2 && bash scripts/sweep.sh`
```
RUN total=231 identical=230 wrong=0 emitted_crash=0 rejection=1 no_oracle_log=0
FINAL total=231 final_identical=230 final_wrong=0 no_oracle_final=1
MANIFEST_REASON_DIFF restated=0 args=0 bucket_moved=0 added=0 removed=0 (informational)
```
crash=0, RUN wrong=0, FINAL wrong=0, MANIFEST diff all zeros.

`git diff --stat v6/prolog/compile/out`
```
(no output)   # zero emitted modules changed; the plane rows are SQL-seed-only until step 8
```

`cd v6 && just green-all` — fail set exactly (no eighth):
```
FAIL  scale-floor       FAIL  memory-soak     FAIL  prolog-lint
FAIL  lsp-diags         FAIL  compile-speed   FAIL  typecheck
FAIL  rtkq-golden
```
Expected set: compile-speed lsp-diags memory-soak prolog-lint rtkq-golden scale-floor typecheck. Matches exactly; nothing else fails.

compile-speed is a pre-existing fail (attribution open). Confirmed NOT altered: swapping in the base `lower.pl` and re-running reports the identical `COMPILE_SPEED regressions=9 improvements=0 FAIL` with the same nine rows (flagship-flow lower+39.5%/emit+23.4%, golden-flex plan+12.6%/lower+58%/emit+21.6%, flagship-callgraph lower+71.2%/emit+20.4%, door-handwritten lower+103.8%/emit+13.3%). My change adds zero inference work to these pinned programs: `catalog_row_ddl/8` (the only path that runs the plane generator) is gated on `UsesCatalog`, false for all four, and the `catalog_rows/4` refactor is work-identical.

prolog-lint is a pre-existing fail (findings=2 baseline=1). Confirmed identical to base (`findings=2 baseline=1`, single NEW finding `undefined_predicate compile-(emit_ts:run_incremental_tick_fn_lines/8)`); my change adds no findings.

`cd v6/prolog && swipl -q -g "consult(lower),halt" -t halt` prints nothing (no singleton warnings).

## Plane-row counts per family

Measured live over the conformance corpus at `intern(dict)`, via the same `catalog_all_rows/8` the seed renders:

| kind | local_name pattern | count |
| --- | --- | --- |
| delta | `__delta_<rel>` | 849 |
| frontier | `__frontier_<rel>` | 849 |
| next_frontier | `__next_frontier_<rel>` | 849 |
| departure | `__departure_frontier_<rel>` | 6 |
| view | `__txt_<table>` (rel + delta views) | 1384 |
| pre | `__pre_<rel>` | 21 |
| dictionary | `__str` + `__ref_<Type>` | 260 |
| **TOTAL** | | **4218** |

Compare to the plan's 3097 for these families (plan 7.3):

- The plan's own numbers do not self-consistently sum to 3097. Plan 3.3 lists the same families as delta 776, frontier 776, next_frontier 776, departure 6, view 1264, pre 18, dictionary (str 191 + ref 46) 237, which totals 3853, not 3097. 3097 appears to be a stale per-module projection from before the corpus grew.
- My measurement runs over the current live corpus: 246 fixtures lower at `intern(dict)` (the plan measured 220 emitted modules). Dragging every per-rel family up (delta/frontier/next 849 vs 776) and the intern-derived families (view 1384 vs 1264, pre 21 vs 18, dictionary 260 vs 237). departure matches the plan exactly (6).

The corpus-wide name gate (the one test the plan calls the highest-value artifact) passes with zero mismatches, so every plane row agrees with its DDL mint site: no `view` row where `text_view_ddls/6` emitted nothing, no `__str`/`__ref_` row outside dict/struct-type existence, no delta/frontier/next row for a table the lowering did not create.

## Files touched

- `v6/prolog/lower.pl` (+211/−): `catalog_row_ddl/8` and `catalog_all_rows/8` grow to thread `Mode, DepartureRefs, PreRefs, Types` (mirroring the `lower_program/2` call-site derivations); `catalog_rows/4` refactored to `/4` delegating to `catalog_decl_rows/5` (decl block, byte-stable) which returns the `FinalId` the plane block starts at; the plane families `catalog_plane_rows/8` plus per-rel (`delta`, `frontier`, `next_frontier`, `departure`, `pre`, `view`) and per-module (`dictionary` = `__str` + `__ref_<Type>`) generators. Export updated to `catalog_all_rows/8`.
- `v6/prolog/compile/test/plunit_tests.pl` (+165/−): corpus-wide `catalog_plane_rail` gate (DDL CREATE-names vs plane-row local_names, in the interned-storage-rail shape); ids-stable receipts (`catalog_all_rows_equals_decl_rows`, `catalog_seed_ddl_byte_identical_after_split`); plane-kind checker reconstructed from the plan's `Mode`/`DepartureRefs`/`PreRefs`/`Types`.

`emit_ts.pl` untouched (the TS const does not widen until step 8). `v6/prolog/compile/out` untouched (0 files in the diff-stat).

## Deviations

- **Rel-view arity undercounts a level-headed `__refcount` passthrough by 1.** A level-headed rel's `__txt_<rel>` view also carries `t."__refcount"`; my `view` row arity counts `__id` (+1 for declared struct types) but not `__refcount`. Level-headedness is a step-4 input (LevelHeadedRefs), so I deferred it rather than thread it early. The name-gate does not read arity, so the corpus check still passes; noted here for the step-4 debur pass, which should add the `__refcount` passthrough to the arity.
- The brief's `run.pl` conformance invocation does not exist; the gate is `just conformance` (330/0).
- `pnpm install` was run once in `v6/tsv2` and `v6/sprefa-store/js` to populate `node_modules` (rxjs, better-sqlite3) so sweep's stage-3 TS diff leg can run. Environment setup, not a source change; no tracked file modified by it.
- `kind` for the `__txt_` family is `view` (the user-lawed SQL word per plan 11.3).

## Next action

The Rules-derivable plane families are scaffolded and name-agree with their DDL mint sites corpus-wide; hand to the step-4 lane (level-statement planes: scope, refcount, refcount staging, expand, dred, avg) with note of the `__refcount` arity deviation.

bus hail --to fable-main --kind result --body "catalog3 done: Rules-derivable plane rows (delta/frontier/next_frontier/departure/view/pre/dictionary) scaffolded; corpus-wide DDL-vs-plane name gate + ids-stable tests green (plunit 489/0, conformance 330/0, text-door 231/231/0, sweep wrong=0); compile/out clean; green-all fails only the expected 7."
