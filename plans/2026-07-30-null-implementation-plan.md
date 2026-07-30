# Null implementation plan

Lane: `lane/nullplan`, worktree `/Users/chrishafley/projects/sprefa-lane-nullplan`.
Base sha: `22c0c9f71ca6b16e848c53f8980f4b0c6e3d6ecd` (`git rev-parse HEAD`, confirmed).

This lane wrote one document and no code. Everything below that says MEASURED was
executed by this lane against `sqlite3` 3.43.2 and `@libsql/client` (bundled SQLite
3.45.1); the receipts are in section 2 and are reproducible from the SQL quoted
there.

## 0. What is already decided, and is not re-opened here

The ruling is to have null. The design is settled by
[2026-07-30-null-coherence-lab.md](2026-07-30-null-coherence-lab.md) and its 28
dual-engine assertions in
[2026-07-30-null-coherence-receipts.mjs](2026-07-30-null-coherence-receipts.mjs):

- **Design D**: `T?` marked columns, total language equality, explicit presence
  narrowing, nullable keys rejected.
- Surface token: unquoted `null`; quoted `'null'` stays text.
- Oracle ground value: a reserved ground compound.
- Tick-log encoding: JSON literal `null` in the existing row array.
- json1 trap: `json_extract` and `->>` collapse absent-key with present-JSON-null;
  `json_type`, `->` and `json_each` preserve the distinction.
- Column null is tier 0, class (c), the first tier-0 construct this project accepts.

This plan takes those as input and answers only: what lands, in what order, with
which receipt, and what it costs in plan shape.

Four lab claims this lane sharpened rather than repeated, all with executed
receipts (section 2):

1. The lab's key contract says "a nullable column cannot appear in a **declared**
   key". That is necessary and not sufficient. `lower.pl:719` uses **every column**
   as the primary key when a set rel has no declared key, and `lower.pl:738` makes
   that table `WITHOUT ROWID`. MEASURED: that DDL **rejects the row outright**, and
   the obvious ordinary-rowid fallback **accumulates duplicates** and breaks set
   semantics. Both failure modes, and the fix, are in step 3.
2. "Lower `=` to a null-safe comparison" is one rewrite in the lab and **two** in the
   emitted SQL. MEASURED: `IS` keeps a plain column index and **loses** an expression
   index; `json_array(...) = json_array(...)` keeps the expression index and cannot
   serve a partial-column join. Rewriting every `=` to `IS` without also fixing the
   index turns the refCount maintenance subquery from `SEARCH` into `SCAN` per head
   row. That is the formerly-quadratic class the COUNT/EXPLAIN law exists for, and it
   is priced in step 3 and step 4.
3. Card 2's "unquoted `null`" collides with the central spelling ruling. MEASURED
   through the real parser: `repo('alpha', null).` today parses to
   `repo(alpha, _G)` with `bindings=[null=_G]`; a bare identifier in argument
   position is a **fresh variable**, per `SYNTAX.md`'s spelling call. Written naively,
   `repo_latest(Repo, null)` would silently mean "any value" and the flagship
   outer-join program would be silently wrong. `null` must be a **reserved value
   word**, and the mechanism already exists: `repo(alpha, true).` MEASURED parses to
   `bool_lit(true)`, not a variable. Step 1 carries this.
4. Card 4's `present(Commit)` is **not** caught by refusal-by-absence. MEASURED:
   `b(R,C) <- a(R,C), present(C).` parses today to an ordinary body atom
   `present(_C)` with zero findings, because it looks like a relation atom and an
   undeclared relation is a legal EDB (design review A5, ruling `edb_definition`).
   `present` therefore needs a **reserved body word** in the parser as well as a
   registry row, or a typo'd `present` becomes a silent empty EDB. Step 6 carries this.

## 1. Baseline on this base sha

Run by this lane, on `22c0c9f7`:

| gate | measured here | justfile / doc comment says |
|---|---|---|
| `just conformance` | 193 PASS, 0 other | "expect: 156 PASS" (stale) |
| `just plunit` | 222/222 | "expect: 137/137" (stale) |
| `just text-door` | `compiled=133 byte_identical=133 failures=0` | "expect: 95/95/0" (stale) |
| `just sweep` | not run here (no `node_modules` in this worktree) | justfile "total=95 identical=93"; `SCOREBOARD.md` "155 swept / 94 compiled / 61 unsupported" |

The justfile expect comments and `SCOREBOARD.md` disagree with each other and with
the tree. **Step 1 of the implementing lane records the true baseline first**; every
later step's receipt is a delta against that recorded number, not against a comment.

Fixture corpus: 193 `fixture/5` facts across 23 files in
`v6/prolog/conformance/fixtures/`. Refusal inventory: 53 distinct
`unsupported_construct/1` reason functors across `v6/prolog/**.pl`.

## 2. Receipts this lane executed

All against a scratch in-memory database, `SPREFA_CONFIG=/nonexistent/nullplan.toml
DL_NO_DAEMON=1`, no daemon touched, nothing under `~/.local/state` read or written.

### 2.1 Storage and row identity

| id | SQL executed | result |
|---|---|---|
| P1 | `CREATE TABLE t(a TEXT NOT NULL, b TEXT, PRIMARY KEY(a,b)) WITHOUT ROWID` then insert `('alpha', NULL)` | **`NOT NULL constraint failed: t.b`**; the current unkeyed set-rel DDL shape cannot store a null row at all |
| P1b | same columns as an ordinary rowid table with `PRIMARY KEY(a,b)`, two identical `INSERT OR IGNORE` of `('alpha', NULL)` | **2 rows**; set semantics broken, `INSERT OR IGNORE` does not dedupe |
| P2 | plain rowid table + `CREATE UNIQUE INDEX t_row ON t(json_array(a,b))`, insert `('alpha',NULL)` twice, `('alpha','null')`, `('alpha','c1')` | **3 rows**; the two nulls deduped to `["alpha",null]`, and the TEXT string `'null'` stayed distinct as `["alpha","null"]` |
| P14 | `INSERT OR IGNORE INTO head(a,b) SELECT a,b FROM src RETURNING a,b` against the expression unique index, `src` holding a duplicated null row | 2 rows returned, duplicate deduped; **`RETURNING` works with `OR IGNORE` on an expression unique index** |
| P15 | the same statement run a second time | returns nothing; level recompute stays idempotent |
| P9 | keyed rel `PRIMARY KEY(session) WITHOUT ROWID`, `ON CONFLICT(session) DO UPDATE` writing a NULL non-key column | 1 row, replaced; **only keys must be total; a null non-key column replaces correctly** |

### 2.2 Equality, negation, and the plans

