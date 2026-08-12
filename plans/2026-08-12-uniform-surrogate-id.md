# Uniform surrogate id for every set rel

## Context

`v6/prolog/lower.pl` emitted set-rel tables in two shapes:

| branch | `lower.pl` | shape |
|---|---|---|
| reference target (a declared type name) | :2101-2103 | `CREATE TABLE x ("__id" INTEGER PRIMARY KEY, <cols>, UNIQUE (<cols>))` |
| ordinary set rel | :2112-2114 | `CREATE TABLE x (<cols>, PRIMARY KEY (<cols>)) WITHOUT ROWID` |

The second shape is a composite TEXT primary key, which the repo's own
surrogate-keys law (`sql-relational-design`) calls a DEFECT (measured
1.7-2.0x slower on identical tables, every index copies the full key). The
split is also why a zero-column rel had no table at all.

User decision 2026-08-12, verbatim: "TAKE THE CORRECT AND MOST CONSISTENT ONE
EVEN IF IT MEANS MORE WORK." The decision is settled; this arc implements it.

## Decisions

1. **One shape for every set rel.** Both branches collapse onto
   `CREATE TABLE "x" ("__id" INTEGER PRIMARY KEY, <cols>, UNIQUE (<pk>))`.
   The `UNIQUE` is over `PkSql` (the declared key when keyed, else all
   columns), which is what `set_rel_pk_sql/6` already produced. This keeps
   the existing `ON CONFLICT (<key>)` and `INSERT OR IGNORE` conflict/dedup
   targets identical to the previous `PRIMARY KEY (<pk>)`, while the table
   gains the `__id` surrogate and the composite TEXT PK is gone.
   - Rejected alternative: `UNIQUE` over strictly all columns. That would
     widen the conflict target past the keyed rels' `ON CONFLICT (<key>)` and
     SQLite rejects a subset conflict target against a wider UNIQUE
     (`ON CONFLICT clause does not match any PRIMARY KEY or UNIQUE
     constraint`). Using `PkSql` preserves keyed-replace exactly.
   - Rejected alternative: keep `WITHOUT ROWID` for keyed set rels. That is
     a third shape and violates "one shape, no exceptions".

2. **Zero column, no degenerate constraint.** At zero columns the target is
   `CREATE TABLE "x" ("__id" INTEGER PRIMARY KEY)` with no trailing comma and
   no `UNIQUE ()`. A 0-ary rel is a proposition: there is no content, so there
   is no dedup, and every arrival mints a row.

3. **Keyed replace semantics unchanged.** Because `UNIQUE` carries `PkSql`
   (key-or-all), the keyed UPSERT `ON CONFLICT (<key>) DO UPDATE SET <nonkey> =
   excluded.<nonkey>` matches, keeps `__id`, and last-write-wins still holds.
   No reader of the old `WITHOUT ROWID` PK needed a semantic change.

4. **The 0_generic_expand.pl stop stays.** Probe C (every column moved out by
   an option split) still cannot compile green: removing the stop at
   `0_generic_expand.pl:278-284` unblocks the named throw but the rel is then
   dropped from `AllRefs`/`RelPlans` entirely (no `kind`/`col_type`/`keyed`
   declaration survives, so no DDL is emitted for it) and a downstream
   `column_type_unknown` fires on `combo_move`. The existing conformance
   fixture `reference_target_emptied_by_option_split_is_named` asserts the
   stop's throw. Both removal conditions (probe C green AND a two-parent
   distinctness fixture) are unmet, and the real fix reaches files outside
   this lane (`analyze.pl`, `compile.pl`, `0_type_plane.pl`, `registry.pl`).
   See `plans/2026-08-12-zero-column-ref-target.REFUTATION.md` for the
   independent control. The stop is left in place.

## Verification

### Gates, before and after (identical counts)

