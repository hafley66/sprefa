# REPORT-IL-B — family B: ordered aggregates at `intern(dict)`

Lane I-L family B, branch `lane/i-l-final-aggregates`, base
`9705a424768eca90d516a18d3acb348263520eed` verified first action. Scope: the 4
ordered-aggregate modules on `plans/2026-08-08-flip-referee-red.md`'s family-B
row.

## TOC

| § | contents |
|---|---|
| 1 | the 4 reproduced first-diffs |
| 2 | root cause, one mechanism |
| 3 | the fix |
| 4 | before/after SQL, `ordered_group_concat_value` |
| 5 | gate receipts |
| 6 | fail-first |
| 7 | corrections to the referee doc |
| 8 | noted for families C/D, not fixed |

## 1. The 4 reproduced first-diffs

Atom flipped to `dict` in the lane worktree, `bash scripts/sweep.sh`.
Reproduced the brief's numbers exactly: **RUN wrong=8, FINAL wrong=12**, all 4
family-B modules FINAL-only (RUN identical on each).

| module | FINAL first diff |
|---|---|
| `ordered_group_concat_value` | actual `value_joined [["north","null"]]` vs oracle `[["north","apple > orange > pear"]]` |
| `ordered_group_concat_ordinal` | actual `ordinal_joined [["north","null"]]` vs oracle `[["north","pear > orange > apple"]]` |
| `ordered_mermaid_line_assembly` | actual `mermaid_text [["chart","null"]]` vs oracle `[["chart","a\n  b"]]` |
| `ordered_fragment_line_assembly` | actual `fragment_text [["openapi","null"]]` vs oracle `[["openapi","openapi: 3.1\n  paths"]]` |

In every one the GROUP KEY (`north`, `chart`, `openapi`) decodes correctly and
only the aggregated column is null. The source rel's own rows are identical to
the oracle on both sides.

## 2. Root cause, one mechanism

`group_concat` builds a string the dictionary has never seen, and the aggregate
head path was the ONE writer in `lower.pl` with no intern-on-write.

`level_insert_sql/6` (`lower.pl:3753`) forks on `aggregate_head_template/2`. The
plain arm calls `head_select_list/7`, whose `head_column_expr/6` (`:932-935`)
wraps a `direct`-encoded expression in `interned_id_sql/2` and hands the raw
text back as a BuiltValue, which `intern_write_statements/4` (`:939`) turns into
the `INSERT OR IGNORE INTO "__str"` that must run first. The aggregate arm
called `aggregate_select_statement/7` and set `InternSqls = []` unconditionally.

So at dict, for `value_joined(Group, group_concat(Value, " > ")) <- item(Group, Value)`:

| step | what happened |
|---|---|
| head DDL | `"col2" INTEGER NOT NULL` — `col2` is declared `text`, and `text` is the interned type |
| the SELECT | `group_concat((SELECT s."content" ... ), ' > ' ORDER BY (SELECT s."content" ... ))` — `compile_expr(Mode, value, ...)` DOES decode each element, so the concatenation itself is correct: `apple > orange > pear` |
| the INSERT | those characters go into an INTEGER-declared column. SQLite's INTEGER affinity leaves a non-numeric string as TEXT, stored silently |
| the render | `__txt_value_joined` decodes `(SELECT s."content" FROM "__str" s WHERE s."__id" = t."col2")`; `'apple > orange > pear'` is no `__id`, so the lookup answers NULL |

