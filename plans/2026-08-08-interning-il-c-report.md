# REPORT-IL-C — family C: struct/json boundary at `intern(dict)`

Lane I-L family C, branch `lane/i-l-struct-json`, base
`7c554d5cadaf29e90e7f3549bd9b45f5880db8cb` verified first action. Scope: the 4
modules on `plans/2026-08-08-flip-referee-red.md`'s family-C row.

## TOC

| § | contents |
|---|---|
| 1 | the 4 reproduced first-diffs |
| 2 | the C/D boundary: one of the 4 is not C's |
| 3 | root cause A, the struct plane's own text columns |
| 4 | root cause B, the snapshot the occurrence machinery re-binds |
| 5 | before/after, the two named modules |
| 6 | gate receipts |
| 7 | fail-first |
| 8 | corrections to the referee doc |
| 9 | noted for family D, not fixed |

## 1. The 4 reproduced first-diffs

Atom flipped to `dict` in the lane worktree, `bash scripts/sweep.sh`.
Reproduced the brief's numbers exactly: **RUN wrong=8, FINAL wrong=8**, the same
8 names in both sets.

| module | RUN first diff (tick 1 unless noted) |
|---|---|
| `struct_nested_value_renders_whole_tree` | `diag_file add [[null]]` vs `[["a.rs"]]`; `place add [[null,{"end":9,"start":3}]]`; `diag`'s rendered tree carries `"file":null` |
| `struct_ghcacher_stars_normalization` | `repo_body add [[null,17]]` vs `[["cli",17]]`; `current_body`'s tree carries `"full_name":null` |
| `json_typed_capture_folds_into_a_keyed_int_total` | `total add [[null,4]]` vs `[["cli",4]]`. `star_event add [["cli",4]]` is IDENTICAL to the oracle on both sides |
| `zombie_scope_negative_case_a2b` | line 2: `detail_view` rel ABSENT from the actual deltas, oracle has `add [["item_a","body_a"]]`. No null anywhere |

The fourth diff has a different shape from the other three: a whole rel missing
rather than a null column. That is the first sign of §2.

## 2. The C/D boundary: one of the 4 is not C's

`zombie_scope_negative_case_a2b` belongs to **family D's mechanism**, not C's,
and is left red.

Measured, not inferred. The tables after the run:

```
__str        {"__id":4,"content":"{\"fn\":\"detail\",\"args\":[\"item_a\"]}"}
live_detail  {"pane_id":1,"target":4}
demanded     {"target":4,"pane_id":1}
detail_view  (empty)
```

The write side is correct: `live_detail`'s rel-term head interns the rendered
object and stores id 4. The READ is wrong:

```sql
-- detail_view's insert, at dict
... FROM "__frontier_demanded" d0, "detail_row" b0
WHERE json_extract(d0."target", '$.fn') = 'detail' ...
```

`d0."target"` is the INTEGER 4. `json_extract(4, '$.fn')` is NULL, the guard
never matches, and the rel stays empty. Contract §5.2 **row 14 is wrong**: it
calls this shape SAFE because "the operand is a `json`-typed column, and `json`
is never interned". The operand here is a `text` column holding a compound
term, which IS interned.

The same missing decode, spelled identically, is the whole of family D:

| module | the guard that reads an id as json |
|---|---|
| `switch_as_keyed_replace` | `json_extract(d0."target", '$.fn') = 'route_data'` |
| `merge_policy` | `json_extract(d0."col1", '$.fn') = 'tab'` |
| `exhaust_policy` | `json_extract(d0."col1", '$.fn') = 'tab'` |
| `concat_program_queue` | `json_extract(d0."col1", '$.fn') = 'tab'` |
| `zombie_scope_negative_case_a2b` | `json_extract(d0."target", '$.fn') = 'detail'` |

All five show the same symptom (one `*_view` rel entirely absent) and all five
need the same §5.3 `value`-demand decode around the rel-term operand. Fixing
one fixes five, and that is family D's lane, not this one. Expected post-fix
count was RUN 4 / FINAL 4; the measured 5 is this module, not a regression.

