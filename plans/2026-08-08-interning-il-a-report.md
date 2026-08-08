# REPORT-IL-A — family A: departure reads at `intern(dict)`

Lane I-L family A, branch `lane/i-l-departed-reads`, base `9cea5cd0` verified
first action. Scope: the 5 departure/pre modules on
`plans/2026-08-08-flip-referee-red.md`'s family-A row.

## TOC

| § | contents |
|---|---|
| 1 | the 5 reproduced first-diffs |
| 2 | root cause, one mechanism |
| 3 | the fix |
| 4 | before/after SQL, pairwise |
| 5 | gate receipts |
| 6 | fail-first |
| 7 | corrections to the referee doc |
| 8 | noted for families B/C/D, not fixed |

## 1. The 5 reproduced first-diffs

Atom flipped to `dict` in the lane worktree, `bash scripts/sweep.sh`.
Reproduced the referee's numbers exactly: **RUN wrong=13, FINAL wrong=17**.

| module | first diff |
|---|---|
| `departed_fires_next_tick_on_retraction` | `line 3: actual {"closed_at":{"add":[[null,3]]}}` vs oracle `[["alpha",3]]` |
| `keyed_replace_departs_the_old_row` | `line 3: actual {"replaced_value":{"add":[[null,null]]}}` vs oracle `[["cli","v1"]]` |
| `pairwise_reads_state_at_the_departure_tick` | `line 3: actual` has `reading` only; oracle also carries `"step":{"add":[["north",10,9]]}`. FINAL: actual `step` absent, oracle `[["north",10,9],["north",14,9]]` |
| `pairwise_pairs_adjacent_values_when_the_source_idles` | `line 3: actual {"deltas":{}}` vs oracle `{"step":{"add":[["north",10,14]]}}` |
| `finalize_over_log_fires_on_retention_prune` | `line 4: actual {"gone":{"add":[[1,null]]}}` vs oracle `[[1,"a"]]` |

Two symptoms, one cause: a text column rendering NULL (rows 1, 2, 5) and an
arm returning zero rows (rows 3, 4).

## 2. Root cause, one mechanism

The departure frontier is the ONE table in the emitted program that SQL does
not fill. `IncrementalRuntime.stage_departures`
(`v6/tsv2/runtime/1_incremental.ts:1363-1390`) stages the tick's boundary
delta `del` rows into it, and those rows came out of `boundary_delta`
(`:880-921`) reading `boundary_sql`, which selects from the DECODED view
`__txt___delta_<rel>`. So the staged values are CHARACTERS under every mode.

`departure_frontier_ddl/5` declared the same columns the rel stores. At dict
that is `INTEGER NOT NULL`, and every read of the table assumed an id:

```
CREATE TEMP TABLE "__departure_frontier_reading"
  ("_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL,
   "sensor" INTEGER NOT NULL, "previous" INTEGER NOT NULL)
```

SQLite's INTEGER affinity leaves a non-numeric string as TEXT, so the column
silently held `'north'`. Two failure planes follow, one per read site:

| plane | emitted-SQL site | lower.pl clause that emitted it | what breaks |
|---|---|---|---|
| join | `pairwise_reads_state_at_the_departure_tick.ts:279` `project_sql`, `... WHERE b0."sensor" = d0."sensor"` | `edge_delta_project_sql/12`, `compile_atom_args(Mode, TriggerArgs, ...)` (was `lower.pl:2192`) | TEXT vs INTEGER never compare equal in SQLite, so the arm returns 0 rows and `step` never fires |
| projection | `departed_fires_next_tick_on_retraction.ts:302` `project_sql`, `SELECT d0."item" AS "item"` | same clause, via `head_select_list/7` | characters go into `closed_at."item"` (INTEGER), then `__txt_closed_at` (`:177`) decodes `s."__id" = 'alpha'` and finds nothing -> `null` |

