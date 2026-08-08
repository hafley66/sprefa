# REPORT-IK — built strings at `intern(dict)` (pass 1)

Lane I-K (opus), branch `lane/i-k-built-strings`, base `5c8d9784` verified
first action. Commit `afb3593a`. Pass 1 of 2. Banked by the coordinator; the
lane's harness blocked writing this file itself. Coordinator re-ran sweep,
plunit, tsgo, intern-ab in the worktree: identical numbers.

## 1. What landed, per scope item

| # | scope item | landed |
|---|---|---|
| 1 | rule ONE's other half (I-C deviation 2 / residual 1) | `Bound` is `typed(Sql, Type, Encoding)`. `compile_expr/7` returns the result's encoding; `demanded_sql/5` (`lower.pl:521`) decodes a `dict` column under `value` demand. §5.2 rows 2, 3, 5, 6, 7 now read characters |
| 2 | intern on write | `head_column_expr/6` (`:926`): a head position whose expression came out `direct` into a column whose encoding is `dict` is wrapped in `interned_id_sql/2` and reported as a built value. `intern_write_sql/4` (`:939`) builds §5.7.1's statement one from the arm's own FROM/WHERE |
| 3 | placement | level recompute only. `level_insert_statements/5` (`:3710`) returns `[InternSql, InsertSql]`; `level_statement_group/3` appends the groups. Zero emitter change, zero runtime change: the list is already `;`-joined and run through `executeMultiple` |
| 3b | delta / refCount / edge | intern statement NOT placed. §5 states why and prices the three exits |
| 4 | mixed encodings | `aligned_pair/6` (`:304`): a join or comparison whose sides carry different encodings resolves the characters side to an id. Also closes an I-C hole in `compile_sub_args/8` (a literal compared against a destructured `json_extract` was being resolved as an id) |
| 5 | G9 classifier | three new classes `built-id`, `intern-write`, `value-decode`; `unexplained=0` |
| 5b | plunit | 10 new tests, dict+direct pairs, fail-first receipts taken |
| 6 | direct bytes | `git status` on `compile/out` empty after a full sweep: 0 bytes move |

Measured:

| quantity | value |
|---|---|
| modules projecting a built string into an interned head text column | 41 (contract §5.7.4 said 17; that count was `concat` only) |
| id lookups over a built expression (`built-id`) | 437 |
| intern statements emitted (`intern-write`) | 141 |
| text columns decoded under `value` demand (`value-decode`) | 327 |
| emitted bytes moving at `intern(direct)` | 0 |

## 2. The corpus enumeration

Taken by instrumenting `head_column_expr/6` and sweeping at `intern(dict)`;
instrumentation removed before the commit.

| statement family | site | modules |
|---|---|---|
| level recompute | `level_insert_sql/6`, `lower.pl:3714` | 39 |
| level refCount arm | `level_ref_count_arm/4`, `:3645` | 39 |
| level delta arm | `level_delta_select_arm/8`, `:4242` | 39 |
| edge project | `edge_statement_single/10`, `:2062` | 3 |
| edge delta project | `edge_delta_project_sql/11`, `:2180` | 3 |
| level recursive CTE arm | `level_recursive_arm_parts/9`, `:3627` | 0 |

41 distinct modules; the recursive arm at 0 is §5.7.5's number to watch (no
built-text projection sits on a `fixpointIr` head, so the priced ~28%
per-round double execution does not occur today).

Why 41 and not 17: the contract counted `concat` projected into a head text
column. `head_column_expr/6` asks the complete question — did the expression
come out as CHARACTERS — which is also true for a variable bound by
`decode/2` to a `json_extract`, a `$name` key capture, and a tagged-term
projection. The 24 extra modules are almost entirely the `json_*` family.

## 3. Before/after SQL, one module

`interpolation_desugars_to_concat`, head `message(Path, Line, concat([...]))`.

Before (at dict) — one statement, characters into an INTEGER column:

```sql
INSERT OR IGNORE INTO "message" ("path", "line_number", "text")
SELECT b0."path", b0."line_number",
       ('eprintln at ' || b0."path" || ':' || b0."line_number" || ' is waived')
FROM "eprintln_hit" b0
```

After — two statements; the concat's column operand decodes, the built value
interns before the row reads its id:

```sql
INSERT OR IGNORE INTO "__str" ("content")
SELECT DISTINCT ('eprintln at '
                 || (SELECT s."content" FROM "__str" s WHERE s."__id" = b0."path")
                 || ':' || b0."line_number" || ' is waived')
FROM "eprintln_hit" b0;

INSERT OR IGNORE INTO "message" ("path", "line_number", "text")
SELECT b0."path", b0."line_number",
       (SELECT s."__id" FROM "__str" s WHERE s."content" =
          ('eprintln at '
           || (SELECT s."content" FROM "__str" s WHERE s."__id" = b0."path")
           || ':' || b0."line_number" || ' is waived'))
FROM "eprintln_hit" b0
```

At `intern(direct)` the module is byte-identical to base.

Executed receipt, sqlite3 3.43.2, §5.7.6's own input: `diag` 3 rows, `__str`
2 rows (duplicate interned once), decoded output identical to the direct
baseline, NULL row dropped by both forms for the same NOT NULL reason.
`EXPLAIN QUERY PLAN`: `SCAN b0` + correlated scalar subquery `SEARCH s USING
COVERING INDEX sqlite_autoindex___str_1 (content=?)` — one probe per row.

## 4. The `typed` encoding design

```prolog
typed(Sql, Type, Encoding).       % was typed/2; Encoding = dict | direct

% column_encoding(+Mode, +DeclaredType, -Encoding) is det.      lower.pl:980
% compile_expr(+Mode, +Demand, +Expr, +Bound, -Sql, -Type, -Encoding), was /6.
%   Encoding is the encoding of Sql, not of Expr: `value` demand on a dict
%   column yields direct, because the decode already happened.
% demanded_sql(+Demand, +BoundEncoding, +BoundSql, -Sql, -Encoding)      :521
% aligned_pair(+LeftEnc, +LeftSql, +RightEnc, +RightSql, -L, -R)         :304
%   One id beside characters: resolve the characters, never decode the id,
%   so the indexed column stays bare on its side of the comparison.
```

Arity 3 rather than a new Type token: `Type` reaches
`join_column_types_agree/4`, `check_comparison_types/4`,
`arithmetic_result_type/4`; a `dict_text` token would need spelling at every
one and any miss is a silent equality. Encoding is a different question and
gets its own slot. `compile_expr` grew an output rather than a second walk:
a parallel `expr_encoding/5` is exactly the drift `interned_literals/2` was
written to avoid.

22 call sites threaded in lower.pl (list in the lane transcript; consumers
of the encoding are `head_column_expr/6`, `demanded_sql/5`, and
`aligned_pair/6` at six sites). `join_column_types_agree/4` deliberately
unchanged: type mismatch is refused, encoding mismatch is resolved —
different questions. `where_text/3` lost its Mode argument; `lit/3`'s third
field became the ENCODING, which closes the `compile_sub_args/8` hole
(a destructured `json_extract` position is `direct` in every mode).

## 5. The finding that needs a referee: three families have no statement seam

§5.7.1's premise ("both statements go in the tick's existing batch") is
FALSE for three of the five families:

| family | emitted-plan carrier | second statement possible |
|---|---|---|
| level recompute | `recompute_sql`, `;`-joined, `executeMultiple` (sqlRunner.ts:28) | yes — landed |
| level delta | `insert_sql`, ONE string, `execute` with RETURNING consumed (1_incremental.ts:468) | no |
| level refCount | `support_sql`, fixed 11-tuple destructured positionally (1_incremental.ts:552-556) | no |
| edge project / edge delta | `project_sql` + `write_sql` bound per arrival | no |

`db.execute` is @libsql single-statement; SQLite has no DML-in-CTE, no
statement expression, and RETURNING does not survive INSTEAD OF triggers.
Cost today at dict: those families' built values resolve to NULL against a
NOT NULL column and the row drops (before this lane: characters stored in an
INTEGER column, row unfindable — both wrong, neither reachable by a runnable
gate until closed).

Exits priced:

| option | cost | verdict |
|---|---|---|
| dict-only optional field per statement kind (`intern_sql?` on level/edge statements, `support_intern_sql?`) — mirrors the existing `expand_sql?`/`dred_sql?` optional-field precedent | types.ts + two reads in 1_incremental.ts + edge resolver block + one classifier class | lane recommendation; APPROVED by coordinator post-review (pass 2) |
| INSTEAD OF INSERT trigger view | FOR EACH ROW = per-row write, N+1 law | rejected |
| per-column direct fallback | rev 2's answer, deleted by rev 3 user word | rejected |

## 6. Deviations from the contract

| # | contract | landed | reason |
|---|---|---|---|
| 1 | §5.7.1's `JOIN "__str"` form | correlated scalar subquery | same plan (receipt §3); composes for N built columns; one id-lookup spelling shared with I-C so the seed reader cannot drift |
| 2 | 17 modules | 41 | contract counted concat; compiler counts encodings |
| 3 | §5.7.5 compile-time warning | not written | a counter pinned at 0 with no fixture able to move it; measured instead (recursive arm = 0) |
| 4 | §5.3 receipt per `text_only` expression row | not written | `norm/1` 0 corpus occurrences; mechanism pinned at `demanded_sql/5` unit; owed with the first norm fixture |
| 5 | — | `aligned_pair/6` is new, contract has no rule for it | rev 3 §5.6 reasons about stored columns; a `json_extract` value is not a column and is `direct` under both modes; the state is reachable today (`switch_as_keyed_replace`) |

## 7. Gates, verbatim

```
SWEEP total=308 compiled=211 unsupported=97 crash=0
RUN total=211 identical=210 wrong=0 emitted_crash=0 rejection=1 no_oracle_log=0
FINAL total=211 final_identical=210 final_wrong=0 no_oracle_final=1
MANIFEST_REASON_DIFF restated=0 args=0 bucket_moved=0 added=0 removed=0

plunit: All 429 (+46 sub-tests) passed        (baseline 419)
tsgo --noEmit: clean
pnpm test: 183 tests, 182 pass, 1 skip
INTERN_AB modules=211 built-id=437 decode-read=1863 decode-subquery=23
  decode-view=1242 dictionary-ddl=184 door-call=772 door-plan=2093
  intern-write=141 ir-encoding=32 literal-id=802 literal-seed=463
  mode-stamp=422 seed-id=423 value-decode=327 unexplained=0
ARCH.pl: 7 PASS
git status compile/out: empty (0 direct bytes moved)
```

`literal-id` holding at 802 across the classifier rewrite is the check that
the new paren-balanced scanner is equivalent to the regex it replaced.
The flip gate was NOT run and still cannot be (§5).

## 8. Fail-first receipts

Sabotaging each decision predicate, one at a time, restored after:
`head_column_expr/6` interned arm → tests 22 and 24 red;
`demanded_sql/5` value clause → tests 27 and 30 red;
`align_to_encoding/4` dict clause → test 31 red.
Every `_at_direct` twin stayed green through all three. List pinned in the
plunit test header.

New tests: `built_string_projection_interns_on_write`,
`built_string_projection_stays_a_word_at_direct`,
`built_string_intern_precedes_the_row_insert`,
`the_intern_statement_repeats_the_arms_from_and_where`,
`two_built_columns_union_into_one_intern_statement`,
`interned_column_decodes_under_value_demand`,
`interned_column_keeps_its_id_under_identity_demand`,
`text_column_stays_a_column_at_direct`,
`concat_over_a_text_column_reads_characters`,
`a_characters_side_join_resolves_to_an_id`.

## 9. Residuals for I-K-R / pass 2

| # | residual | who |
|---|---|---|
| 1 | the statement seam for delta insert, refCount seed, both edge projections (§5): option A approved, pass 2 implements | I-K pass 2 |
| 2 | `\==` between dict column and direct expression: NULL-drop hazard; 10 corpus `<>` all against seeded literals, 0 live occurrences; `IS NOT` when the first appears | I-K-R |
| 3 | `norm/1` end-to-end unexercised (0 fixtures) | I-K-R |
| 4 | `ir_bound/3` drops the encoding: offloaded executor sees a Bound variable's type but not id-ness; same shape as the eq_lit fence | offload contract |
| 5 | `__str_stats` short by built strings the door never sees (141 statements) | I-G |
| 6 | bare atom into a `json` column still id-resolved under identity demand (0 occurrences); encoding slot makes it detectable at `head_column_expr/6` | I-C-R |