Family C proper is therefore **3 modules across 2 root causes**, both WRITE-side,
both a runtime plane that writes characters into a column the DDL declares
INTEGER. Neither is what the referee doc read (§8).

## 3. Root cause A — the struct plane's own text columns

Modules: `struct_nested_value_renders_whole_tree`,
`struct_ghcacher_stars_normalization`.

`TextPlane.intern` walks the arrival batch and rewrites the columns
`TEXT_INTERN_PLAN.relColumns[rel]` flags. A declared struct type's target row
is **never in that batch**. The arrival for
`struct_nested_value_renders_whole_tree` is one `diag` row whose column 0 is a
`place` object; `relColumns.diag = [false, true]`, so the door interns
`"unused"` and nothing else. `StructPlane.intern` then synthesizes the `place`
and `span` target rows itself and hands them to `apply_targets`, which is the
arrival applicator, downstream of the door that already ran.

Measured after the run, at dict, pre-fix:

```
__str       {"__id":1,"content":"unused"}          -- "a.rs" was never interned
place       {"__id":1,"file":"a.rs","at":1}        -- into "file" INTEGER NOT NULL
__ref_place __rendered = {"file":null,"at":{...}}  -- decode of a non-id
diag_file   {"file":"a.rs"}                        -- copied on from place
```

Every read is correct and consistent with dict. `__ref_place` decodes
`t."file"` through `__str`; `diag_file`'s insert copies `b0."file"` from
`__ref_place` as an id into an id column. The dictionary simply never saw the
string. Same shape in ghcacher: `repo_body.full_name` holds `"cli"` and
`relColumns.repo_body = [true, false]` was sitting right there unused.

`ITextInternPlan.relColumns` already carries an entry for every relplan with an
interned column, struct-type rels included, so the fix needs no new emitted
field and no new lowering: the plane takes the plan it was already given.

| # | site | change |
|---|---|---|
| 1 | `structPlane.ts:intern` | takes `text_plan?: ITextInternPlan` |
| 2 | `structPlane.ts:intern_one_type` | splits: builds the target rows, runs them through `TextPlane.intern` under the target's own rel name, then hands the interned rows to the new `intern_target_rows/6` |
| 3 | `structPlane.ts:intern_target_rows` | the preflight / insert / lookup, unchanged except that its tuples now carry ids |
| 4 | `types.ts:IStructPlane.intern` | the sixth parameter, absent at direct |
| 5 | `emit_ts.pl:naive_reference_normalize_lines/3`, `incremental_reference_normalize_lines/3` (both were `/2`), new `struct_text_plan_argument/2` | passes `TEXT_INTERN_PLAN` when `HasTextIntern` |

**Why the intern runs before the tuple is encoded, not after.** `conflict_sql`,
`intern_sql` and `lookup_sql` are all fed the SAME `json_each(?)` tuple text and
all three JOIN it against the stored columns (`t."file" = json_extract(i.value,
'$[0]')`). Interning only the applicator's copy would leave the preflight and
the key lookup comparing characters to ids: the lookup would return no row and
`relation reference normalization lost the id` would throw. One substitution,
upstream of `encoded`, keeps all three statements in one plane.

**Ordering.** Contract §6's law is "text intern runs BEFORE struct intern".
This does not move it; it applies the same law one level finer, at each struct
type's own target write, which is a write the outer ordering never covered.
`types` is topologically ordered, so a child type is interned and its dense id
resolved before the parent tuple that names it is built.

**Storage.** Nothing changes shape. `place."file"` was already
`INTEGER NOT NULL` under dict inside a `UNIQUE ("file","at")`, and the plane was
the one writer not honouring it. Declaring it TEXT instead would put a natural
TEXT key back inside a struct dictionary's UNIQUE index, which the surrogate-keys
law forbids and `sqlite-costs` prices at 1.7-2.0x.

## 4. Root cause B — the snapshot the occurrence machinery re-binds

Module: `json_typed_capture_folds_into_a_keyed_int_total`.

