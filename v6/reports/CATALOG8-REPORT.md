# lane catalog8 step 8

Catalog backbone step 8: widen the emitted `rel_catalog` TS const from the
decl-only block to the full `catalog_all_rows/10` block (decl + plane), per
ruling `catalog_plane_in_const` (F1=A). Pass 1 of 2.

## The change

- `v6/prolog/emit_ts.pl` `program_catalog_rows` (arity /10 now): renders the
  full `lower:catalog_all_rows/10` block (decl + plane) instead of the
  decl-only `lower:catalog_rows/4`. The call site threads the plane inputs it
  already computes: `InternMode`, `PlanDecls`, `PlanRules`, `DepartureRefs`,
  `PreRefs`, `LoweringTypes` (from `type_definitions/2`), and
  `RuleLevelStatements`.
- `v6/tsv2/runtime/types.ts` `IRelCatalogRow.kind` union widens from five
  kinds to the full emitted set: adds `delta`, `frontier`, `next_frontier`,
  `departure`, `pre`, `view`, `dictionary`, `refcount`, `refcount_staging`,
  `expand`, `dred`, `scope`, `avg_accumulator`, `port`, `port_response`,
  `storage`.

Two commits:
- `de89b38d` catalog: widen the emitted rel_catalog const to the plane block (step 8, F1=A)
- `5b0a4876` catalog: the deliberate step-8 regen (F1=A) — 231 emitted modules

## Gates (verbatim)

    cd v6/tsv2 && bash scripts/sweep.sh     # crash=0, RUN wrong=0, FINAL wrong=0 (oracle-identical), manifest all zeros
        SWEEP total=330 compiled=231 unsupported=99 crash=0
        RUN total=231 identical=230 wrong=0 emitted_crash=0 rejection=1 no_oracle_log=0
        FINAL total=231 final_identical=230 final_wrong=0 no_oracle_final=1
        MANIFEST_REASON_DIFF restated=0 args=0 bucket_moved=0 added=0 removed=0
    cd v6 && just text-door                 # 231/231/0
        TEXT_DOOR compiled=231 byte_identical=231 failures=0
    cd v6 && just plunit && just tsv2-test  # 0 fail
        plunit 496 pass, 0 fail
        tsv2-test 188 pass, 0 fail
    bash scripts/sweep.sh AGAIN, then git status --short v6/prolog/compile/out  # EMPTY: regen is a fixed point
        EMPTY (repeat sweep regenerates byte-identical modules; no second regen)
    cd v6 && just green-all                 # fail set EXACTLY the expected 7
        FAIL scale-floor, memory-soak, prolog-lint, lsp-diags, compile-speed, typecheck, rtkq-golden

All 7 green-all fails are the pre-change baseline set (recorded identically in
CATALOG456-REPORT.md). Verified pre-existing, not introduced by step 8:
- `prolog-lint` NEW finding `undefined_predicate emit_ts:run_incremental_tick_fn_lines/8`
  reproduces at base commit 38522d5a (checked in a detached worktree).
- `typecheck` error is at `golden-flex.ts:3531` inside `run_naive_tick`
  (`Observable<unknown>` vs `ITickDeltas`), unrelated to the catalog const.

## Const growth (one module)

Module: `v6/prolog/compile/out/aggregate_count_min_max_track_arrivals_and_retraction.ts`
(before = pre-widen emitted at regen parent; after = regenerated).

| measure | before | after |
| --- | --- | --- |
| module bytes | 25073 | 28498 |
| rel_catalog block bytes | 2429 | 5854 |
| rel_catalog share | 9.7% | 20.5% |

Plan section 5 predicted 7.7% -> ~15%; measured landing is 9.7% -> 20.5% on
this module (the module carries a scope + storage dense plane block; the exact
multiple varies by module, matching the "one deliberate regen" design).

## Deviations

- Plan section 7 step 8 text calls for a compile-speed re-pin in the same PR.
  OVERRIDDEN by the brief: no re-pin, because the baseline's 8 regressions are
  unattributed and an open bisect owns them.
- No emitted-DDL changes, no `lower.pl` changes (plane rows already emitted by
  the DDL seed in steps 2-6; step 8 only widens the TS const).

## Regrid summary

- `de89b38d` code (emit_ts.pl + types.ts), separate from
- `5b0a4876` the deliberate regen (231 modules). The repeat-sweep fixed-point
  check is clean (`git status --short v6/prolog/compile/out` empty), so there
  is no nondeterminism and no second regen was needed.