The per-occurrence arm has the same two planes one hop out: the JS resolver
reads the frontier through `departure_read_sql/3`
(`emit_ts.pl:1584`, const `EDGE_<HEAD>_<N>_DEPARTURE_SQL`) and binds the row
back as `?1..?n`, which `compile_trigger_bound/4` had declared `dict`.

Per module:

| module | plane |
|---|---|
| `departed_fires_next_tick_on_retraction` | projection (`closed_at."item"`) |
| `keyed_replace_departs_the_old_row` | projection, both columns (`replaced_value."key"`, `."old_value"`) |
| `pairwise_reads_state_at_the_departure_tick` | join (`b0."sensor" = d0."sensor"`) AND projection |
| `pairwise_pairs_adjacent_values_when_the_source_idles` | same, same rule text |
| `finalize_over_log_fires_on_retention_prune` | projection (`gone."payload"`); departures reach the frontier through `apply_retention_statement` (`1_incremental.ts:854`) -> `stage_events` -> the same boundary read |

`pre/1` is NOT affected and needed no change: `__pre_<rel>` is filled by
`INSERT INTO "__pre_x" (...) SELECT ... FROM "x"` (`emit_ts.pl:1639-1641`),
SQL to SQL, ids to ids.

## 3. The fix

One new predicate, three call sites. `lower.pl` only; zero runtime, zero
emitter, zero `types.ts` change.

```prolog
% lower.pl:197-201
% The runtime fills the departure frontier from the tick's boundary delta,
% whose rows already crossed the decoded text view: characters under any mode.
trigger_read_mode(departure, _, direct) :- !.
trigger_read_mode(ordered_departure, _, direct) :- !.
trigger_read_mode(_, Mode, Mode).
```

| # | clause | change |
|---|---|---|
| 1 | `departure_frontier_ddl/4` (`:4387`, was `/5`) | dropped the `Mode` argument, takes the frontier mode from `trigger_read_mode/3`; text columns stay `TEXT NOT NULL` at both modes. Single call site `delta_ddl/4` (`:4376`) updated |
| 2 | `edge_delta_project_sql/12` (`:2204-2205`) | `compile_atom_args(TriggerMode, ...)` for the trigger's own args; `OtherAtoms`, `PreAtoms`, guards, negations and `head_select_list/7` still take the program `Mode` |
| 3 | `edge_statement_single/10` (`:2084-2085`) | `compile_trigger_bound(TriggerMode, ...)`, so a departure placeholder is `direct` and an arrival placeholder stays `dict` (the ingest door already interned it) |

No new SQL shape and no second statement: the id resolution lands on the
machinery lane I-K already built, at the two points where a `direct` value
meets a `dict` column.

- join: `aligned_pair/6` (`:304`) -> `align_to_encoding(dict, direct, ...)` ->
  `interned_id_sql/2`. The characters side resolves; the STORED column stays
  bare on its own side, so the indexed column is still probeable.
- projection: `head_column_expr/6` (`:928`) wraps the `direct` expression in
  `interned_id_sql/2` because the head column's encoding is `dict`.

Totality of the lookup: a departed value was stored in the rel's table, so it
was interned to get there, and `__str` is append-only within a run. No boot
seed is owed and no NOT NULL row can drop.

Why the frontier is not moved to ids instead (the `sql-relational-design`
question). Interning at the write would mean the runtime resolving each staged
value, which needs a per-column encoding on `IIncrementalRelationPlan`, a
`types.ts` field, and a read in `stage_departures` — three files owned by other
lanes, to buy nothing measurable: the table is TEMP, cleared and refilled every
tick, holds only that tick's net departures, carries no PK and no index, and
the `sqlite-costs` TEXT tax is a btree-key tax on tables that have keys. The
column that IS keyed and indexed (`reading."sensor"`) keeps its integer id and
stays bare in the comparison. Declaring the staging table to match what the
staging code writes also makes dict byte-equal to direct in shape, which is
what made the direct gate free.