| id | executed | result |
|---|---|---|
| P3a | `NOT EXISTS (SELECT 1 FROM latest n WHERE n.repo = s.repo AND n."commit" = s."commit")` over a null-bearing row present on both sides | **the null-bearing row survives negation**; negation reports a stored row as absent |
| P3b/P3c | same with `IS` / with `IS NOT DISTINCT FROM` | zero rows; correct, agrees with oracle membership |
| P4a/b/c | `EXPLAIN QUERY PLAN` of all three forms | **identical plan in all three**: `SEARCH n USING COVERING INDEX sqlite_autoindex_latest_1 (repo=? AND commit=?)`. The null-safe rewrite costs zero plan shape on a plain column index |
| P13 | `IS NOT DISTINCT FROM` / `IS DISTINCT FROM` on 3.43.2 | supported, two-valued (`1`, `0`, `1`) |
| P10 / P10b | `EXPLAIN` of `col IS ?` and `col = ?` with **bound parameters** on a plain column index | identical: `SEARCH row_set USING COVERING INDEX row_set_cols (repo=? AND commit=?)` |
| P12 | correlated scalar subquery `(SELECT n.__support_count FROM support_next n WHERE h.a = n.a AND h.b = n.b)` over a null-bearing row | `=` form returns **no row** (drives refCount to 0 and retracts a live row); `IS` form returns 2 |
| P21 | the `InsertNewSql` `NOT EXISTS` shape, three ways | `IS` 0, `json_array` 0, **`=` 1 (wrong)** |
| P16 | `EXPLAIN` of the refCount `UPDATE ... COALESCE((SELECT ... WHERE h.a IS n.a AND h.b IS n.b),0)` against a table whose only index is `UNIQUE(json_array(a,b))` | **`SCAN n`**; the `IS` rewrite loses an expression index |
| P17 | same UPDATE written `json_array(n.a,n.b) = json_array(h.a,h.b)` | **`SEARCH n USING INDEX supp_row (<expr>=?)`** |
| P18 | same `IS` form with a plain `CREATE INDEX supp2_cols(a,b)` also present | **`SEARCH n USING INDEX supp2_cols (a=? AND b=?)`**; a plain column index restores the plan |
| P19 / P20 | `NOT EXISTS` over a table carrying only the expression index | `IS` form `SCAN h`; `json_array` form `SEARCH h USING INDEX head_row (<expr>=?)` |

**Rule extracted from P16-P20**: `IS` uses a plain column index and never an
expression index; `json_array(...) = json_array(...)` uses the expression index and
cannot serve a partial-column join. A table with a nullable column therefore needs
**both** indexes, and the rewrite is `IS` everywhere except full-row identity
addressing.

### 2.3 Row addressing, aggregation, json, driver

| id | executed | result |
|---|---|---|
| P5 | `(repo,"commit") IN (SELECT repo,"commit" FROM scope)` where both rows hold a null | **0 matches**; the `EXISTS ... IS` rewrite gets 1 |
| P8 | `DELETE FROM t WHERE repo = 'alpha' AND "commit" = NULL` | deletes nothing; the `IS` form deletes the row |
| P10c/d | `EXPLAIN` of `DELETE ... WHERE json_array(a,b) = json_array(?,?)` and `... IN (SELECT value FROM json_each(?))` | `SEARCH row_set USING INDEX row_set_row (<expr>=?)` in both; indexed, not a scan |
| P11 | the same delete executed | removes the null-bearing row |
| P7 | `GROUP BY` and `SELECT DISTINCT` over a nullable column | nulls form one group; duplicate nulls collapse; **both already null-safe, no change needed** |
| P6 | `json_valid(NULL)`, `json_type(NULL)`, and the whole `canonical_column_expr` CASE over a NULL column | all NULL; **`canonical_column_expr` needs no change; a NULL column reads back as NULL** |
| L1 | `@libsql` 3.45.1: bind JS `null`, read a NULL column back | driver returns JS `null` (`typeof` `object`); `IS ?` and `IS NOT DISTINCT FROM ?` with a bound null match, `= ?` does not |
| L2 | `@libsql`: insert JS `null` into a `TEXT NOT NULL` column | `SQLITE_CONSTRAINT: NOT NULL constraint failed`; a nullable value reaching a total column is a **loud** runtime failure, never silent |
| L3 | `JSON.stringify(["a", null])` | `["a",null]`; `multisetDiff`'s `rowKey` and `boundaryDelta`'s row key are already null-safe with no change |

### 2.4 The real parser, on this base sha

Executed through `parse_dl/4` (codes in), not read off the DCG.

| id | input | result |
|---|---|---|
| G1 | `rel repo(name: text, commit: text?).` | **`dl_parse_error(statement, ...)`**; a hard parse error today. Accepting `?` therefore cannot change any existing program's meaning |
| G1b | `rel repo(name: text, stars: int?).` | same |
| G2 | `rel repo(name: text, commit: text).` | parses to `col_type(repo/2,name,text), col_type(repo/2,commit,text)` |
| G3 | `repo('alpha', null).` | **`repo(alpha, _G)` with `bindings=[null=_G]`**; bare `null` is a fresh **variable**, not a value |
| G4 | `repo(alpha, true).` | `repo(_G, bool_lit(true))`; **`true` is already a reserved value word**; the mechanism `null` needs exists |
| G5 | `repo('alpha', 'null').` | `repo(alpha, null)`; quoted stays the ordinary text atom, as card 2 wants |
| G6 | `b(R,C) <- a(R,C), present(C).` | parses to an ordinary body atom `present(_C)`, **zero findings**; refusal-by-absence does not fire on a relation-shaped goal |
| G7 | corpus collision scan | `grep -c '\bpresent('` over `prolog/conformance/`, `dl/fixtures/`, `dl_view/` = **0**. `grep '\bnull\b'` over every `.dl6` = 16 hits, **all** in comments or shell templates, **zero** program tokens. Both reservations are collision-free |

## 3. The type carrier: `nullable(T)`, chosen to fail closed

The type carrier already exists and is layered:

| layer | shape | file |
|---|---|---|
| surface decl | `col_type(Ref, Column, Type)` | `parse_dl.pl:412-429`, `:497-503` |
| storage kind | `column_storage/3` -> `int` \| `text` \| `float` \| `bool` \| `ref(Name)` | `0_type_plane.pl:79` |
| program-wide inference | `frozen(Type)` / `open(Type)` fixpoint, `contribution_to_type/2` | `analyze.pl:467-505` |
| lowering carrier | `relplan(Ref, Kind, Columns, KeyOrNone, ColumnTypes)` | `lower.pl` throughout |

Represent nullability as a **wrapper**, `nullable(text)` / `nullable(int)`, in
exactly those four places. The two alternatives considered and dropped were a parallel
`nullable_columns/2` decl list (splits one fact across two carriers, so a consumer can
read the type and miss the nullability) and a sixth flat atom beside `int`/`text`
(loses the base type, so `column_def/3` cannot pick a storage class).

The reason is mechanical: the wrapper **fails closed** at every existing type guard,
so a half-finished implementation is loud rather than wrong. Enumerated against the
real code:

| existing guard | with `nullable(text)` | verdict |
|---|---|---|
| `column_def/3` (`lower.pl:749-763`) | no clause head unifies | **silent Prolog failure**; needs an explicit refusal clause added FIRST (step 1) |
| `canonical_column_expr/3` (`lower.pl:2472-2491`) | no clause head unifies | **silent Prolog failure**; same |
| `join_column_types_agree/4` (`lower.pl:282`) | `nullable(text) == nullable(text)` succeeds | **silently emits `=`**; needs an explicit clause |
| `check_comparison_types(same_type, ...)` (`lower.pl:672`) | `LeftType == RightType` succeeds | **silently emits `=`**; needs an explicit clause |
| `compile_int_operand/4` (`lower.pl:521`) | `Type == int` fails | throws `arith_operand_not_int`; closed, but **misleading name** |
| `compile_aggregate_number_operand/5` (`lower.pl:2189`) | `memberchk(Type,[int,float])` fails | throws `aggregate_operand_not_number`; closed, misleading name |
| `compile_text_operand/4` (`lower.pl:481`) | `Type == text` fails | throws `text_operand_not_text`; closed, misleading name |
| `boundary_column_type/2` (`emit_ts.pl:588-589`) | passes `nullable(text)` through verbatim into the emitted `IRowColumnType[]` | **not an `IRowColumnType`**; `tsgo` flags it |

Two guards pass silently and six either fail silently or throw a lie. **Step 1 exists
to convert all eight into one named refusal before any nullable type can be
constructed.** After step 1, every subsequent step is a refusal removal plus a fixture
flip, which is this repo's proven landing shape (`edge_trigger_is_derived`,
`pre_in_edge`, `finalize_in_edge` all landed that way).

## 4. Emitted SQL site inventory

Read out of `lower.pl` at this base sha, not guessed. `emit_ts.pl` builds identifiers
and never SQL (`lower.pl:192-194` states this and it holds), so this is the whole
surface.

### 4.1 Sites that must change: 18

| # | site | file:line | current SQL | rewrite | plan cost |
|---|---|---|---|---|---|
| S1 | `column_def/3` | `lower.pl:749-763` | `... NOT NULL` (5 clauses) | nullable clauses drop `NOT NULL`; `bool`/`float` `CHECK` becomes `col IS NULL OR (...)` | none |
| S2 | `rel_ddl/6` declared-type table | `:728` | `UNIQUE (all columns)` when unkeyed | see step 3 | see step 3 |
| S3 | `rel_ddl/6` plain set table | `:738` | `PRIMARY KEY (all columns)) WITHOUT ROWID` | rowid table + `CREATE INDEX(cols)` + `CREATE UNIQUE INDEX(json_array(cols))` | one extra index per nullable rel (P2, P14, P18) |
| S4 | `support_ddl/3` | `:2461` | `PRIMARY KEY (all columns)) WITHOUT ROWID` | same as S3 | same |
| S5 | `aggregate_scope_ddl/2` | `:1917` | `PRIMARY KEY (scope cols)) WITHOUT ROWID` | same as S3 when a scope column is nullable | same |
| S6 | `where_text(pair(L,R))` | `:308` | `L = R` | new functor `pair_nullsafe(L,R)` -> `L IS R`; emitted only when either operand type is `nullable(_)` | none on a column index (P4, P10) |
| S7 | `compile_negative_uses/5` | `:376-378` | `NOT EXISTS (... WHERE <where_text>)` | inherits S6 | none (P4) |
| S8 | `qualified_equalities/4` | `:2082` | `h.col = n.col` | `h.col IS n.col`, **and the target table must carry the plain column index from S3/S4** | `SCAN` -> `SEARCH` regression if the index is omitted (P16 vs P18) |
| S9 | refCount `UpdateSql` | `:1941-1943` | correlated subquery over S8 | inherits S8 | P12: `=` form silently retracts a live row |
| S10 | refCount `InsertNewSql` | `:1947-1950` | `NOT EXISTS` over S8 | inherits S8 | P21: `=` form reports a present row as new |
| S11 | `eq_placeholder/2` -> arrival `DelSql` | `:1405`, `:1358-1360` | `DELETE ... WHERE col = ?` | `col IS ?` | none (P10, L1) |
| S12 | incremental arrival `DelSql` | `:1365-1367` | `DELETE ... WHERE (cols) IN (SELECT json_extract(value,'$[i]') FROM json_each(?))` | `DELETE ... WHERE json_array(cols) IN (SELECT value FROM json_each(?))` | indexed `SEARCH` via S3's expression index (P10d) |
| S13 | `aggregate_delete_scoped_sql/5` | `:1868-1871` | `(scope cols) IN (SELECT ... FROM scope)` | `EXISTS (SELECT 1 FROM scope c WHERE c.col IS h.col AND ...)` | P5: `IN` form matches zero |
| S14 | `aggregate_insert_scoped_sql/6` | `:1893-1894` | `(group exprs) IN (SELECT ... FROM scope)` | same as S13 | same |
| S15 | `key_join_equalities/5` | `:879-881` | `t.col = json_extract(i.value,'$[n]')` (dictionary intern conflict + lookup, `:866`, `:869`) | refuse a nullable column inside a struct type's key (declared or all-column fallback at `:844`) | none |
| S16 | `delta_reference_identity/8` | `:2308` | `r0.col = d0.col` over every column | refuse `nullable(_)` inside a `ref(_)` target's columns | none |
| S17 | `struct_intern_statements/7` boot lookup | `:2586` | `KeyColumn = KeySlot` | inherits S15's refusal | none |
| S18 | `compile_guard_goal/3` rebind-as-check | `:614`, `:622` | `VariableSql = Sql` | `IS` when either side is nullable | none |

Additionally, `compile_comparison/3` (`:639-644`) does not emit a new *site* but must
become type-directed: `==`/`\==` on nullable operands lower to `IS` / `IS NOT`, and
`< =< > >=` refuse an unnarrowed nullable operand (step 6).

### 4.2 Measured no-ops: 9 sites the implementer must NOT touch

| # | site | file:line | why unchanged |
|---|---|---|---|
| N1 | `where_text(lit(L,V))` | `:312` | SQL `col = 'x'` over NULL is UNKNOWN and drops the row, which **is** Design D's `null != value`. In a `NOT EXISTS` the oracle's `not(rel(K,'x'))` also succeeds against a stored `null_value(sql)`. Both doors already agree |
| N2 | every `GROUP BY` | `:1957`, `:2052`, `:2056`, `:2159`, `:2345` | MEASURED P7: null grouping is already null-safe |
| N3 | every `SELECT DISTINCT` | `:1844`, `:2297` | MEASURED P7 |
| N4 | every `ORDER BY` | `:200`, `:1654`, `:2358` | none orders by a data column; `_phase`/`_sequence`/`rowid` only |
| N5 | `canonical_column_expr(_, text, _)` | `:2487-2491` | MEASURED P6: the whole `json_valid`/`json_type` CASE yields NULL over a NULL column |
| N6 | keyed `ON CONFLICT` | `:1378`, `:1386`, `:1584`, `:1588` | keys stay total; MEASURED P9: a NULL non-key column replaces correctly |
| N7 | `count(*)` | `:2173` | bag count over derivation rows, null-blind by construction (engine.pl q7) |
| N8 | `HAVING count(*) > 0` | `:2162` | row count, not a value |
| N9 | `empty_recursive_anchor` | `:2003-2010` | already emits `NULL AS col ... WHERE 0` |
| N10 | `pre_ddl/3` unkeyed branch | `:2406` | plain `CREATE TEMP TABLE`, no primary key |

The internal `NOT NULL` columns stay `NOT NULL` and are not part of this arc:
`__tick.n` (`:600`), `__support_count` (`:722`, `:2461`), `_phase`, `_sequence`,
`_sign` (`:2385`, `:2419`, `:2429`, `:2439`).