Contract §5.2 row 13 names this class ("computed text projected into a head
column"), and rev 3 (§1780 row 2) deletes the automatic `direct` fallback in
favour of interning on write. The plain head path already implements that. The
aggregate head path never got it.

Not the class family A found. The frontier there was a table declared with the
wrong storage type; here every table is declared correctly and one WRITER skips
the dictionary. It is also not what the referee doc read (see §7).

Three writers of the same head, all three affected:

| writer | site | lands in |
|---|---|---|
| recompute | `level_insert_sql/6` (`lower.pl:3753`) | `boot[]` and `recompute_sql` |
| scoped re-derive | `aggregate_insert_scoped_sql/7` (`lower.pl:2751`) | `aggregate_sql.insert_scoped_sql` |
| — | `avgsql` is float-headed and interns nothing | n/a |

RUN stayed identical only because these four fixtures seed every row in
`Initial` and tick nothing into the aggregated rel; the delta path and the final
path both read the same broken column, and only the final snapshot has a row to
show for it.

## 3. The fix

`lower.pl` (the lowering), `emit_ts.pl` (one optional field), and the aggregate
seam in the tsv2 runtime. No new statement family: the built string rides the
same intern-on-write machinery I-C built for plain heads.

| # | clause | change |
|---|---|---|
| 1 | `aggregate_select_expr/5` (`:4154-4193`, was `/4`) | returns an Encoding. Every aggregate function answers with the characters it computed, so all of them are `direct`; `plain(Expr)` passes `compile_expr/7`'s own encoding through |
| 2 | `aggregate_select_exprs/7` (`:4166`, was `/5`) | takes the head's column types and marks each position `built` (head column is `dict`, expression is `direct`) or `stored` |
| 3 | `aggregate_select_statement/9` (`:4103`, was `/7`) | returns `InternSqls`, and switches shape when anything is `built` |
| 4 | `aggregate_intern_statements/5` (`:4144`) | the `__str` write, one UNION arm per built value |
| 5 | `aggregate_encoded_statement/6` (`:4130`) | the id lookup, one level out |
| 6 | `level_insert_sql/6` (`:3753`) | passes `HeadColumnTypes`, takes `InternSqls` back instead of `[]` |
| 7 | `aggregate_insert_scoped_sql/7` (`:2751`) | reads `relplan_column_types/3`, answers `InsertScopedSql-InternSqls` |
| 8 | `level_aggregate_sql/5` (`:2367`), `aggsql/7` (was `/6`) | carries the collected intern statements; `emit_ts.pl:1371` renders them through the existing `intern_sql_field/2`, so the field is ABSENT at direct |
| 9 | `IAggregateLevelPlan.intern_sql?` (`types.ts:159`), `apply_aggregate_level_statement` (`1_incremental.ts:421`) | the statements join the scope batch |

**Why the id lookup runs one level out.** The first shape tried was
`head_column_expr/6`'s exact spelling, `interned_id_sql(group_concat(...))`.
SQLite rejects it: `SQLITE_ERROR: misuse of aggregate function group_concat()`
— an aggregate may not appear inside a scalar subquery of the query that
aggregates. Measured on all 4 modules (sweep `emitted_crash=4`). So the grouped
select is aliased into a subquery and the lookups run over its rows:

```sql
SELECT "__agg_1", (SELECT s."__id" FROM "__str" s WHERE s."content" = "__agg_2")
FROM (SELECT <group key> AS "__agg_1", group_concat(...) AS "__agg_2" FROM ... GROUP BY ... HAVING count(*) > 0)
```

**Why the intern arm repeats the GROUP BY.** `intern_write_sql/4`'s
`SELECT DISTINCT <value> FROM <from> WHERE <where>` is row-wise. Over an
aggregate it would concatenate the whole relation into ONE string that no head
row holds, and leave every per-group string out of the dictionary. The arm is
therefore the same grouped select with only the built column in the select list.

**What still reads the id, deliberately.** The group key (`"__agg_1"`) is
returned bare; the `GROUP BY` reads `b0."group"` bare; the scope predicate
`(b0."group") IN (SELECT "group" FROM "__agg_scope_...")` compares ids to ids.
A decode on any of those would be the correct-but-slow regression §5.3 names,
and `the_aggregate_group_key_stays_an_id_at_dict` pins it.

**Ordering, and why the runtime touch is three lines.** The scoped intern arm
reads `__agg_scope_<head>`, so it must follow the seed; the scoped insert reads
`__str`, so it must precede that. `apply_aggregate_level_statement`'s existing
`scope_statements` batch is already an ordered clear-then-seed, so the intern
statements append to it. Absent (`?? []`) on every pre-field module and at
direct.

**Totality.** The intern arm and the insert are the same query with the same
FROM, WHERE and GROUP BY, so every string the insert asks for is a string the
arm just wrote. `__str` is append-only within a run, so a concurrent group
cannot remove one. No `NOT NULL` insert can drop.

**Storage (the `sql-relational-design` question).** Nothing changes shape: the
head column was already `INTEGER` under dict and the aggregate is the only
producer that was not honouring it. The alternative — declaring aggregated text
columns `direct` and leaving them TEXT — would put a natural TEXT key back into
a `WITHOUT ROWID` PRIMARY KEY (`value_joined ("group", "col2")`), which the
surrogate-keys law forbids and `sqlite-costs` prices at 1.7-2.0x. The dictionary
row is written once per distinct concatenation and the head stores 8 bytes.

## 4. Before/after SQL, `ordered_group_concat_value`

Recompute / boot, at dict:

```sql
-- before: characters into an INTEGER column, dictionary never told
INSERT OR IGNORE INTO "value_joined" ("group", "col2")
SELECT b0."group",
       group_concat((SELECT s."content" FROM "__str" s WHERE s."__id" = b0."value"), ' > '
                    ORDER BY (SELECT s."content" FROM "__str" s WHERE s."__id" = b0."value"))
FROM "item" b0 GROUP BY b0."group" HAVING count(*) > 0

-- after, statement one (new)
INSERT OR IGNORE INTO "__str" ("content")
SELECT group_concat((SELECT s."content" FROM "__str" s WHERE s."__id" = b0."value"), ' > '
                    ORDER BY (SELECT s."content" FROM "__str" s WHERE s."__id" = b0."value"))
FROM "item" b0 GROUP BY b0."group" HAVING count(*) > 0

-- after, statement two
INSERT OR IGNORE INTO "value_joined" ("group", "col2")
SELECT "__agg_1", (SELECT s."__id" FROM "__str" s WHERE s."content" = "__agg_2")
FROM (SELECT b0."group" AS "__agg_1",
             group_concat((SELECT s."content" FROM "__str" s WHERE s."__id" = b0."value"), ' > '
                          ORDER BY (SELECT s."content" FROM "__str" s WHERE s."__id" = b0."value")) AS "__agg_2"
      FROM "item" b0 GROUP BY b0."group" HAVING count(*) > 0)
```

Scoped per-tick arm, at dict, same pair with the scope predicate carried into
both:

```sql
-- aggregate_sql.intern_sql[0]  (runs in the scope batch, after the seed)
INSERT OR IGNORE INTO "__str" ("content")
SELECT group_concat((SELECT s."content" FROM "__str" s WHERE s."__id" = b0."value"), ' > '
                    ORDER BY (SELECT s."content" FROM "__str" s WHERE s."__id" = b0."value"))
FROM "item" b0
WHERE (b0."group") IN (SELECT "group" FROM "__agg_scope_value_joined")
GROUP BY b0."group" HAVING count(*) > 0

-- aggregate_sql.insert_scoped_sql[0]
INSERT OR IGNORE INTO "value_joined" ("group", "col2")
SELECT "__agg_1", (SELECT s."__id" FROM "__str" s WHERE s."content" = "__agg_2")
FROM (SELECT b0."group" AS "__agg_1", group_concat(...) AS "__agg_2"
      FROM "item" b0
      WHERE (b0."group") IN (SELECT "group" FROM "__agg_scope_value_joined")
      GROUP BY b0."group" HAVING count(*) > 0)
RETURNING "group", "col2"
```

At `intern(direct)` all of them are byte-identical to base:

```sql
INSERT OR IGNORE INTO "value_joined" ("group", "col2")
SELECT b0."group", group_concat(b0."value", ' > ' ORDER BY b0."value")
FROM "item" b0 GROUP BY b0."group" HAVING count(*) > 0
```

A `json_group_array` head is untouched at both modes: `json` is never interned,
so its position stays `stored`, no alias wrapper appears and `InternSqls` is
empty (`a_json_aggregate_head_owes_the_dictionary_nothing_at_dict`).

## 5. Gate receipts, verbatim

**(a) dict, pre-fix** — the reproduction:

```
SWEEP total=308 compiled=211 unsupported=97 crash=0
RUN total=211 identical=202 wrong=8 emitted_crash=0 rejection=1 no_oracle_log=0
FINAL total=211 final_identical=198 final_wrong=12 no_oracle_final=1
MANIFEST_REASON_DIFF restated=0 args=0 bucket_moved=0 added=0 removed=0
```

**(b) dict, post-fix:**

```
SWEEP total=308 compiled=211 unsupported=97 crash=0
RUN total=211 identical=202 wrong=8 emitted_crash=0 rejection=1 no_oracle_log=0
FINAL total=211 final_identical=202 final_wrong=8 no_oracle_final=1
MANIFEST_REASON_DIFF restated=0 args=0 bucket_moved=0 added=0 removed=0
```

The remaining 8 are exactly families C and D, the same 8 on both sides, no
additions:

| family | modules still wrong (RUN and FINAL alike) |
|---|---|
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

**(d) plunit:** `swipl -q -l v6/prolog/compile/test/plunit_tests.pl -g run_tests
-g halt` — **453 tests, 0 failed**, exit 0 (baseline 446, +7 here).

New tests, all in unit `interning`:
`an_aggregate_text_head_interns_the_group_concat`,
`an_aggregate_text_head_is_a_bare_group_concat_at_direct`,
`the_aggregate_intern_arm_repeats_the_grouping`,
`the_aggregate_group_key_stays_an_id_at_dict`,
`a_json_aggregate_head_owes_the_dictionary_nothing_at_dict`,
`the_scoped_aggregate_insert_carries_its_own_intern`,
`the_scoped_aggregate_insert_has_no_intern_at_direct`.

**(e) ARCH.pl:** 7 PASS.

**(f) tsv2 package** (the runtime seam is touched): `pnpm typecheck` exit 0;
`pnpm test` — tests 188, pass 187, fail 0, skipped 1, 6.6s.

## 6. Fail-first

`lower.pl`, `emit_ts.pl`, `plunit_tests.pl`, `types.ts` and `1_incremental.ts`
stashed back to base, atom at dict, full sweep:

| run | RUN wrong | FINAL wrong |
|---|---|---|
| dict, sources at base (fail-first) | **8** | **12** |
| dict, fixed | **8** | **8** |
| direct, fixed | **0** | **0** |

The fail-first FINAL wrong set is byte-for-byte the base red, all 4 family-B
modules back in it. Restored, `out/` regenerated at direct, `git status` on
`compile/out` empty.

An intermediate receipt worth keeping: with the naive
`interned_id_sql(group_concat(...))` shape the same sweep reported
`emitted_crash=4` on exactly these 4 modules,
`SQLITE_ERROR: misuse of aggregate function group_concat()`. That is why
`aggregate_encoded_statement/6` exists rather than a direct reuse of
`head_column_expr/6`'s spelling.

## 7. Corrections to `flip-referee-red.md`

| claim in the referee doc | measured |
|---|---|
| "`value_joined [["north","null,null,..."]]` — group_concat concatenates NULLs" | it does not. The concatenation is CORRECT at dict — `compile_expr(Mode, value, ...)` already decodes each element, so `group_concat` produces `apple > orange > pear`. The single measured value is `"null"`, one null, and it is the FINAL RENDER of the head column, not the aggregate's output |
| "the final-snapshot render of an ordered aggregate reads ids where the delta log path decodes" | both paths decode identically (`__txt_value_joined` and `__txt___delta_value_joined` are the same expression). Neither read is wrong. The WRITE is wrong: characters entered an INTEGER column with no intern, so both views decode a value the dictionary does not hold |
| "family B = ordered aggregates over decoded text" | ordering is not implicated at all. `group_concat_ordered`'s ORDER BY reads an int ordinal (§5.2 row 4, SAFE) and plain `group_concat`'s ORDER BY reads the already-decoded value. §5.2 rows 2-3 are already fixed. The family is aggregate heads whose RESULT is text, and it would bite an unordered `group_concat` the same way |
| "RUN identical / FINAL wrong" framed as two paths disagreeing | one path. These 4 fixtures seed everything in `Initial` and never tick the aggregated rel, so RUN has no row to be wrong about |
| the brief's expectation "RUN stays wrong=8 or drops" | RUN was already 8 at the reproduction and stayed 8; family B contributes nothing to RUN at any point |

The doc's family assignment was right (the same 4 modules, one mechanism, and
the fix is one change). Its reading of the mechanism was not.

## 8. Noted for families C/D, not fixed

| # | note | family |
|---|---|---|
| 1 | families C and D are now identically wrong on RUN and FINAL — the same 8 names in both sets, so neither is a render-only class and both bite the delta path |
| 2 | `json_typed_capture_folds_into_a_keyed_int_total`'s `total [["null",10],["null",8]]` is the same WRITE-side shape family B had: a plain (non-aggregate) head whose text column receives a value from a json decode. If the C lane finds `head_column_expr/6` is being handed an already-`dict` encoding for a `json_extract` result, the fix rhymes with this one | C |
| 3 | `aggregate_scope_group_exprs/5` compiles the scope seed's group keys against the DELTA table alias `d0`. If a future group key is ever a built string, its scope seed would need the same intern the insert now has, and there is no call site for it yet. Unreachable in the corpus (every group key is a plain column read) | all |
| 4 | `avgsql`'s boot and refresh statements were left alone: the accumulator is REAL/INTEGER and the public head column is `float`, so nothing on that path can build a string. A text-headed `avg` is refused by `compile_aggregate_number_operand/6` | — |