`total`'s emitted SQL is correct at dict and reads ids throughout:

```sql
SELECT ?1 AS "repo", ?2 AS "sum" WHERE NOT EXISTS (SELECT 1 FROM "total" n0 WHERE n0."repo" = ?1)
```

The binds are wrong. Traced at tick 1, pre-fix: `args=["cli","4"]`. `total.repo`
ends up holding the characters `cli`, and `__txt_total` decodes them to null.

The occurrence row reached `?1` from `ordered_level_occurrences`, which is
`multiset_diff(before["star_event"], mid["star_event"]).add` over `read_snapshot`
— and `read_snapshot` reads the DECODED view:

```sql
SELECT CASE WHEN json_valid("repo") ... ELSE "repo" END AS "repo", "stars" FROM "__txt_star_event"
```

The tick-log leg needs that decode; `build_deltas` output is what
`TickLogEmitter` prints. The occurrence leg needs the opposite, because its rows
go BACK into statements the dictionary owns. One read was serving two planes.

The plane was genuinely mixed, in both directions, which is why "declare it
direct" (family A's answer for the departure frontier) is not the answer here:

| producer of an `arrival`-kind occurrence row | source | encoding pre-fix |
|---|---|---|
| `ordered_outside_occurrences` | `arrivals`, after `TextPlane.intern` | **ids** |
| `apply_ordered_occurrence`'s `written` rows | arm `project_sql` results | **ids** |
| `read_ordered_carry` | `__frontier_<rel>`, `INTEGER NOT NULL` | ids by DDL, filled from below |
| `ordered_level_occurrences` | `read_snapshot` | **characters** |
| `ordered_carry_additions` -> `stage_ordered_frontiers` | `read_snapshot` diff + `written` | **mixed**, into the INTEGER frontier |

Ids win: they are what the consumers want under §5.3 (`n0."repo" = ?1` is
identity, and the head column is the dict-encoded one), they are what two of the
producers already carry, and they are what the frontier's own DDL declares. So
the occurrence machinery gets a second, stored-plane read and the tick log keeps
the decoded one.

| # | site | change |
|---|---|---|
| 1 | `lower.pl:delta_statement/3` | `deltastmt/4` -> `deltastmt/5`, the new field a bare `SELECT <cols> FROM "<table>"`: base table, no `__txt_` view, no `canonical_column_expr` term render |
| 2 | `emit_ts.pl:read_stored_snapshot_fn_lines/4` | emits `type Snapshots`, `read_stored_snapshot/1` and `read_snapshots/1`; `[]` when `HasTextIntern` is false |
| 3 | `emit_ts.pl` `tick_head_read_line/2`, `tick_decoded_before/2`, `tick_stored_before/2`, `ordered_mid_read_line/2`, `ordered_after_read_lines/2` | one two-clause predicate per varying chain position, `false` reproducing the previous text byte for byte |
| 4 | `emit_ts.pl:edge_resolve_call_exprs/3` (was `/2`) | the naive door's resolver takes the same plane |
| 5 | `emit_ts.pl:run_naive_tick_fn_lines`, `run_ordered_tick_fn_lines` | thread the two snapshots |

**Why `ordered_carry_additions` needed the boundary in the stored plane too.**
It keeps only rows that are boundary-visible additions, testing
`boundary_adds.get(rel)?.has(JSON.stringify(row))`. With the diff moved to ids
and the boundary left decoded, every membership test would answer false and the
carry would go empty. `build_deltas` is a pure function of two snapshots, so it
is called twice: `deltas` (decoded, for the tick log) and `stored_deltas` (for
the filter). Same function, no second implementation.

