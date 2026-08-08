# REPORT-IL-D — family D: rel-term demand keys at `intern(dict)`

Lane I-L family D, branch `lane/i-l-demand-keys`, base
`67f2f74cf7b801a0d2b0ad8210cbca0fe67e0035` verified first action. Scope: the 4
modules on `plans/2026-08-08-flip-referee-red.md`'s family-D row plus
`zombie_scope_negative_case_a2b`, reassigned from family C by lane C's §2
measurement. This is the last family; the dict sweep is now wrong=0.

## TOC

| § | contents |
|---|---|
| 1 | the 5 reproduced first-diffs |
| 2 | root cause, one decode, confirmed per module |
| 3 | the fix |
| 4 | before/after SQL, the two named modules |
| 5 | the `boundary_sql` CASE decision: NOT in scope, with the receipt |
| 6 | gate receipts, verbatim |
| 7 | fail-first |
| 8 | the §5.2 row 14 correction |
| 9 | residual risk for the flip retry |

## 1. The 5 reproduced first-diffs

Atom flipped to `dict` in the lane worktree, `bash scripts/sweep.sh`.
Reproduced the brief's numbers exactly: **RUN wrong=5, FINAL wrong=5**, the same
5 names in both sets, nothing outside the referee doc's 17.

| module | RUN first diff |
|---|---|
| `switch_as_keyed_replace` | line 1: `route_view` rel ABSENT; oracle has `add [["settings","body_settings"]]` |
| `merge_policy` | line 1: `tab_view` ABSENT; oracle `add [["tab_a","body_a"],["tab_b","body_b"]]` |
| `exhaust_policy` | line 1: `tab_view` ABSENT; oracle `add [["tab_a","body_a"]]` |
| `concat_program_queue` | line 1: `tab_view` ABSENT; oracle `add [["tab_a","body_a"]]` |
| `zombie_scope_negative_case_a2b` | line 2: `detail_view` ABSENT; oracle `add [["item_a","body_a"]]` |

One shape in all five: a whole `*_view` rel missing, no null anywhere, and every
OTHER rel in the tick identical to the oracle. In particular `demanded` renders
`[["route_data(settings)","session_one"]]` correctly on both sides at dict,
which is §5's receipt.

## 2. Root cause, one decode, confirmed per module

**"One decode closes all five" HELD.** Confirmed per module rather than
inherited: one edit to one clause took the dict sweep from wrong=5 to wrong=0
in a single step, with no second change and no module needing its own arm.

The mechanism, named at the clause: `compile_pattern_arg/8`'s compound branch
(`lower.pl:298-306`) is what lowers a rel-term pattern like `detail(ItemId)`.
It handed the RAW column expression to both json readers:

| # | site | emitted before |
|---|---|---|
| 1 | `where_text(pair_lit(Left, Functor))`, `lower.pl:361-363` | `json_extract(d0."target", '$.fn') = 'detail'` |
| 2 | `compile_sub_args/8`, `lower.pl:345` | `json_extract(d0."target", '$.args[0]')` |

Under dict, `d0."target"` is the INTEGER id. `json_extract(4, '$.fn')` is NULL,
the guard matches nothing, and the rel stays empty with no refusal.

Per module, the exact operand where the id meets `json_extract`:

| module | guard operand | sub-arg operand | head rel lost |
|---|---|---|---|
| `switch_as_keyed_replace` | `json_extract(d0."target", '$.fn') = 'route_data'` | `json_extract(d0."target", '$.args[0]')` | `route_view/2` |
| `zombie_scope_negative_case_a2b` | `json_extract(d0."target", '$.fn') = 'detail'` | `json_extract(d0."target", '$.args[0]')` | `detail_view/2` |
| `merge_policy` | `json_extract(d0."col1", '$.fn') = 'tab'` | `json_extract(d0."col1", '$.args[0]')` | `tab_view/2` |
| `exhaust_policy` | same | same | `tab_view/2` |
| `concat_program_queue` | same | same | `tab_view/2` |

Lane C's reading of the mechanism was accurate, and its reassignment of
`zombie_scope_negative_case_a2b` was correct: that module's guard is the same
clause, same spelling, differing only in the functor.

**What lane C's table did not carry, and it matters.** The sub-argument reader
(`'$.args[N]'`) is the second half of the same defect and it is a WRITE too:
each of the five modules emits an `intern_sql` /`support_intern_sql` statement
of the form

