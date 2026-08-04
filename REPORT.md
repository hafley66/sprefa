# Rename `__support_count` -> `__refcount` everywhere emitted

## Lane entry
- Worktree HEAD at start: `d84a1770` (matches required)
- Branch: `lab/refcount-ddl`

Process: no source/deviation STOP triggered. pnpm installs run in `v6/tsv2` and
`v6/sprefa-store/js` (node_modules absent).

## Site list

`rg -l "__support_count" v6 --glob '!node_modules'` found:

- hand-edited (source sites)
  - `v6/prolog/lower.pl` (9)
  - `v6/prolog/compile/test/plunit_tests.pl` (4)
  - `v6/tsv2/tests/edgeGuard.test.ts` (1)
- regenerated (generated artifacts, never hand-edited)
  - `v6/prolog/compile/out/*.ts` (144 files, produced by `just sweep` stage 1)
  - `v6/tsv2/gen_emitted/*.ts` (145 files; 144 produced by `just sweep` stage 3,
    plus `door-handwritten.ts` regenerated via `compile_dl6.sh`)

`v6/prolog/emit_ts.pl` and `v6/tsv2/runtime/*.ts` were listed as candidate
source sites but had zero matches.

## Empty-token proof

`rg -c "__support_count" v6 --glob '!node_modules'` at end: empty, exit 1
(no matches).

## Gate tails (run from v6/, all exit 0)

### just plunit
```
% [324/324] dot_member_access..oal_refuses_by_name .. passed (0.000 sec)
EXIT=0
```
324/324 passed.

### just conformance
```
PASS  over_baseline_gate_blocks_commit_only
PASS  fix_by_waiver_returns_to_clean
PASS  new_file_diag_at_hit_line_exact_rows
PASS  new_file_no_exceeded_diag
PASS  unwrap_aggregate_and_interpolation
PASS  unwrap_unchanged_file_silent
PASS  unwrap_below_budget_silent
PASS  tightened_baseline_catches_regrowth
EXIT=0
```

### just text-door
```
TEXT_DOOR compiled=206 byte_identical=206 failures=0
EXIT=0
```

### just tsv2-test
```
ℹ tests 146
ℹ suites 0
ℹ pass 144
ℹ fail 0
ℹ cancelled 0
ℹ skipped 2
ℹ todo 0
ℹ duration_ms 6327.877
EXIT=0
```

### just sweep
```
RUN total=206 identical=205 wrong=0 emitted_crash=0 rejection=1 no_oracle_log=0
  REJECTION log_retraction_rejected retract from log rel 'event'
FINAL total=206 final_identical=205 final_wrong=0 no_oracle_final=1
  NO_ORACLE_FINAL log_retraction_rejected oracle threw on this schedule too; no final state to diff
MANIFEST_REASON_DIFF restated=0 args=0 bucket_moved=0 added=0 removed=0 (informational)
EXIT=0
```

## Deviations
None.