| gate | result |
|---|---|
| `sweep.sh` | total=286 identical=283 wrong=0 |
| `just text-door` | compiled=288 byte_identical=288 failures=0 |
| `just conformance` | 392 PASS, 0 FAIL |
| `just dd-grade` | DD-GRADE HOLDS |
| `swipl -g go -t halt ARCH.pl` | rc 0 |

`text-door` byte-identity moved by design: every moved byte is the emitted
DDL changing from `PRIMARY KEY (<pk>) WITHOUT ROWID` to `__id INTEGER PRIMARY
KEY, ..., UNIQUE (<pk>)`. `sweep`'s identical/wrong counts did not move; the
computed answers are unchanged (bench checksums identical below).

### Fail-first arity-0 receipt (step 1)

```bash
printf 'rel zed().\nrel w(id: int).\n\nw(1) <- zed().\n' > /tmp/zed.dl6
bash v6/prolog/compile/scripts/compile_dl6.sh /tmp/zed.dl6 /tmp/zed.ts; echo rc=$?
```

Before the fix: `rc=1` with nothing on stdout or stderr (the silence was part
of the defect). After: `rc=0` with a compile trace and `zed` emits
`CREATE TABLE "zed" ("__id" INTEGER PRIMARY KEY)`. The silence is removed.

### Price: dropping `WITHOUT ROWID` (step 5)

`just dl6-bench-full`, before = base `154ae23c`, after = this branch. One run
per case, no best-of; single-digit percentages are noise. Checksums identical
means the answers did not move.

| case | before fixpoint ms | after fixpoint ms | delta | before rows/s | after rows/s | before RSS | after RSS |
|---|---|---|---|---|---|---|---|
| grid_10000 | 1235 | 1365 | +10.5% | 865,749 | 783,297 | 598 MB | 635 MB |
| layered_10000 | 11886 | 14079 | +18.4% | 837,237 | 706,825 | 1208 MB | 1390 MB |
| chain_10000 | 23820 | 27353 | +14.8% | 419,656 | 365,452 | 1419 MB | 1696 MB |

- checksums: grid `9d7239568960d6a8`, layered `addcf85b5162b9da`, chain
  `df09b2f409f8b9a8` — identical before and after.
- `dl6-budget`: grid fixpoint 1181ms -> 1371ms, still under the 2500 ceiling
  (RSS 597 -> 627 MB, under 900).
- `dl6-dred-bench` (incremental refCount, does not rebuild the public table):
  insert_one 26->26, delete_one 54->51, drain 1->0, delete_structural 80->76,
  insert_batch100 2850->2928, delete_batch100 10662->10813. Within noise.
- `perf-all`'s Rust legs (shootout, store-rig, profile_dred, sqlite_baseline)
  do not consume `lower.pl` output and are unaffected; the affected `dl6*`
  legs are reported above.

The fixpoint is 10-18% slower and RSS up to ~20% higher. This is the price of
consistency, matching `sqlite-costs` ("WITHOUT ROWID beats rowid+UNIQUE on a
fixpoint head"). It is a recorded finding, not a reason to abandon the shape;
the user chose consistency knowing it means more work.

<!-- todo(perf): public set-rel tables moved off WITHOUT ROWID; fixpoint 10-18% slower (grid/layered/chain), RSS up to ~20% higher. Bench numbers banked in plans/2026-08-12-uniform-surrogate-id.md. -->
<!-- todo(feature): zero-column reference target still refused (reference_target_has_no_columns); the fix needs registration in analyze.pl + lowering + type plane, outside this lane. -->

## Staffing

One lane. Worktree: `refactor/uniform-surrogate-id`, base `154ae23c`. Files
owned: `v6/prolog/lower.pl`, `v6/prolog/analyze.pl` (arity-0 only),
`v6/prolog/0_generic_expand.pl` (stop untouched), conformance fixtures,
`v6/prolog/compile/dl_view/**` (regenerated), this plan doc. Forbidden:
`compile.pl`, `6_emit_dd_plan.pl`, `.github/**`, `v6/bench-cli/**`,
`v6/labs/**`, `chat_log/**`.