```sql
INSERT OR IGNORE INTO "__str" ("content") SELECT DISTINCT json_extract(d0."target", '$.args[0]') FROM ...
```

which at dict was interning NULL rather than the key. So the guard, the
projection, the join and the dictionary write were all reading the same raw id.
One decode at the branch fixes all four, because all four take the operand from
that one clause.

The write side needed nothing. `live_detail`'s rel-term head interns the
rendered object correctly (`__str` id 4 = `{"fn":"detail","args":["item_a"]}`);
this family is READ-side only, which makes it the first of the four families
that is.

## 3. The fix

`lower.pl` only. One clause, three lines, zero emitter, zero runtime, zero
`types.ts`, zero DDL.

```prolog
    ; compound(Arg)
    -> Arg =.. [Functor | SubArgs],
       % json_extract reads the term's characters, so the operand is `value`
       % demand: over a dict column's id every path answers NULL.
       demanded_sql(value, Encoding, ColumnExpr, TermExpr, _TermEncoding),
       FnCheck = pair_lit(TermExpr, Functor),
       compile_sub_args(Mode, SubArgs, TermExpr, 0, Bound0, Bound, MoreWhere, Binding),
       WhereParts = [FnCheck | MoreWhere]
```

`demanded_sql/5` (`lower.pl:527`) is contract §5.3 rule one's existing
predicate, already in use by concat/norm/regexp/ORDER BY. `Encoding` is the one
`compile_pattern_arg/8` computed for this column two lines up, so the branch
takes the decode when and only when the column is interned.

**Why the decode and not an intern-the-term comparison.** Comparing the whole
column id against an interned literal of the full term JSON would be the
identity read and would need no `__str` probe. It is only available when every
sub-argument is ground. In all five modules the sub-argument is a variable
(`detail(ItemId)`, `tab(TabId)`), so the pattern is a pattern and the guard is
a structural inspection of the term's characters. §5.3 rule one names exactly
that as `value` demand.

**What still reads the id, deliberately.** The stored, indexed side of every
join stays bare. `b0."route_id" = (SELECT s."__id" ... = json_extract(<decode>,
'$.args[0]'))` resolves the CHARACTERS side and leaves the probeable column
alone; a decode there would be the correct-but-slow regression §5.3 refuses, and
`the_rel_term_join_leaves_the_stored_column_bare_at_dict` pins it. The head
projection and the `GROUP BY` in the refcount arm likewise keep ids.

**Storage (the `sql-relational-design` question).** Nothing changes shape. No
DDL, no index, no key. The demand-key column was already `INTEGER` under dict
and stays one; the decode is a read-boundary materialization, which is what that
skill's "human-readable output is a JOIN or view at the read boundary" line
prescribes. The alternative — declaring rel-term text columns `direct` so they
stay TEXT — puts a natural TEXT key back into `demanded`'s btrees, which the
surrogate-keys law forbids and `sqlite-costs` prices at 1.7-2.0x.

**Cost, measured, not hidden.** `EXPLAIN QUERY PLAN` on the decoded
`detail_view` delta arm, sqlite3 with the emitted DDL:

```
QUERY PLAN
|--SCAN d0
|--CORRELATED SCALAR SUBQUERY 3
|  `--SEARCH s USING INTEGER PRIMARY KEY (rowid=?)
|--BLOOM FILTER ON b0 (item_id=?)
|--SEARCH b0 USING AUTOMATIC COVERING INDEX (item_id=?)
|--CORRELATED SCALAR SUBQUERY 5
|  |--SEARCH s USING COVERING INDEX sqlite_autoindex___str_1 (content=?)
|  `--CORRELATED SCALAR SUBQUERY 4
|     `--SEARCH s USING INTEGER PRIMARY KEY (rowid=?)
|--CORRELATED SCALAR SUBQUERY 2
|  |--SEARCH s USING COVERING INDEX sqlite_autoindex___str_1 (content=?)
|  `--CORRELATED SCALAR SUBQUERY 1
|     `--SEARCH s USING INTEGER PRIMARY KEY (rowid=?)
`--USE TEMP B-TREE FOR DISTINCT
```