## 5. The oracle door

### 5.1 Representation: `null_value(sql)`

A reserved ground compound term. The parser owns the token: unquoted surface `null`
in value position produces it; quoted `'null'` stays the ordinary text atom.

**Why a compound and not an atom.** MEASURED on the corpus:
`grep -rno '\bnone\b' v6/prolog/conformance/fixtures` = **84 hits**, including
`spine_semantics.pl:168-170` whose own comment says the atom `none` "is the ordinary
`none` value already used for" an absent pointer, and `body.pl:147` (`Value \== none`)
which already reads it as JSON absence. `\bnull\b` = **1 hit** (`json_arm.pl:37`, a
comment). Every text column's value domain is atomics, so any atom chosen as the null
marker is indistinguishable from a text column legitimately holding that word, and
`sql_literal/2` (`lower.pl:208-220`) would render it as quoted text with no hook to
intercept. A compound cannot collide, because a text value is never compound.

**The precedent is already in the tree**: `bool_lit(true)` is a reserved ground
compound whose SQL storage (`INTEGER 1`, `lower.pl:221-222`) is nothing like its term
shape, whose tick-log rendering has its own clause (`ticklog.pl:111`), and which is
recognized as a literal witness by the type inference (`analyze.pl:402-409`).
`null_value(sql)` is the same pattern with zero new mechanism.

### 5.2 Why the oracle needs no engine change

MEASURED by reading, not assumed. `engine.pl` decides row identity with:

| operation | file:line | behaviour on `null_value(sql)` |
|---|---|---|
| `memberchk(srow(Row), Store)` | `:316` | term identity, total on a ground compound |
| `exclude(==(srow(Row)), Store0, _)` | `:308` | `==`, total |
| `key_of/3` + key unification | `:121`, `:320-325` | keys are refused nullable, so unreachable |
| `sort/2`, `msort/2` on rows | `:211`, `:335-336`, `:420` | standard order, total |
| `ord_subtract/3` | `:410`, `:419` | standard order, total |
| `set_diff_delta/3` (`memberchk`) | `:462-464` | term identity, total |
| `eval_expr(Value, Value)` fallthrough | `body.pl:59` | `null_value/1` matches no earlier clause (not arithmetic, not a registry `text_scalar`, not `{}/1`, not a list) and evaluates to itself |

Total ground-term identity is exactly what Design D's "total language equality"
means, and Prolog supplies it for free. **The oracle change is three clauses and one
predicate, not an engine rewrite.**

One caveat belongs in the fixture-authoring notes. Prolog standard
order sorts compounds after atoms and numbers, so a fixture's `final(Ref, Expected)`
list (compared with `==` after `msort`, `engine.pl:545-547`, `:568-572`) places
null-bearing rows last. `deltas(Ref, Expected)` is compared **unsorted**
(`engine.pl:551-554`), so intra-tick delta order is fixture data as it already is.

### 5.3 The four oracle-side edits

| edit | file | shape |
|---|---|---|
| tick-log encoder | `ticklog.pl` | a `value_json(null_value(sql), 'null')` clause placed **before** the `compound(Value)` clause at `:114`, which would otherwise render `"null_value(sql)"` |
| presence goal | `body.pl` | `solve(present(Value), _) :- !, eval_expr(Value, V), V \== null_value(sql).` beside the existing `solve/2` clauses |
| json canonicalization | `body.pl` | `json_canon(null_value(sql), null_value(sql))` and a `braces_decode` arm distinguishing key-absent from present-JSON-null (section 7) |
| served-door schedule values | `compile/scripts/dl6_oracle.pl:65-67` | `schedule_value/2` currently ends in `term_to_atom(Value, Atom)`, so a JSON `null` posted over HTTP becomes the **atom `null`** while the emitter binds SQL NULL. A `schedule_value(null, null_value(sql))` clause ahead of it closes a live two-door divergence at the served grading door |

### 5.4 The formal check against TICK-MODEL.md

`TICK-MODEL.md` section 1 makes the B ring (level/set membership) idempotent boolean
and section 2 makes the tick log the discrete derivative of B. Under Design D:

- A `T?` column's value domain is `dom(T) ⊎ {null_value(sql)}`, a **disjoint sum on
  the value axis**. The membership ring is untouched: a row either is in the relation
  or is not.
- All three rings (B, N, Z) are therefore unchanged, and every theorem in section 5
  survives verbatim.
- Designs B and C would put SQL UNKNOWN into rule constraints. A constraint that
  answers UNKNOWN makes **membership** three-valued, which is not a boolean semiring,
  and section 2's "the delta stream is the derivative of the B relation" stops being
  well-defined. **That is the formal reason Design D is the only candidate compatible
  with TICK-MODEL.md as written**, independent of the ergonomics argument the lab
  makes. The implementing lane adds this as a row in section 4's coercion table
  (`T?` = a value-domain sum, no ring coercion) and a line in section 5.

## 6. The delta and tick-log contract

Stated once here because the tick log is byte-diffed across targets and this cannot
be discovered per runtime. Derived from B-ring idempotence, and matching the lab's D1
measurement rather than depending on it.

```text
ROW IDENTITY
  Two rows are the same row when every column is null-safe equal.
  SQLite:  json_array(cols) equality (unique index), or column-wise IS
  Prolog:  ==/2 on ground terms
  JS:      JSON.stringify(row) as the map key (measured L3: ["a",null])

TRANSITIONS at one relation, one key or one row slot
  null -> null    identical write, B is idempotent: NO B change, NO Z event,
                  the tick log line does not mention the relation
  null -> value   one -old then one +new
  value -> null   one -old then one +new
  absent -> null  one +new     (a null row is a present row)
  null -> absent  one -old

ENCODING
  oracle term      null_value(sql)
  SQLite storage   SQL NULL
  driver seam      JS null
  tick-log JSON    the literal null inside the row array, e.g. ["beta",null]
  final-state read same
  quoted text      'null' is the four-character string, encoded "null"
```

The last line is the discriminating case and gets its own fixture: a program holding
both a real null and the text `'null'` in the same column, whose tick log must show
`[null]` and `["null"]` as different bytes. MEASURED P2 shows SQLite keeps them
distinct in the row-identity index.

Sorting is unchanged: `ticklog.pl:98-99` and `ticklog.ts` both `sort` the already-
rendered JSON row texts, so `null` sorts as the four bytes `null` on both doors with
no new comparator.

## 7. The json junction

`lane/json-wiring` owns `parse_dl.pl`, `print_dl.pl`, `lower.pl`, `registry.pl` and
`analyze.pl` right now. This is a contract between the two lanes, not an edit.

That lane's current lowering (see `plans/2026-07-30-json-syntax-lab.md:160-186`) uses
`json_extract(src, path) IS NOT NULL` as its presence predicate, and its
**CARD-JSON-NULL is open**. Per the lab's receipt J1, `json_extract` returns SQL NULL
for **both** an absent key and a present JSON null, so that predicate discards both.
That is correct while every column is total and wrong the moment `T?` exists.

**The contract, whichever lane lands second:**