## 4. Before/after SQL, `pairwise_reads_state_at_the_departure_tick`

DDL, at dict:

```sql
-- before
CREATE TEMP TABLE "__departure_frontier_reading" ("_phase" INTEGER NOT NULL,
  "_sequence" INTEGER NOT NULL, "sensor" INTEGER NOT NULL, "previous" INTEGER NOT NULL)
-- after (identical to the direct-mode text)
CREATE TEMP TABLE "__departure_frontier_reading" ("_phase" INTEGER NOT NULL,
  "_sequence" INTEGER NOT NULL, "sensor" TEXT NOT NULL, "previous" INTEGER NOT NULL)
```

Delta arm, at dict:

```sql
-- before: 'north' = 1 is never true; the arm returns zero rows
SELECT d0."sensor" AS "sensor", d0."previous" AS "previous", b0."previous" AS "current"
FROM "__departure_frontier_reading" d0, "reading" b0
WHERE d0."_phase" >= 0 AND b0."sensor" = d0."sensor"
ORDER BY d0."_phase", d0."_sequence"

-- after: the characters side resolves, the stored column stays bare
SELECT (SELECT s."__id" FROM "__str" s WHERE s."content" = d0."sensor") AS "sensor",
       d0."previous" AS "previous", b0."previous" AS "current"
FROM "__departure_frontier_reading" d0, "reading" b0
WHERE d0."_phase" >= 0
  AND b0."sensor" = (SELECT s."__id" FROM "__str" s WHERE s."content" = d0."sensor")
ORDER BY d0."_phase", d0."_sequence"
```

Per-occurrence arm, same module, at dict:

```sql
-- before
SELECT ?1 AS "sensor", ?2 AS "previous", b0."previous" AS "current"
FROM "reading" b0 WHERE b0."sensor" = ?1
-- after
SELECT (SELECT s."__id" FROM "__str" s WHERE s."content" = ?1) AS "sensor",
       ?2 AS "previous", b0."previous" AS "current"
FROM "reading" b0
WHERE b0."sensor" = (SELECT s."__id" FROM "__str" s WHERE s."content" = ?1)
```

At `intern(direct)` all three texts are byte-identical to base.

## 5. Gate receipts, verbatim

**(a) dict, pre-fix** — the reproduction:

```
SWEEP total=308 compiled=211 unsupported=97 crash=0
RUN total=211 identical=197 wrong=13 emitted_crash=0 rejection=1 no_oracle_log=0
FINAL total=211 final_identical=193 final_wrong=17 no_oracle_final=1
MANIFEST_REASON_DIFF restated=0 args=0 bucket_moved=0 added=0 removed=0
```

**(b) dict, post-fix:**

```
SWEEP total=308 compiled=211 unsupported=97 crash=0
RUN total=211 identical=202 wrong=8 emitted_crash=0 rejection=1 no_oracle_log=0
FINAL total=211 final_identical=198 final_wrong=12 no_oracle_final=1
MANIFEST_REASON_DIFF restated=0 args=0 bucket_moved=0 added=0 removed=0
```

The remaining 12 are exactly the referee's families B, C and D, no additions:

| family | modules still wrong |
|---|---|
| B (4) | `ordered_group_concat_value`, `ordered_group_concat_ordinal`, `ordered_mermaid_line_assembly`, `ordered_fragment_line_assembly` |
| C (4) | `struct_nested_value_renders_whole_tree`, `struct_ghcacher_stars_normalization`, `json_typed_capture_folds_into_a_keyed_int_total`, `zombie_scope_negative_case_a2b` |
| D (4) | `switch_as_keyed_replace`, `merge_policy`, `exhaust_policy`, `concat_program_queue` |

(`log_retraction_rejected` is the pre-existing `rejection` / `no_oracle_final`
row, unchanged and counted in neither total.)

**(c) direct, post-fix** — the committed state:

```
SWEEP total=308 compiled=211 unsupported=97 crash=0
RUN total=211 identical=210 wrong=0 emitted_crash=0 rejection=1 no_oracle_log=0
FINAL total=211 final_identical=210 final_wrong=0 no_oracle_final=1
MANIFEST_REASON_DIFF restated=0 args=0 bucket_moved=0 added=0 removed=0
git status v6/prolog/compile/out: empty       (0 direct bytes moved)
```

**(d) plunit:** `swipl -q -l test/plunit_tests.pl -g run_tests -g halt` from
`v6/prolog/compile` — **446 tests, 0 failed** (baseline 437, +9 here). Wall
1.2s.

**(e) ARCH.pl:** 7 PASS.

New tests, all in unit `interning`, each with a `_at_direct` twin:
`departure_frontier_stays_characters_at_dict`,
`departure_frontier_is_unchanged_at_direct`,
`departure_delta_join_resolves_the_frontier_side`,
`departure_delta_join_leaves_the_stored_column_bare_at_direct`,
`departure_delta_projection_interns_the_head_column`,
`departure_delta_projection_is_a_column_at_direct`,
`departure_placeholder_resolves_in_the_projection`,
`departure_placeholder_is_a_bind_at_direct`,
`an_arrival_placeholder_is_not_resolved_at_dict`.

## 6. Fail-first

`lower.pl` reverted to base, atom at dict, full sweep:

| run | RUN wrong | FINAL wrong |
|---|---|---|
| dict, lower.pl at base (fail-first) | **13** | **17** |
| dict, lower.pl fixed | **8** | **12** |
| direct, lower.pl fixed | **0** | **0** |

The fail-first run is byte-for-byte the base red, all 5 family-A modules back
in the wrong set. Restored, out/ regenerated at direct, `git status` on
`compile/out` empty.

Per-clause receipts (each `trigger_read_mode/3` cut clause deleted in turn, so
departure falls through to the program's mode) are pinned in the plunit
section header; every `_at_direct` twin stayed green through all three.

## 7. Corrections to the referee doc

| claim in flip-referee-red.md | measured |
|---|---|
| "pairwise reads wrong VALUE (9 vs oracle 14)" | no value is wrong. `reading`'s own deltas match the oracle at every tick; the `step` rel is EMPTY. The 9 and the 14 in that diff line are both from `reading`, both correct on both sides. No id ever reached arithmetic |
| "a tick-3 delta empty where oracle has one" (`finalize_over_log_fires_on_retention_prune`) | the first diff is at tick **4**, and it is a NULL payload, not an empty delta: `gone add [[1,null]]` vs `[[1,"a"]]` |
| family A framed as "departed/pre state reads" | `pre/1` is not implicated. `__pre_<rel>` is SQL-filled from the base table and is correct at dict. The family is departure-frontier reads only |

The doc's operative sentence held: "NULL says the id lookup happens where no
intern ever ran". The correction is that the value was never in the id space
to begin with, because the runtime writes that table.

## 8. Noted for families B/C/D, not fixed

| # | note | family |
|---|---|---|
| 1 | `boundary_sql`'s term-render CASE (`json_valid(col) AND json_type(col,'$.fn')='text' ...`) runs over the DECODED text of a text column. A stored text value that IS a term-shaped JSON object leaves the boundary as `fn(args)`, a string `__str` may not hold. This is the departure frontier's one remaining hazard under the fix, it is the same rewrite that produces family D's `route_data(settings)` demand keys, and it is unreachable in the corpus today (no fixture stores a term-shaped object in a text column) | D |
| 2 | `stage_departures` (`1_incremental.ts:1373`) is the only writer of an emitted table that does not go through emitted SQL. Any future column encoding beyond `dict`/`direct` has to be told about it | all |
| 3 | families B and D both need a decision the read side cannot make alone: B's `group_concat`/`json_group_array` ORDER BY at the FINAL snapshot reads ids where the delta path decodes (contract §5.2 rows 2-3), D's demand keys cross the dictionary on one side only | B, D |