**Cost, stated rather than hidden.** The reference path's per-tick full reads go
from 3 to 5 (ordered) and 2 to 3 (naive with edges), at dict only. This is the
non-incremental referee path; `host_residency` ("zero full-table reads into JS
per tick") is a property of `run_incremental_tick`, which never calls
`read_snapshot` and is untouched here. The full 308-fixture sweep stayed inside
its usual wall time.

**A latent defect this also closes.** `trigger_occurrences(kind, rel,
before[rel], arrivals)` builds its `seen` set from the snapshot and tests
`arrivals` rows against it. Pre-fix at dict that compared characters to ids, so
an arrival duplicating a standing row was never skipped. No corpus fixture is
red from it today; it is correct now for the same reason.

## 5. Before/after, the two named modules

### `struct_nested_value_renders_whole_tree`

The emitted call, at dict:

```ts
// before
StructPlane.intern(seam, STRUCT_TYPES, STRUCT_REF_COLUMNS, arrivals,
  (targets) => apply_arrivals(seam, targets),
)
// after
StructPlane.intern(seam, STRUCT_TYPES, STRUCT_REF_COLUMNS, arrivals,
  (targets) => apply_arrivals(seam, targets), TEXT_INTERN_PLAN,
)
```

No emitted SQL changed. What changed is the tuple those three statements are
fed. Before, `place`'s tuple was `[["a.rs", 1]]`, so:

```sql
INSERT OR IGNORE INTO "place" ("file", "at") SELECT json_extract(value, '$[0]'), ... FROM json_each('[["a.rs",1]]')
-- stores the characters a.rs in "file" INTEGER NOT NULL (affinity keeps it TEXT)
```

After, the same statement is fed `[[2, 1]]` (`__str` gains
`{"__id":2,"content":"a.rs"}` first), so `place."file"` holds 2,
`__ref_place.__rendered` decodes to `{"file":"a.rs","at":{...}}`, and
`diag_file` copies the id on. At `intern(direct)` the emitted text is
byte-identical to base and the plane takes no plan.

### `json_typed_capture_folds_into_a_keyed_int_total`

`EDGE_TOTAL_0_PROJECT_SQL` is unchanged at both modes:

```sql
SELECT ?1 AS "repo", ?2 AS "sum" WHERE NOT EXISTS (SELECT 1 FROM "total" n0 WHERE n0."repo" = ?1)
```

New at dict, the read that feeds `?1`:

```ts
function read_stored_snapshot(seam: ISqlSeam): Observable<Snapshot> {
  return forkJoin({
    event: select_rows(seam, `SELECT "payload" FROM "event"`, ...),
    star_event: select_rows(seam, `SELECT "repo", "stars" FROM "star_event"`, ...),
    total: select_rows(seam, `SELECT "repo", "sum" FROM "total"`, ...),
  });
}
```

against `read_snapshot`'s existing
`SELECT CASE WHEN json_valid("repo") ... END AS "repo", "stars" FROM "__txt_star_event"`.

The ordered chain, at dict:

```ts
// before
  return read_snapshot(seam).pipe(
    ...
    concatMap((before) => read_snapshot(seam).pipe(map((mid) => ({ before, mid })))),
    concatMap(({ before, mid }) => process_ordered_occurrences(seam, before, mid, arrivals)...
    concatMap(({ before, mid, written }) => read_snapshot(seam).pipe(map((after) => ({ mid, after, written, deltas: build_deltas(before, after) })))),
    concatMap(({ mid, after, written, deltas }) => stage_ordered_frontiers(seam, INCREMENTAL_RELATIONS, ordered_carry_additions(mid, after, deltas, written))...

// after
  return read_snapshots(seam).pipe(
    ...
    concatMap((before) => read_stored_snapshot(seam).pipe(map((mid) => ({ before, mid })))),
    concatMap(({ before, mid }) => process_ordered_occurrences(seam, before.stored, mid, arrivals)...
    concatMap(({ before, mid, written }) => read_snapshots(seam).pipe(map((after) => ({ mid, after, written, deltas: build_deltas(before.decoded, after.decoded), stored_deltas: build_deltas(before.stored, after.stored) })))),
    concatMap(({ mid, after, written, deltas, stored_deltas }) => stage_ordered_frontiers(seam, INCREMENTAL_RELATIONS, ordered_carry_additions(mid, after.stored, stored_deltas, written))...
```

Bind at tick 1 moves from `args=["cli","4"]` to `args=[1,4]`, and `total` stores
`{"repo":1,"sum":4}`. At `intern(direct)` every one of those lines is the
`false` clause of its predicate and the text is byte-identical to base.

## 6. Gate receipts, verbatim

**(a) dict, pre-fix** — the reproduction:

```
SWEEP total=308 compiled=211 unsupported=97 crash=0
RUN total=211 identical=202 wrong=8 emitted_crash=0 rejection=1 no_oracle_log=0
FINAL total=211 final_identical=202 final_wrong=8 no_oracle_final=1
MANIFEST_REASON_DIFF restated=0 args=0 bucket_moved=0 added=0 removed=0
```

**(b) dict, root cause A only** (the struct plane, before the snapshot fix):

```
RUN total=211 identical=204 wrong=6 emitted_crash=0 rejection=1 no_oracle_log=0
FINAL total=211 final_identical=204 final_wrong=6 no_oracle_final=1
```

**(c) dict, post-fix:**

```
SWEEP total=308 compiled=211 unsupported=97 crash=0
RUN total=211 identical=205 wrong=5 emitted_crash=0 rejection=1 no_oracle_log=0
FINAL total=211 final_identical=205 final_wrong=5 no_oracle_final=1
MANIFEST_REASON_DIFF restated=0 args=0 bucket_moved=0 added=0 removed=0
```

The remaining 5 are family D's four plus `zombie_scope_negative_case_a2b`, all
five sharing the §2 mechanism, the same 5 names in both sets, no additions and
nothing outside the referee doc's 17:

| still wrong | why |
|---|---|
| `switch_as_keyed_replace`, `merge_policy`, `exhaust_policy`, `concat_program_queue`, `zombie_scope_negative_case_a2b` | family D: `json_extract(<interned id column>, '$.fn')` |

(`log_retraction_rejected` is the pre-existing `rejection` / `no_oracle_final`
row, unchanged and counted in neither total.)

**(d) direct, post-fix** — the committed state:

```
SWEEP total=308 compiled=211 unsupported=97 crash=0
RUN total=211 identical=210 wrong=0 emitted_crash=0 rejection=1 no_oracle_log=0
FINAL total=211 final_identical=210 final_wrong=0 no_oracle_final=1
MANIFEST_REASON_DIFF restated=0 args=0 bucket_moved=0 added=0 removed=0
git status v6/prolog/compile/out: empty       (0 direct bytes moved)
```

**(e) plunit:** `swipl -q -l v6/prolog/compile/test/plunit_tests.pl -g run_tests
-g halt` — **462 tests, 0 failed**, exit 0 (baseline 453, +9 here).

New tests, all in unit `interning`:
`a_struct_target_row_crosses_the_ingest_plan_at_dict`,
`a_struct_target_row_takes_no_ingest_plan_at_direct`,
`the_stored_snapshot_reads_the_table_not_the_view`,
`the_stored_select_carries_no_decode`,
`the_occurrence_plane_reads_the_stored_snapshot`,
`the_tick_log_still_reads_the_decoded_snapshot`,
`there_is_no_stored_snapshot_at_direct`,
`an_edge_resolver_reads_the_stored_snapshot_at_dict`,
`an_edge_resolver_reads_the_one_snapshot_at_direct`.

Four existing tests pattern-matched `deltastmt/4` and were updated to `/5`
(`switch_as_keyed_replace_delta_sql_open_scope`,
`switch_as_keyed_replace_delta_sql_route_change_log`,
`boundary_reads_go_through_the_view`, `boundary_reads_name_the_table_at_direct`),
plus the one in `test/run_sql_check.pl`. No assertion was weakened; only the
arity moved.

**(f) ARCH.pl:** 7 PASS.

**(g) tsv2 package:** `pnpm typecheck` exit 0; `pnpm test` — tests 188,
pass 187, fail 0, skipped 1, 6.9s. Identical to baseline.

## 7. Fail-first

All five changed sources stashed back to base, atom at dict, full sweep:

| run | RUN wrong | FINAL wrong |
|---|---|---|
| dict, sources at base (fail-first) | **8** | **8** |
| dict, struct fix only | **6** | **6** |
| dict, both fixes | **5** | **5** |
| direct, both fixes | **0** | **0** |

The fail-first wrong set is byte-for-byte the base red, all 3 family-C modules
back in it and `zombie_scope_negative_case_a2b` red throughout (it never left).
Restored, `out/` regenerated at direct, `git status` on `compile/out` empty.

## 8. Corrections to `flip-referee-red.md`

| claim in the referee doc | measured |
|---|---|
| "a text member inside a struct/json rendering resolves through `__str` and misses (value never interned on that path)" | the FIRST half is right for the two struct modules and the reason is exact: `"a.rs"` is not in `__str` at all. The doc frames it as a RENDER path; it is a WRITE. `__ref_place`'s decode is correct SQL over a column whose writer skipped the door |
| "or the renderer emits the raw id slot as null" | no renderer emits a raw id slot. Every read in all three modules is correct and dict-consistent. Three writers were not |
| family C = "struct/json boundary renders null" | two mechanisms, not one, and they share nothing but the family label: the struct plane's synthesized target rows (2 modules), and the ordered/naive occurrence plane's snapshot binds (1 module). The second has no struct and no rendering in it at all |
| `json_typed_capture_folds_into_a_keyed_int_total` grouped with the struct modules | its `star_event` write is CORRECT at dict and the emitted SQL for `total` is correct too. The defect is a JS-held row crossing back into a `?` bind from the wrong plane |
| I-L-B's note 2, "if the C lane finds `head_column_expr/6` is being handed an already-`dict` encoding for a `json_extract` result, the fix rhymes with this one" | it does not. `head_column_expr/6` is correct and the json decode is already interned on write (`INSERT OR IGNORE INTO "__str" ... SELECT DISTINCT json_extract(b0."payload", '$."repo"')` is in the emitted module at base). B's instinct that it was write-side was right; the writer was the runtime, not the lowering |
| `zombie_scope_negative_case_a2b` listed under C | family D's mechanism, verbatim: `json_extract(d0."target", '$.fn') = 'detail'` over an interned id. §2 |

The doc's assignment of the two struct modules was right. Its mechanism was
read-side in all four rows and all three defects were write-side, which is now
the third lane in a row to report that (A: a runtime-filled table declared
`dict`; B: a built string with no intern-on-write; C: two runtime-filled planes).

## 9. Noted for family D, not fixed

| # | note |
|---|---|
| 1 | Contract §5.2 **row 14 is wrong** and should be restated when D lands. `where_text(pair_lit(Left, Functor))` is called SAFE on the grounds that the operand is `json`-typed; the five red modules pass a `text` column holding a compound term, which IS interned, and `json_extract` over its id answers NULL. This is one decode at one site and it closes all five |
| 2 | I-L-A's note 1 predicted exactly this from the other end (`boundary_sql`'s term-render CASE running over decoded text) and called it unreachable in the corpus. It is reachable, from the read side, and these five are it |
| 3 | The rel-term round trip has two spellings that must agree: `canonical_column_expr`'s render (`detail(item_a)`) and the stored `__str` content (`{"fn":"detail","args":["item_a"]}`). Whatever D does to the guard has to keep the boundary render reading the same one. `live_detail`'s write already interns the JSON object spelling, so the dictionary holds the object, not the rendering |
| 4 | `canonical_column_expr(Column, ref(TypeName), _)` renders a struct column's VALUE in the decoded snapshot while the table stores its `__id`. `read_stored_snapshot` now hands the occurrence machinery the id for such a column. No corpus fixture binds a ref column as an occurrence trigger, so this is untested either way and worth a fixture when someone touches it |
| 5 | `ordered_carry_additions` and `trigger_occurrences` are the only two places left where a JS-held row from one plane is compared against a row from another. Both now take the stored plane. If a third such comparison is added, it needs the same decision made explicitly |