```text
Two outputs per decoded field, in this order:
  presence  :=  json_type(source, path) IS NOT NULL
  value     :=  json_extract(source, path)

Destination totality decides what happens:
  target T   , key absent            -> presence false, body fails, no row
  target T   , key present, JSON null-> named refusal json_null_into_total_column(Ref, Column)
  target T   , key present, scalar   -> bind the scalar          (unchanged today)
  target T?  , key absent            -> presence false, body fails, no row
  target T?  , key present, JSON null-> bind null_value(sql) / SQL NULL
  target T?  , key present, scalar   -> bind the scalar

Absence is NEVER null. A missing key produces no row; it does not produce a null row.
That is the line CARD-JSON-NULL option (a) draws, and this plan adopts it.
```

Oracle side of the same contract: `body.pl:145-148`'s `braces_decode` currently tests
`Value \== none`, which conflates JSON null with the ordinary text atom `none` that
84 fixture occurrences rely on. The junction fixes this too: absence is
`\+ memberchk(Key-_, Pairs)` (key not in the pair list) and a present JSON null is
`memberchk(Key-null_value(sql), Pairs)`. `decode_missing_key_fails_quietly`
(`json_arm.pl:37-45`) is the fixture that pins the current conflation and must be
split into two fixtures by whichever lane lands second.

Sequencing: **neither lane blocks the other.** The json lane can land its total-column
lowering unchanged, because `json_type` presence and `json_extract` value agree
exactly when the destination is total. The null lane's step 7 then adds the
`json_type` presence conjunct and the two destination arms. If the null lane lands
first, the json lane reads the contract off this document.

## 8. Migration

**What breaks: nothing, measured.**

- Every existing declaration is total. `col_type(Ref, Column, text)` is unchanged;
  `nullable(text)` is only ever constructed by a `?` the author wrote. There is no
  inference path that produces `nullable(_)`: `analyze.pl`'s
  `literal_witnesses_type/2` (`:422-431`) can only answer `int` / `float` / `bool` /
  `text`, and `contribution_to_type/2` (`:483-485`) turns "no witness" into `text`.
  A null column is opt-in at the decl, full stop.
- Every emitted DDL byte, every emitted SQL byte, and every checked-in `gen_emitted`
  module for a program with no `?` stays **byte-identical**. That is the receipt for
  steps 3, 4 and 5, and it is the reason `pair_nullsafe/2` is a new functor rather
  than a widened `pair/2`: `where_text(pair(L,R))` keeps emitting `L = R` unchanged.
- The 193 fixtures and 133 text-door programs need **no regrade**. The corpus grows
  by the new fixtures and nothing moves buckets. Any bucket movement in the existing
  193 is a defect in the landing, not an expected regrade, and that is exactly the
  assertion each step's receipt makes.
- The one honest exception is step 2: the tick-log encoder gains a clause. Every
  existing oracle artefact is byte-identical because no existing row contains
  `null_value(sql)`, but the **cross-target log contract text** changes, so any other
  target grading against it must adopt section 6 at the same time. Today `tsv2` is the
  only other target, and it is in the same repo.

**What is loud rather than silent** (receipt L2): a nullable value reaching a total
column fails at the driver with `SQLITE_CONSTRAINT: NOT NULL constraint failed`,
naming the table and column. The checker refusal in step 6 catches it earlier, and
this is the backstop.

**Deferred with named slots, not silently dropped:**

| slot | what it defers |
|---|---|
| `slot_nullable_host_output` | `sh_decl/4` output columns typed `T?`, and the `coerce/3` path at `serve/1_hosts.ts:199` |
| `slot_nullable_bind_column` | `bind_decl/2` columns typed `T?` |
| `slot_nullable_ref_column` | `ref(T)?`; a nullable relation reference; refused by S16 until the struct plane has a null-safe dictionary lookup |
| `slot_nullable_key` | the lab's alternative: `NULLS NOT DISTINCT` key identity. Refused, and the lab already prices it |
| `slot_nullable_enum_variant` | `T?` inside an `enum_decl/2` variant field |

## 9. The ordered steps

Each step is independently landable, leaves `just green` at exit 0, and states the
test that is **red before and green after**. "Reversible" means reverting that one
commit restores green with no fixture rewrite and no regeneration outside
`gen_emitted/`.

---

### Step 1; Fail-closed floor: `col: text?` parses and is refused by name

**Files**: `parse_dl.pl`, `print_dl.pl`, `0_type_plane.pl`, `lower.pl` (refusal
clauses only), `registry.pl` (SYNTAX regen), `1_emit_registry_docs.pl`
(tmLanguage regen), `analyze.pl`.

**Change**

- `typed_column_type/3` (`parse_dl.pl:419-429`) gains a trailing `?`:
  `typed_column_type(nullable(Type)) --> base_type(Type), '?'`. **The `?` does not
  collide**, on two receipts. By reading: `?` appears in exactly two places,
  `query_stmt/5` (`:766`, statement-initial) and the probe rider per `SYNTAX.md:144`,
  and `ident/3` (`:212-221`) accepts only alpha/underscore then alnum/underscore, so
  `?` can never be part of a type name. By execution: receipts G1/G1b, `text?` and
  `int?` are `dl_parse_error(statement, ...)` today, so accepting them cannot change
  any existing program's meaning.
- **`null` becomes a reserved value word**, in the same clause family that already
  reserves `true`/`false` into `bool_lit(_)` (receipt G4). Without this, receipt G3
  says bare `null` is a fresh variable and `repo_latest(Repo, null)` silently means
  "any value". Quoted `'null'` keeps parsing to the text atom `null` (receipt G5).
  Collision-free: receipt G7 finds zero `null` program tokens in the `.dl6` corpus.
  In this step `null` parses and is refused; step 2 gives it its oracle value.
- `print_decl_column/3` (`print_dl.pl:307-311`) prints `col: text?`. **The printer's
  synthesis path must never emit `?`**: `print_dl.pl:94-136` synthesizes `col_type/3`
  for witness-carrying undeclared EDB refs, and inference cannot produce
  `nullable(_)` (section 8), so this is a no-op assertion, stated as a comment.
- `column_storage/3` (`0_type_plane.pl:79`) resolves `nullable(T)` to
  `nullable(Storage)`.
- **The eight guards from section 3** each gain an explicit leading clause throwing
  `unsupported_construct(nullable_column_not_lowered(Ref, Column, Type))`.
- `present/1` is deliberately **not** enabled yet. It cannot simply be left out:
  receipt G6 shows `present(C)` parses today as an ordinary relation atom with zero
  findings, so refusal-by-absence never fires. This step therefore adds `present` as a
  **reserved body word** whose only behaviour is to throw
  `unsupported_construct(nullable_narrowing_not_lowered(Goal))`; step 6 replaces the
  throw with the real narrowing. Collision-free: receipt G7 finds zero relations named
  `present` in `prolog/conformance/`, `dl/fixtures/` or `dl_view/`.

**Fail-first receipt**

New fixture `nullable_column_refused` in
`v6/prolog/conformance/fixtures/` plus a `.dl6` in `v6/dl/fixtures/`:

1. Before: `parse_dl` over `rel repo(name: text, commit: text?).` throws
   `dl_parse_error(statement, ...)` (receipt G1). After: `col_type(repo/2, commit,
   nullable(text))`.
