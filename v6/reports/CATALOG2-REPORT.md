lane catalog2 step 2

## What changed

Split the catalog producer per plan 7 step 2. `catalog_rows/4` (decl half)
is untouched; the seed path now renders from a new `catalog_all_rows/5` =
decl rows ++ plane rows, where the plane half is an empty scaffold.
`program_catalog_rows/4` in emit_ts.pl still calls `catalog_rows/4`, so the
emitted TS const is unchanged.

### files touched

| file | change |
| --- | --- |
| v6/prolog/lower.pl | `catalog_all_rows/5` + scaffold `catalog_plane_rows/5` (returns `[]`); `catalog_row_ddl/5` now calls `catalog_all_rows/5`; exported `catalog_all_rows/5` |
| v6/prolog/compile/test/plunit_tests.pl | imported `catalog_all_rows/5`; added 2 tests + helper `catalog_seed_render/3` |

emit_ts.pl: not modified (requirement was to keep `program_catalog_rows/4`
on `catalog_rows/4`).

## New tests

1. `catalog_all_rows_equals_decl_rows` — `catalog_all_rows/5` with the empty
   plane half equals `catalog_rows/4` output exactly (identity receipt).
2. `catalog_seed_ddl_byte_identical_after_split` — rendering the decl half
   vs the full list yields the same seed statement, and that statement
   matches what the live DDL emits (byte-identical before/after).

## Gates (verbatim)

    cd v6 && just plunit
    -> 488 tests, 0 failures, exit 0

    cd v6 && just text-door
    -> TEXT_DOOR compiled=231 byte_identical=231 failures=0

    cd v6 && just tsv2-test
    -> tests 189, pass 187, fail 0, skipped 2

    git diff --stat v6/prolog/compile/out
    -> (prints nothing)

## Deviations

- tsv2-test initially failed on a fresh worktree because its environment was
  not staged: `v6/sprefa-store/js/node_modules` was absent (rxjs unresolvable
  from the linked engine) and `v6/tsv2/gen_emitted/` was empty. Restored via
  `pnpm install --prefer-offline` in sprefa-store/js (pnpm per the laws) and
  `scripts/sweep.sh` to populate `gen_emitted/`. No source-bearing change.
  `git diff --stat v6/prolog/compile/out` prints nothing after the sweep.
- emit_ts.pl is listed in ownership but required no edit this step.