Every `__str` access is a **SEARCH**, never a SCAN: the decode is an INTEGER
PRIMARY KEY probe and the id lookup is a covering-index probe. The stored
`b0."item_id"` is probed by index and is never decoded. The honest cost line:
SQLite does not CSE identical correlated subqueries, so the decode of
`d0."target"` is evaluated **3 times per d0 row** (subqueries 1, 3 and 4) where
1 would do. All three are integer PK probes on the dictionary, and `d0` is a
demand frontier (this tick's net demand rows), not a base table. §9 row 1
records the hoist as available and unspent.

## 4. Before/after SQL, the two named modules

### `switch_as_keyed_replace`, `route_view` delta arm, at dict

```sql
-- before: json_extract over the INTEGER id; the guard matches no row and
-- route_view stays empty at every tick
INSERT OR IGNORE INTO "route_view" ("route_id", "body")
SELECT DISTINCT (SELECT s."__id" FROM "__str" s WHERE s."content" = json_extract(d0."target", '$.args[0]')), b0."body"
FROM "__frontier_demanded" d0, "route_row" b0
WHERE d0."_phase" >= 0
  AND json_extract(d0."target", '$.fn') = 'route_data'
  AND b0."route_id" = (SELECT s."__id" FROM "__str" s WHERE s."content" = json_extract(d0."target", '$.args[0]'))
UNION ALL ...

-- after: one decode feeds the guard and the sub-argument; the stored
-- b0."route_id" is untouched on its own side
INSERT OR IGNORE INTO "route_view" ("route_id", "body")
SELECT DISTINCT (SELECT s."__id" FROM "__str" s WHERE s."content" = json_extract((SELECT s."content" FROM "__str" s WHERE s."__id" = d0."target"), '$.args[0]')), b0."body"
FROM "__frontier_demanded" d0, "route_row" b0
WHERE d0."_phase" >= 0
  AND json_extract((SELECT s."content" FROM "__str" s WHERE s."__id" = d0."target"), '$.fn') = 'route_data'
  AND b0."route_id" = (SELECT s."__id" FROM "__str" s WHERE s."content" = json_extract((SELECT s."content" FROM "__str" s WHERE s."__id" = d0."target"), '$.args[0]'))
UNION ALL ...
```

The rule's `intern_sql` moves the same way, which is what put the sub-argument
into the dictionary instead of NULL:

```sql
-- before
INSERT OR IGNORE INTO "__str" ("content") SELECT DISTINCT json_extract(d0."target", '$.args[0]') FROM "__frontier_demanded" d0, "route_row" b0 WHERE ...
-- after
INSERT OR IGNORE INTO "__str" ("content") SELECT DISTINCT json_extract((SELECT s."content" FROM "__str" s WHERE s."__id" = d0."target"), '$.args[0]') FROM "__frontier_demanded" d0, "route_row" b0 WHERE ...
```

### `zombie_scope_negative_case_a2b`, `detail_view` delta arm, at dict

```sql
-- before
... FROM "__frontier_demanded" d0, "detail_row" b0
WHERE d0."_phase" >= 0 AND json_extract(d0."target", '$.fn') = 'detail'
  AND b0."item_id" = (SELECT s."__id" FROM "__str" s WHERE s."content" = json_extract(d0."target", '$.args[0]'))

-- after
... FROM "__frontier_demanded" d0, "detail_row" b0
WHERE d0."_phase" >= 0
  AND json_extract((SELECT s."content" FROM "__str" s WHERE s."__id" = d0."target"), '$.fn') = 'detail'
  AND b0."item_id" = (SELECT s."__id" FROM "__str" s WHERE s."content" = json_extract((SELECT s."content" FROM "__str" s WHERE s."__id" = d0."target"), '$.args[0]'))
```

Lane C measured `__str` id 4 = `{"fn":"detail","args":["item_a"]}` and
`demanded.target = 4`. After the fix the decode returns that object text,
`json_extract(..., '$.fn')` returns `detail`, the guard matches, and
`detail_view` fires at tick 2 as the oracle has it.

At `intern(direct)` every one of these texts is byte-identical to base:
`git status v6/prolog/compile/out` is empty after regenerating (§6d).

## 5. The `boundary_sql` term-render CASE: NOT in scope

Lane A's note 1 flagged `boundary_sql`'s term-render CASE (`lower.pl:4663`) as
family D's, on the theory that "a text column storing a term-shaped JSON object
would leave the boundary as `fn(args)`, a string `__str` may not hold".

**Decision: out of scope, and the CASE is correct as written.** Two reasons,
both measured rather than read:

1. **The CASE never sees an id.** It is emitted over `__txt_<table>` /
   `__txt___delta_<table>`, the decode view, which has already resolved every
   interned column to its characters. The rendering runs on text under both
   modes. Cited from the emitted module at dict:
   `... ELSE "target" END AS "target" FROM "__txt___delta_demanded"`.
2. **The rendering is the oracle's own spelling.** The pre-fix red run showed
   `demanded add [["route_data(settings)","session_one"]]` matching the oracle
   EXACTLY at dict, in every one of the five modules, while the `*_view` rel was
   missing. If the boundary render were implicated, that row would have been the
   first diff. It never was, in any of the five.

Lane A's worry that `fn(args)` is "a string `__str` may not hold" is true and
harmless: the boundary render is a read-only projection into the tick log and
nothing writes it back into the dictionary. `__str` holds the JSON object
spelling (lane C's note 3), the boundary emits the `fn(args)` rendering, and the
two are never compared. Nothing in this lane touched either.

## 6. Gate receipts, verbatim

**(a) dict, pre-fix** — the reproduction:

```
SWEEP total=308 compiled=211 unsupported=97 crash=0
RUN total=211 identical=205 wrong=5 emitted_crash=0 rejection=1 no_oracle_log=0
FINAL total=211 final_identical=205 final_wrong=5 no_oracle_final=1
MANIFEST_REASON_DIFF restated=0 args=0 bucket_moved=0 added=0 removed=0
```

**(b) dict, post-fix — the first all-green dict sweep:**

```
SWEEP total=308 compiled=211 unsupported=97 crash=0
RUN total=211 identical=210 wrong=0 emitted_crash=0 rejection=1 no_oracle_log=0
FINAL total=211 final_identical=210 final_wrong=0 no_oracle_final=1
MANIFEST_REASON_DIFF restated=0 args=0 bucket_moved=0 added=0 removed=0
```

Zero wrong on both sides, zero emitted crashes, zero manifest movement.
(`log_retraction_rejected` is the pre-existing `rejection` / `no_oracle_final`
row, unchanged and counted in neither total.)

**(c) direct, post-fix** — the committed state, atom back at
`default_intern_mode(direct)`:

```
SWEEP total=308 compiled=211 unsupported=97 crash=0
RUN total=211 identical=210 wrong=0 emitted_crash=0 rejection=1 no_oracle_log=0
FINAL total=211 final_identical=210 final_wrong=0 no_oracle_final=1
MANIFEST_REASON_DIFF restated=0 args=0 bucket_moved=0 added=0 removed=0
```

**(d) direct emitted bytes:** `git status v6/prolog/compile/out` **empty** after
regenerating at direct. **0 direct bytes moved.** The decode is behind
`demanded_sql/5`'s `dict` clause, so at direct the branch is the identity it was.

**(e) plunit:** `swipl -q -l v6/prolog/compile/test/plunit_tests.pl -g run_tests
-g halt` — **470 tests, 0 failed**, exit 0 (baseline 462, +8 here). Wall 1.3s.

New tests, all in unit `interning`:
`a_rel_term_guard_decodes_the_demand_key_at_dict`,
`a_rel_term_guard_reads_the_column_at_direct`,
`a_rel_term_sub_argument_decodes_its_parent_at_dict`,
`the_rel_term_join_leaves_the_stored_column_bare_at_dict`,
`a_rel_term_intern_arm_decodes_the_demand_key_at_dict`,
`the_zombie_scope_demand_key_decodes_at_dict`,
`the_zombie_scope_demand_key_is_a_column_at_direct`,
`the_rel_term_recompute_arm_decodes_the_demand_key_at_dict`.

No existing test changed. No arity moved.

**(f) ARCH.pl:** `cd v6/prolog && swipl -g go -t halt ARCH.pl` — **7 PASS**.

**(g) tsv2 package:** `pnpm test` — tests 188, pass 187, fail 0, skipped 1,
6.7s. Identical to baseline. No TypeScript touched, so no typecheck delta.

## 7. Fail-first

`lower.pl` reverted to base (tests kept), atom at dict, full sweep:

| run | RUN wrong | FINAL wrong |
|---|---|---|
| dict, `lower.pl` at base (fail-first) | **5** | **5** |
| dict, fixed | **0** | **0** |
| direct, fixed | **0** | **0** |

The fail-first wrong set is byte-for-byte the base red: the same 5 modules, the
same first-diff lines, each still missing its `*_view` rel. Restored, `out/`
regenerated at direct, `git status` on `compile/out` empty.

Plunit fail-first, same reverted `lower.pl`: **5 of the 8 new tests FAILED**,
exit 1 —
`a_rel_term_guard_decodes_the_demand_key_at_dict`,
`a_rel_term_sub_argument_decodes_its_parent_at_dict`,
`a_rel_term_intern_arm_decodes_the_demand_key_at_dict`,
`the_zombie_scope_demand_key_decodes_at_dict`,
`the_rel_term_recompute_arm_decodes_the_demand_key_at_dict`.

The 3 that stayed green are green BY DESIGN and are named so no reader mistakes
them for fail-first receipts: the two `_at_direct` twins pin the no-op (they
must be green on both sides), and
`the_rel_term_join_leaves_the_stored_column_bare_at_dict` is the
correct-but-slow regression pin, which was already satisfied before the fix and
exists to stay satisfied after it.

## 8. The §5.2 row 14 correction

`plans/2026-08-08-interning-contract.md` §5.2 row 14 is corrected in this
commit, marked rev 3.2 / lane-D. It read:

> **SAFE.** The operand is a `json`-typed column, and `json` is never interned
> (§3.1)

It now reads **BREAKS**, carrying the measurement: a rel-term demand key is a
`text` column holding a compound term, `text` IS the interned type, so
`json_extract` gets an INTEGER id and every path answers NULL; 5 modules, each
losing its whole `*_view` rel with no refusal. The row's site column now names
both readers (`where_text/2` at `:360-366` AND `compile_sub_args/8` at
`:343-349`) and the clause that feeds them (`compile_pattern_arg/8`, `:298-306`),
because the original row named only the guard and the sub-argument reader is
half the defect.

Two edits follow from it in the same file: §5.3's heading now reads "rows 2, 3,
5, 6, 7, 11, 12, 14", and §5.3's exhaustive caller list gains
`compile_pattern_arg/8`'s compound branch with the note that it decodes ONCE and
hands the characters to the guard and every sub-argument.

The `json`-typed half of the original claim survives and is stated explicitly:
`column_encoding/3` answers `direct` for `json` under BOTH modes, so a genuinely
json-typed operand still takes no decode. Lane A's `ref(Type)` case is the same
(`lower.pl:1474-1481` records an older incident of this exact NULL shape over a
struct endpoint, fixed there by rewriting to `__ref_<type>` atoms); `ref` is
also `direct`, so this change does not touch it.

## 9. Residual risk for the flip retry

| # | risk |
|---|---|
| 1 | **The decode is evaluated 3x per frontier row** where 1 would do (§3's EXPLAIN: correlated subqueries 1, 3, 4 all decode `d0."target"`). SQLite does not CSE identical correlated subqueries. Every probe is an INTEGER PK search on `__str` and the row source is a per-tick demand frontier, so it is not a defect today. The hoist (alias the decode once in a subselect, the shape lane B used for aggregates) is available and unspent; it would move emitted bytes at dict only |
| 2 | The full 308-fixture sweep is the only gate that has ever caught a family here, and it caught all four. Every lane's plunit was green on the fixture that was red. The flip retry should treat the sweep, not plunit, as the verdict |
| 3 | No fixture in the corpus exercises a compound pattern over a **`json`-typed** column, so the "json takes no decode" half of row 14 is argued from `column_encoding/3` (one clause, `interned_column(dict, text)`) and pinned only by the `_at_direct` twins, which take the identical code path. A fixture would be worth adding the day a json-typed rel-term pattern appears |
| 4 | A **fully ground** rel-term pattern (`detail(item_a)` with no variable) takes the same decode as a pattern one, where an interned-literal identity compare would be cheaper and index-friendly. No corpus fixture is ground, so this is unmeasured in both directions |
| 5 | Four families, four different planes, and all four were invisible to a green compile. `unexplained=0` and `crash=0` held through every red run in this arc. Nothing in the compiler's own diagnostics distinguishes "correct at dict" from "silently empty at dict" |
| 6 | The flip itself (`compile.pl:153` `direct` -> `dict`) is NOT in this commit. The tree ships at `direct`, byte-identical emitted output, and the coordinator owns pulling the atom |
