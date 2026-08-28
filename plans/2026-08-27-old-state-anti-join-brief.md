# Brief: old-state arm by row_id anti-join (lower.pl)

Work in `$PWD`, your worktree. First action: `git merge --ff-only 3dd679c93c8b7a7915e8c29a3c40f1e1bad75b97`. If it fails, stop and hail.

## Defect

`v6/prolog/lower.pl:672-686` `old_state_relation_sql/4` emits the old-row arm of every delta rule as

```sql
(SELECT <cols> FROM "<table>" old_row GROUP BY <cols>
 HAVING count(*) > (SELECT count(*) FROM "__frontier_<table>" old_delta
                    WHERE old_delta."_phase" >= 0 AND <old_delta.col = old_row.col ...>))
```

Measured live on `ghcache.dl6`: `ghcache___host_response_http__get` holds 17349 rows, 27 MB `body` TEXT + 21 MB `response_headers` TEXT. The GROUP BY sorts all of it per level insert: `level_insert/page_arrival` 187 ms per statement, 21 statements in one clock bucket = 3.9 s (`ghcache_engine_tick_cost`, bucket 29797393).

## Fix

Every stored table with a frontier has `"__id" INTEGER PRIMARY KEY`, and the frontier view already joins by it (`lower.pl:302-306`: `JOIN t ON t."__id" = f."row_id"`). Set rels carry `UNIQUE (<cols>)` (`lower.pl:1302`), so one content group is one row and the count comparison is an existence test. Emit instead:

```sql
(SELECT <projection> FROM "<table>" old_row
 WHERE old_row."__id" NOT IN (SELECT old_delta."row_id" FROM "__frontier" old_delta
                              WHERE old_delta."relation_id" = <RelationId> AND old_delta."_phase" >= 0))
```

- `<projection>` stays what `old_state_projection_columns/5` gives (`__id` first for reference targets, `lower.pl:694-699`).
- `<RelationId>` from `shared_frontier_relation_id/2` (`lower.pl:290`), the same id the view uses.
- Under `frontier_mode(per_rel)` the frontier is the per-rel table `__frontier_<table>`; check whether it carries `row_id` (grep `frontier_ddl` in `lower.pl`). If it carries only columns, keep the old text for that mode and switch on `frontier_mode/1` (`lower.pl:239`). Say which in the PR body.
- Keep `old_state_frontier_where/2` only if some caller still uses it; delete it if not (dead-code rail, `cd v6 && just` lists it).

## Tests to update (they pin the old text)

`v6/prolog/compile/test/plunit_tests.pl:558`, `:578-585`, `:629` assert on `' old_row GROUP BY '`. Rewrite each to assert the new shape: `' old_row WHERE old_row."__id" NOT IN ('` and, for `:578`, that `(SELECT old_row."__id",` still leads the projection.

## Test to add

One plunit test beside `:10935` (the EXPLAIN QUERY PLAN through `sqlite3` precedent): compile a two-rel delta fixture with a `json` column, run EXPLAIN QUERY PLAN on the emitted level insert, assert the plan text contains no `USE TEMP B-TREE FOR GROUP BY` and no `SCAN old_row` line without a following `SEARCH`. Name it `old_state_arm_never_sorts_the_table`. Header states the fail-first receipt: run it before the fix, paste the failing plan line.

## Receipts (all four, numbers in the PR body)

```bash
cd v6 && just plunit                                     # must pass; previous count 765 test/2 heads
bash v6/sprefa-engine-rs/grade.sh                        # RUST-GRADE byte-clean; expect 445/341 unchanged
bash v6/dl/ghcacher/gate.sh                              # GHCACHER goldens=6
bash v6/dl/ghcache/gate.sh                               # GHCACHE_RUST_DOOR_HOLDS ticks=14 account_ticks=14
```

Every one runs in the background with its own log; nothing foreground-waits over 10 s (CLAUDE.md waiting posture). Set `CARGO_TARGET_DIR=$PWD/.target-lane` before any cargo command.

## Ownership

Own: `v6/prolog/lower.pl`, `v6/prolog/compile/test/plunit_tests.pl`.
Forbidden: `v6/dl/**`, `v6/sprefa-engine-rs/src/**`, `emit_rust.pl`, `emit_ts.pl`, `ARCH.pl`, `CLAUDE.md`.
`emit_ts.pl` output for unchanged programs stays byte-identical (user decision 2026-08-21); if a TS golden moves, stop and hail.

## Style laws

- Comments state only constraints the code cannot show; no dates, no arc names, no change-log prose.
- No em dashes, no `provenance/substrate/load-bearing/regime`, no "refusal".
- Descriptive variable names; follow the file's existing style.
- Commit subject: `compiler: PASS - old-state arm anti-joins by row_id`. Post a PR against `main` with the four receipt lines. Then:

```bash
boop beep hail sprefa-coordinator --from <your-lane-name> --body "PR #<n> old-state anti-join: plunit <n>, grade <a>/<b>, ghcacher 6, ghcache ticks=14"
```