2. Before: `repo('alpha', null).` binds `null` as a variable (receipt G3). After: it
   is a reserved value word, and a plunit asserts `bindings == []` for that input.
   Sabotage receipt in the test header: remove the reservation and the assertion goes
   red with `bindings=[null=_]`, which is the silent-wrong shape.
3. Before: no such refusal exists in `refusal_inventory/1`. After,
   `nullable_column_not_lowered` and `nullable_narrowing_not_lowered` both appear, each
   with a `prolog:message//1` line (the design review's B4 finding).
4. `just roundtrip` G1 prints `text?` back and reparses to a variant.
5. `just conformance` 193 -> 194 (one new refusal fixture), zero movement elsewhere.
6. `just text-door` 133 -> 134/134/0.
7. `just sweep` gains exactly one UNSUPPORTED row named `nullable_column_not_lowered`;
   every other bucket byte-identical.

**Reversible**: yes. Nothing downstream depends on it.
**Size**: S.

---

### Step 2; Oracle ground value, tick-log encoder, served-door schedule

**Files**: `parse_dl.pl`, `print_dl.pl`, `conformance/ticklog.pl`, `conformance/body.pl`,
`compile/scripts/dl6_oracle.pl`, new fixtures.

**Change**

- Step 1's reserved `null` word now produces `null_value(sql)` instead of throwing;
  quoted `'null'` stays text. Printer inverts, printing `null_value(sql)` back as bare
  `null` and the text atom `null` back as `'null'`.
- `ticklog.pl` gains `value_json(null_value(sql), 'null')` ahead of `:114`.
- `body.pl` gains `solve(present(Value), _)` and `json_canon(null_value(sql),
  null_value(sql))`.
- `dl6_oracle.pl:65-67` gains `schedule_value(null, null_value(sql))`.
- The compiler still refuses (step 1's refusal stands), so the oracle can run null
  programs and the emitter cannot. That is a legal intermediate state: the sweep bucket
  stays UNSUPPORTED-by-name.

**Fail-first receipt**

1. New oracle-graded fixture `null_column_outer_join` (the lab's worked Design D
   program, section 6.D): `repo_latest(Repo, Commit) <- repo(Repo), latest(Repo,
   Commit).` and `repo_latest(Repo, null) <- repo(Repo), not(latest(Repo, _)).` with
   `final/2` and `deltas/2` expectations. Before the parser change it does not parse;
   before the `ticklog` clause its tick log prints `"null_value(sql)"`; after, `null`.
2. New fixture `text_null_and_real_null_are_distinct`: one column holding both
   `null` and `'null'`; tick log must show `[null]` and `["null"]`.
3. `just plunit` gains a unit asserting `engine.pl` needs no change: run the section
   5.2 identity operations (`memberchk`, `==`, `sort`, `set_diff_delta`) over rows
   containing `null_value(sql)` and assert each is total.
4. `just conformance` +2, `just roundtrip` +2, no other movement.

**Reversible**: yes today, and only today. **This step changes the cross-target tick-log
contract text**; once a second target grades against it the revert window closes.
**Size**: M.

---

### Step 3; Nullable storage and null-safe row identity in DDL

**Files**: `lower.pl` (`column_def/3`, `rel_ddl/6`, `support_ddl/3`,
`aggregate_scope_ddl/2`), `test/run_sql_check.pl`.

**Change**

- `column_def/3` nullable clauses: drop `NOT NULL`; `bool` `CHECK` becomes
  `col IS NULL OR col IN (0,1)`; `float` `CHECK` likewise.
- A set / level / support / aggregate-scope table **whose columns include a
  `nullable(_)` and whose primary key is the all-column fallback** switches from
  `PRIMARY KEY (cols) WITHOUT ROWID` to:
  ```sql
  CREATE TABLE t (cols);                            -- ordinary rowid table
  CREATE INDEX      "t_cols" ON t (cols);           -- serves IS lookups   (P18)
  CREATE UNIQUE INDEX "t_row" ON t (json_array(cols)); -- null-safe set identity (P2)
  ```
  Both indexes, not one. MEASURED P16/P18/P19/P20: `IS` uses only the plain index and
  `json_array(...)` equality uses only the expression index, so a single index loses a
  plan somewhere. This is the storage cost of a nullable rel and it is stated, not
  hidden: two indexes and a rowid instead of one integrated PK, for that rel only.
- Rels with a **declared** key are unchanged (keys stay total; MEASURED P9).

**Fail-first receipt**

1. `test/run_sql_check.pl` gains: run the generated DDL for a nullable rel and insert
   `('alpha', NULL)` twice. **Before**: `NOT NULL constraint failed` (P1). **After**:
   one row, and a third insert of `('alpha','null')` makes two (P2).
2. **EXPLAIN assertion, COUNT-test law**: `EXPLAIN QUERY PLAN` of the row-identity
   delete must contain `SEARCH` and `USING INDEX "t_row"` and must not contain `SCAN`
   (P10c). Sabotage receipt in the test header: drop `t_row` and the assertion goes
   red with `SCAN`.
3. **EXPLAIN assertion**: the refCount correlated subquery in `IS` form must plan as
   `SEARCH ... USING INDEX "t_cols"` (P18). Sabotage: drop `t_cols` and it goes red
   with `SCAN` (P16).
4. `just sweep`; every one of the existing compiled fixtures emits **byte-identical**
   SQL, because none declares `?`. That byte-identity is the receipt that this step
   cannot regress an existing program.

**Reversible**: yes; reverting regenerates `gen_emitted/` back to identical bytes.
**Size**: M.

---

### Step 4; Null-safe equality in joins, negation, and refCount

**Files**: `lower.pl` (`where_text/2`, `compile_pattern_arg/7`,
`compile_negative_uses/5`, `qualified_equalities/4`, `level_support_sql/4`,
`compile_guard_goal/3`).

**Change**

- New `where_text` functor `pair_nullsafe(Left, Right)` -> `Left IS Right`.
  `compile_pattern_arg/7` (`lower.pl:244-263`) emits it instead of `pair/2` when either
  operand's type is `nullable(_)`. `pair/2` is untouched, so every existing emitted
  byte is preserved.
- `join_column_types_agree/4` (`:282-286`) gets an explicit clause: `nullable(T)`
  agrees with `nullable(T)` and refuses against a total `T` by name
  (`join_nullability_mismatch(...)`) rather than emitting a comparison across the
  boundary.
- `qualified_equalities/4` (`:2078-2084`) gains a column-type argument threaded from
  `relplan_column_types/3`, already in scope at both call sites (`level_support_sql/4`
  at `:1931`).
- `compile_guard_goal/3` rebind-as-check (`:614`, `:622`) uses `IS` when either side is
  nullable.

**Fail-first receipt**

1. **THE negation fixture**, `negation_over_nullable_column`: a program where a
   null-bearing row exists in the negated relation. Oracle derives nothing. **Before**:
   the emitted `NOT EXISTS` with `=` derives the row (P3a). **After**: byte-identical
   to the oracle. Sabotage receipt in the test header: revert one `IS` to `=` and the
   fixture goes red.
2. **EXPLAIN assertion**: the null-safe `NOT EXISTS` plans identically to the `=` form
  ; `SEARCH ... USING COVERING INDEX ... (repo=? AND commit=?)`, no `SCAN` (P4).
3. **refCount fixture**, `refcount_over_nullable_row`: a level-headed rel with a
   nullable column and two derivations. **Before**: `UpdateSql`'s subquery misses (P12)
   and drives the count to 0, retracting a live row; `InsertNewSql` re-inserts it as
   new (P21); an oscillation visible as a spurious `-`/`+` pair every tick. **After**:
   the row is stable and the tick log is silent after the first tick.
4. `just sweep` byte-identical on all pre-existing compiled fixtures.

**Reversible**: yes.
**Size**: M.

---

### Step 5; Null-safe row addressing: deletes, IN-lists, aggregate scope

**Files**: `lower.pl` (`eq_placeholder/2`, `arrival_statement/2`,
`aggregate_delete_scoped_sql/5`, `aggregate_insert_scoped_sql/6`).

**Change**

- Arrival `DelSql`: `col = ?` -> `col IS ?` for nullable columns (S11).
- Incremental arrival `DelSql`: `(cols) IN (SELECT json_extract(...))` ->
  `json_array(cols) IN (SELECT value FROM json_each(?))` (S12), served by step 3's
  expression index.
- Aggregate scope membership: `(cols) IN (SELECT ...)` -> `EXISTS (SELECT 1 FROM scope
  c WHERE c.col IS h.col AND ...)` (S13, S14).

**Fail-first receipt**

1. Fixture `retract_null_bearing_row`: schedule adds `-repo_latest('beta', null)`.
   **Before**: the row survives retraction (P8 for the per-row path, P5 for the batched
   path) and the tick log has no `del`. **After**: one `del`, matching the oracle.
2. Fixture `aggregate_group_key_is_nullable`: a `count`/`sum` head grouped on a
   nullable column. **Before**: the scoped delete matches zero (P5), so the stale
   aggregate row survives and no recompute happens; a silently wrong total. **After**:
   the group recomputes.
3. **EXPLAIN assertion**: the batched delete plans as `SEARCH ... USING INDEX "t_row"
   (<expr>=?)`, never `SCAN` (P10d).
4. `just sweep` byte-identical on all pre-existing compiled fixtures.

**Reversible**: yes.
**Size**: M.

---

### Step 6; The checker: `present/1`, narrowing, and the named refusals

**Files**: `registry.pl`, `analyze.pl`, `0_program_check.pl`, `lower.pl`
(`compile_comparison/3`, `compile_int_operand/4`, `compile_text_operand/4`,
`compile_aggregate_number_operand/5`), `conformance/body.pl`, `SYNTAX.md` regen,
`dl6.tmLanguage.json` regen.

**Change**

- `registry.pl` gains `surface(present/1, guard, no_refs, wrapper(expr, lower), live).`
  This one row makes `present` reach the analyze dispatch, the parse/print body-word
  inventory, the generated SYNTAX construct table and the tmLanguage grammar, per the
  registry's existing contract. It does **not** on its own stop a typo becoming an
  EDB (receipt G6); step 1's reserved body word is what does that, and this step
  replaces its throw with the lowering.
- **Spelling note, recorded not decided**: `present` satisfies neither half of the
  vocabulary law cleanly. SQL's word is `IS NOT NULL`, Prolog's nearest are
  `nonvar`/`ground`, and rx has none. The lab's card 4 recommends A (`present`) over B
  (`Commit is not null`) and C (`some(Commit)`); this plan implements A because the
  lab recommended it, and flags the tension so the user can rule on B without any of
  the lowering changing (B is the same narrowing under a different token).
- Nullability rides `frozen(nullable(T))` through `program_column_types/7` so the
  inference fixpoint can never widen it away (`analyze.pl:483-505`,
  `contribution_to_type(frozen(T), T)` already does the right thing).
- **Narrowing** is a retype of a `Bound` entry, folded left to right beside the existing
  bind/guard fold (`compile_guard_goals/4`, `lower.pl:588-590`): `present(Var)` emits
  `VarSql IS NOT NULL` and rebinds `Var-typed(VarSql, nullable(T))` to
  `Var-typed(VarSql, T)` for the rest of the body. Oracle mirror:
  `solve(present(Value), _)` from step 2.
- **New refusals.** Shared classes go in `0_program_check.pl` (both doors must agree);
  compiler-capability classes stay in `analyze.pl`:

  | refusal | door | trigger |
  |---|---|---|
  | `nullable_key_column(Ref, Column, Position)` | shared | a `keyed/2` position naming a `nullable(_)` column |
  | `nullable_column_in_struct_key(TypeName, Column)` | shared | a `nullable(_)` column inside a struct type's declared key **or** its all-column fallback (`lower.pl:842-845`); S15/S17 |
  | `nullable_operand_not_narrowed(Goal, Operand)` | compiler | an unnarrowed `nullable(_)` reaching `< =< > >=`, arithmetic, or `concat` |
  | `nullable_aggregate_operand(Kind, Expr)` | compiler | an unnarrowed `nullable(_)` reaching `sum`/`min`/`max`/`avg` |
  | `nullable_ref_column(Ref, Column)` | compiler | `ref(_)` under `nullable(_)`; S16, `slot_nullable_ref_column` |
  | `present_on_total_column(Ref, Column)` | compiler | narrowing a column that cannot be null: dead code, refuse rather than accept a no-op |

- `==` / `\==` on nullable operands lower to `IS` / `IS NOT` (Design D total
  equality). `check_comparison_types(same_type, ...)` (`lower.pl:672-677`) gains the
  explicit nullable clause section 3 identified as silently passing.

**Fail-first receipt**

1. One fail-first fixture per refusal, six total, each red at **both** doors before and
   named-refused after. `nullable_key_column` is the one the lab specifically calls for
   at both gates.
2. `refusal_inventory/1` covers all six with a `prolog:message//1` line each.
3. Fixture `present_narrows_then_compares`: `present(Commit), Commit != 'blocked'`
   compiles and is byte-identical to the oracle. **Before**: `nullable_operand_not_
   narrowed`. **After**: identical.
4. Fixture `unnarrowed_comparison_refused`: the same program without `present(...)`.
   Stays a named refusal forever; this is the fixture that pins Design D's
   "ordered comparison requires present" against a future silent widening.
5. `just green` exit 0; SYNTAX construct table and tmLanguage regenerated and diffed.

**Reversible**: yes, but reverting removes narrowing and re-refuses programs step 7
and step 9 depend on. Revert only as a whole with them.
**Size**: **L; the step most likely to blow its estimate. See section 10.**

---

### Step 7; The json junction

**Files**: whichever lane lands second, per the section 7 contract. If this lane:
`lower.pl` decode lowering, `conformance/body.pl:143-148`, `json_arm.pl` fixtures.

**Change**: exactly the contract in section 7. Presence via `json_type(src, path) IS
NOT NULL`, value via `json_extract(src, path)`, and the six-row destination table.

**Fail-first receipt**

1. Split `decode_missing_key_fails_quietly` (`json_arm.pl:37-45`) into
   `decode_missing_key_fails_quietly` (key genuinely absent) and
   `decode_present_json_null_binds_null` (key present, value JSON null, destination
   `T?`). **Before**: both source shapes are indistinguishable because
   `braces_decode` tests `Value \== none` and the SQL uses `json_extract ... IS NOT
   NULL`. **After**: the first yields no row, the second yields one row holding null,
   at both doors.
2. Fixture `json_null_into_total_column_refused`: destination declared `text`, source
   holding a JSON null -> named refusal at both doors.
3. Fixture `json_none_atom_is_still_text`: the 84 corpus uses of the atom `none` keep
   meaning text. `just conformance` shows zero movement in the 193.

**Reversible**: yes.
**Size**: M.

---

### Step 8; The runtime seam

**Files**: `v6/tsv2/runtime/types.ts`, `1_incremental.ts`, `2_boot.ts`, `rows.ts`,
`ticklog.ts`, `v6/prolog/compile/emit_ts.pl`.

**Change**

- `IRowValue = string | number | boolean | null` (`runtime/types.ts:39`).
  **The typecheck is the fail-first**: widening the union makes `tsgo` enumerate every
  consumer, so the compiler produces the site list.
- Known consumers, from grep at this base sha: `bindArgs` (`1_incremental.ts:30`),
  `bootArgs` (`2_boot.ts:23`), `rowValueFromSql` (`rows.ts:22`, whose header comment
  explicitly says "would need widening only if a future column type introduces
  `bigint` or `null` into this seam"; this is that future), `boundaryDelta`'s bool and
  float coercions (`1_incremental.ts:558-580`), `encodeValue` (`ticklog.ts:38-45`),
  `IRowColumnType` (`runtime/types.ts:43`), `boundary_column_type/2`
  (`emit_ts.pl:588-589`).
- `encodeValue` gains `if (value === null) return "null";` **first**. Without it,
  `canonicalJsonText(value)` does `value[0]` on null and throws.
- `multisetDiff` (`diff.ts`) and `boundaryDelta`'s row key need **no change**
  (measured L3).
- Nullable `bool`/`float` coercions: null passes through as null before the existing
  0/1 and finite checks.

**Fail-first receipt**

1. `just tsv2-test` gains the lab's implementation gate 6 as a unit: assert that
   `null_value(sql)` (oracle), SQL NULL (driver), JS `null` (seam) and JSON `null`
   (tick log) are **one byte sequence** at the public boundary. Before the
   `encodeValue` branch it throws `TypeError`.
2. `tsgo` clean is itself the receipt that every widened site was visited.
3. `just golden-flex`, `just serve-endurance`, `just serve-leak-soak`, `just
   memory-soak` all still exit 0.

**Reversible**: yes.
**Size**: S.

---

### Step 9; The outer-join receipt and the corpus regrade

**Files**: fixtures, `SCOREBOARD.md`, `v6/justfile` expect comments.

**Change**: promote the lab's section 7 comparison as a graded pair; the same question
written as two relations (7.1) and as one nullable column (7.2); and assert they carry
the same information. Refresh the four stale expect comments recorded in section 1.

**Fail-first receipt**

1. Fixture `outer_join_two_rel_and_nullable_agree`: both spellings, both compiled, and
   an assertion that `repo_latest` minus its null rows equals `repo_with_latest` and
   the null rows' keys equal `repo_without_latest`. Before step 6, the nullable half is
   a named refusal.
2. `just green-all` exit 0 with every count refreshed and the delta from the step-1
   baseline stated per gate.

**Reversible**: yes.
**Size**: S.

---

### Step 10; Named residue, not landed

`slot_nullable_host_output`, `slot_nullable_bind_column`, `slot_nullable_ref_column`,
`slot_nullable_key`, `slot_nullable_enum_variant` (section 8). Each is a named refusal
after step 6, so a program reaching one gets a message rather than a wrong answer.

## 10. The step most likely to blow its estimate

**Step 6, the checker.** The other three large candidates all have their unknowns
already retired: step 3's DDL is measured across four receipts, step 4 touches five
call sites of one predicate with the plans measured, and step 8 has `tsgo` produce its
own site list.

Three reasons, all read off the code:

1. **Narrowing is flow-sensitive and the current `Bound` list has no retype
   operation.** `Bound` is a plain association list built by `compile_pattern_arg/7`
   and `compile_guard_goal/3`, threaded left to right and only ever **extended**
   (`lower.pl:250`, `:612`, `:620`). Every consumer reads it with `bound_lookup/3`
   (`:305-306`), which returns the first match. Adding "the same variable now has a
   narrower type from here on" is the first operation in this compiler that **rebinds**
   an existing entry, and `bound_lookup/3`'s first-match semantics means a naive
   shadowing prepend works only if every consumer is downstream of the fold; which is
   true for guards and head expressions and needs checking for the negative-atom pass,
   which runs after guards in some families (`:1642-1644`) and before them in others.
2. **The oracle's narrowing scope is resolution, the compiler's is a linear fold, and
   they are not the same shape.** `body.pl:154-168` solves a conjunction by Prolog
   resolution with backtracking. `present(Value)` inside `not(...)`, inside a match arm,
   or across two arms has a binding scope the compiler's left-to-right fold does not
   model. The update-arm lab already banked `SUGAR-SCOPE` and
   `ARM-SIBLING-WILDCARD` as open slots in precisely this area
   (`plans/2026-07-29-update-arm-verdict.md`), and the design review's A12 recorded the
   compiler refusing `latest` in edge bodies while accepting a wrong program; the same
   class of two-door scope disagreement. Expect at least one refusal in step 6 to be
   *narrower than intended* and one program to need a scope ruling.
3. **Six refusals across two doors is six fail-first fixtures times two doors, and
   check ORDER is fixture data.** `0_program_check.pl`'s header says so explicitly: a
   program violating two classes reports different ones at the two doors, and each
   door declares its own order (`analyze.pl:1085-1155` vs `engine.pl`'s
   `engine_check_order/1`). Placing six new classes into two ordered lists such that
   existing multi-violation fixtures keep their current diagnostic is fiddly, and the
   tick-alignment arc already lost a generic refused-goal catch exactly this way
   (`finalize_in_level_rule` had to be restored shared-side and the drift turned into
   an agreement test).

Mitigation the implementing lane should take: land step 6 as **6a (refusals only, no
narrowing)** and **6b (`present/1` and narrowing)**. 6a is mechanical and independently
green; 6b is where the design residue lives, and if it stalls, the language still has
nullable columns with total equality and no narrowing, which is a coherent though
smaller surface.

## 11. Laws observed by this lane

- One document, no code. `plans/2026-07-30-null-implementation-plan.md` only.
- Read-only against `parse_dl.pl`, `print_dl.pl`, `lower.pl`, `registry.pl`,
  `analyze.pl`; owned by `lane/json-wiring`, never edited here.
- Probes hermetic: in-memory SQLite, `SPREFA_CONFIG=/nonexistent/nullplan.toml
  DL_NO_DAEMON=1`. Nothing under `~/.local/state` read or written, no daemon touched.
- The `@libsql` probe ran against the main tree's already-installed
  `node_modules` (this worktree has none); the probe file was removed after the run.
- No em dashes. No `provenance` / `substrate` / `load-bearing` / `regime`.
- Descriptive names throughout, in prose and in every proposed identifier.
