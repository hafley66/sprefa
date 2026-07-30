# Null coherence lab

## Context

Lab base: `9beeab45314f0d9bd88c31ccc71ae4ac16228517`, verified with
`git rev-parse HEAD` in worktree `codex/null-3vl`.

The user ruling is the authority for this lab:

> "we should just add json/sql nulls and get it over with in a way that is coherent damn yea"

This lab contains no production edits and lands no syntax. The runnable evidence
is [2026-07-30-null-coherence-receipts.mjs](2026-07-30-null-coherence-receipts.mjs).
It runs one SQL matrix through:

- system `sqlite3` 3.43.2
- `@libsql/client` 0.17.4, bundled SQLite 3.45.1

The current split is visible in source:

- JSON decoding says a missing key and the ground atom `none` both fail a bare
  field pattern in `v6/prolog/conformance/body.pl:97-113`.
- Every emitted scalar column has `NOT NULL` in
  `v6/prolog/compile/lower.pl:749-763`.
- The emitted runtime row type excludes null in
  `v6/tsv2/runtime/types.ts:30-46`.
- The older `v6/dl` header already includes JS `null` in `Value` at
  `v6/dl/src/0_types.ts:34`, while its column type has only `text | int`.
  That declaration does not define query semantics, key behavior, or tick-log
  identity.

The minimum coherent implementation price for the recommended design is **12
semantic seams**:

1. surface token and parser IR
2. nullable type constructor and inference
3. nullable-key refusal
4. JSON path presence lowering
5. oracle ground-value representation
6. oracle equality and narrowing
7. nullable SQLite DDL
8. type-directed SQL equality in joins, negation, deletion, and support lookup
9. aggregate and ordering refusals before narrowing
10. driver row decoding and binding
11. boundary diff and tick-log encoding
12. two-door conformance fixtures

The SQL evidence has 14 cases run against 2 builds, for **28 executed
assertions**. This count excludes the future oracle/emitter parity fixtures.

## 1. JSON1 absent versus JSON null

Run:

```sh
SPREFA_CONFIG=/nonexistent/null-coherence.toml \
DL_NO_DAEMON=1 \
node plans/2026-07-30-null-coherence-receipts.mjs
```

Receipt J1 produced the same rows on both builds:

| document | `json_extract(document,'$.k')` | `json_type(document,'$.k')` | `document -> '$.k'` | `document ->> '$.k'` | matching `json_each` rows |
|---|---|---|---|---|---:|
| `{}` | SQL NULL | SQL NULL | SQL NULL | SQL NULL | 0 |
| `{"k":null}` | SQL NULL | text `null` | text `null` | SQL NULL | 1 |

`json_extract/2` and `->>` collapse the two cases. `json_type/2`, `->`, and
the existence of a `json_each` row preserve the distinction. The
[SQLite JSON1 reference](https://www.sqlite.org/json1.html) documents the same
rules: a missing path returns SQL NULL, a JSON null extracted as an SQL scalar
also returns SQL NULL, `json_type` returns text `null` only for the present JSON
null, and `->` returns JSON text.

### Decode contract

A JSON field decode has two outputs:

```text
presence := json_type(source, path) IS NOT NULL
value    := json_extract(source, path)
```

The language contract:

| source state | destination `text` | destination `text?` |
|---|---|---|
| key absent | body pattern fails, no row | body pattern fails, no row |
| key present with JSON null | type error unless explicitly narrowed away | bind language null |
| key present with JSON string | bind string | bind present string |

The current lowering in the JSON syntax lab uses
`json_extract(...) IS NOT NULL`. That predicate discards both absent and
present-null fields. A nullable destination therefore requires a presence
predicate based on `json_type`, followed by a separate value projection.

For object fan-out, `json_each` already carries the required presence bit as a
row. Its `type` column says `null` and its `value` column is SQL NULL. For an
exact path, `json_type/2` supplies the same information without a join.

## 2. Three-valued logic cost

Receipts L1 through L6 agree on both SQLite builds.

| construct | executed result | language and emitter consequence |
|---|---|---|
| `NULL = NULL` | UNKNOWN, represented as SQL NULL | A body filter drops the row. A join does not match two nulls. |
| `NULL != NULL` | UNKNOWN | A negative comparison also drops the row. It does not establish difference. |
| `NULL IS NULL` | true | SQLite-specific null test with a Boolean result. |
| `NULL IS NOT NULL` | false | SQLite-specific presence test with a Boolean result. |
| `NULL IS NOT DISTINCT FROM NULL` | true | Standard null-safe equality. This is the SQL spelling needed for language-level total equality. |
| `1 IS NOT DISTINCT FROM NULL` | false | Null-safe equality stays two-valued. |
| `NOT NULL` | UNKNOWN | Logical negation preserves UNKNOWN. |
| `NULL AND false` | false | False determines conjunction. |
| `NULL AND true` | UNKNOWN | The nullable operand remains visible. |
| `NULL OR false` | UNKNOWN | The nullable operand remains visible. |
| `NULL OR true` | true | True determines disjunction. |
| `NOT EXISTS (... inner.value = outer.value)` with outer NULL and an inner NULL | true | The inner equality is UNKNOWN, so the subquery has zero qualifying rows. Negation reports absence although a null-bearing row exists. |
| the same `NOT EXISTS` with `IS NOT DISTINCT FROM` | false | Null-safe identity makes negation agree with set membership. |
| `GROUP BY nullable_column` | all nulls form one group | Group identity is null-safe even though `=` is not. |
| `SUM(nullable_column)` | skips nulls; all-null input returns NULL | Result nullability grows. |
| `COUNT(nullable_column)` | counts non-null values | It differs from `COUNT(*)`, which counts rows. |
| `MIN` / `MAX` | skip nulls; all-null input returns NULL | Result nullability grows. |
| `ORDER BY value ASC` | null first | SQLite treats null as smaller than every other value. |
| `ORDER BY value DESC` | null last | Explicit `NULLS FIRST` and `NULLS LAST` can override placement. |
| `SELECT DISTINCT value` | duplicate nulls collapse | DISTINCT uses null-safe duplicate identity. |
| equality join on two nulls | zero matches | Receipt L6 gets 0 equality matches and 4 null-safe matches for two null rows on each side. |

SQLite specifies these behaviors in its
[expression rules](https://sqlite.org/lang_expr.html),
[aggregate reference](https://www.sqlite.org/lang_aggfunc.html), and
[SELECT rules](https://www.sqlite.org/lang_select.html). SQLite's own
[NULL handling note](https://sqlite.org/nulls.html) calls the split between
UNIQUE and DISTINCT arbitrary and puzzling.

### Effect on the current negation lowering

The compiler emits `NOT EXISTS` for a negated body atom. Equality conditions
inside it come from `where_text/2` in
`v6/prolog/compile/lower.pl:308-312`. With nullable columns, the current form:

```sql
NOT EXISTS (
  SELECT 1
  FROM "latest" candidate
  WHERE candidate."repo" = source."repo"
    AND candidate."commit" = source."commit"
)
```

reports true when both commit columns are NULL. The null-safe form:

```sql
NOT EXISTS (
  SELECT 1
  FROM "latest" candidate
  WHERE candidate."repo" IS NOT DISTINCT FROM source."repo"
    AND candidate."commit" IS NOT DISTINCT FROM source."commit"
)
```

matches the oracle's ground-row membership and the B-plane set model.

## 3. Keys and deltas

### 3.1 Executed key behavior

| receipt | schema and write | result on 3.43.2 and 3.45.1 |
|---|---|---|
| K1 | `UNIQUE(key_value)`, insert two NULL keys | 2 rows |
| K2 | `UNIQUE(key_value)`, current `ON CONFLICT DO UPDATE` shape | 2 rows, `old` and `new` |
| K3 | `UNIQUE(key_value)`, `INSERT OR REPLACE` | 2 rows, `old` and `new` |
| K4 | ordinary rowid table with `TEXT PRIMARY KEY` and `INSERT OR REPLACE` | 2 rows |
| K5 | `PRIMARY KEY(key_value) WITHOUT ROWID` | insert fails with `NOT NULL constraint failed` |
| K6 | lookup with `key_value = NULL` | 0 matches; null-safe lookup gets 1 |

The current base emits `ON CONFLICT ... DO UPDATE` for arrival and edge writes
at `v6/prolog/compile/lower.pl:1286-1305` and
`v6/prolog/compile/lower.pl:1488-1508`. K2 measures that exact conflict
behavior. K3 measures the requested `INSERT OR REPLACE` behavior. Both
accumulate rows when the conflict key is NULL.

The table families differ:

- Ordinary set tables use a composite primary key with `WITHOUT ROWID` at
  `v6/prolog/compile/lower.pl:737-740`. A nullable key is rejected.
- Declared relation-value tables use `__id INTEGER PRIMARY KEY` plus
  `UNIQUE(key columns)` at `v6/prolog/compile/lower.pl:726-736`. A nullable key
  accumulates.
- Pre-state and support tables also use `WITHOUT ROWID` primary keys at
  `v6/prolog/compile/lower.pl:2309-2325` and `:2368-2379`.

Removing `NOT NULL` from column DDL therefore creates one semantic row with two
physical outcomes: rejection in one table family and accumulation in another.

### Key contract

For the recommended design:

1. A nullable column cannot appear in a declared key.
2. A declared key remains total and preserves the current one-row-per-key B
   invariant.
3. Non-key nullable columns use null-safe row equality when deciding whether a
   keyed write changed the row.
4. A changed non-key nullable column produces replacement deltas in boundary
   order: `-old`, then `+new`.

The checker refusal should name the relation, column, and key position:

```text
nullable_key_column(open_repo/2, commit, 2)
```

An alternative implementation can support nullable keys only by defining
`NULLS NOT DISTINCT` key identity, creating a matching uniqueness mechanism,
and making every conflict, lookup, deletion, pre-state, support, and oracle key
comparison use that identity. SQLite's default UNIQUE behavior cannot implement
that contract.

### 3.2 Boundary-diff contract

Receipt D1 stages signed rows and groups them by row value:

| transition | staged rows | boundary result |
|---|---|---|
| null -> null | `- [repo,null]`, `+ [repo,null]` | zero delta |
| null -> `"commit-1"` | `- [repo,null]`, `+ [repo,"commit-1"]` | one removal and one addition |
| value -> null | symmetric case | one removal and one addition |

This is a cross-target contract:

```text
same row identity: every column is null-safe equal
null -> null:       no B-plane change, no Z-plane event
null -> value:      -old then +new
value -> null:      -old then +new
```

`GROUP BY` already places equal null-bearing staged rows in one group. The TS
runtime then nets weights by serialized row at
`v6/tsv2/runtime/1_incremental.ts:559-595`. `multisetDiff` uses
`JSON.stringify(row)` at `v6/tsv2/runtime/diff.ts:18-58`, where JS null has a
stable array encoding.

The tick model survives null only when row identity stays total. Rule
comparisons may use a separate logic, but B membership and Z differentiation
cannot return UNKNOWN. `v6/prolog/compile/TICK-MODEL.md` requires the delta
stream to be the discrete derivative of the B relation. UNKNOWN row identity
would make the derivative undefined.

## 4. Two-door representation

### 4.1 Ground representation

The oracle representation should be the reserved compound term:

```prolog
null_value(sql)
```

Properties:

- It is ground.
- It never unifies with an unbound variable by accident beyond ordinary value
  binding.
- It differs from failure.
- It differs from the existing atom `none`, which is used as an ordinary text
  sentinel across current fixtures and is also treated as JSON field absence
  in `body.pl:110-113`.
- It differs from the existing atom `null`, which currently renders as the
  JSON string `"null"` in the JSON interop receipt.
- The parser owns the reserved term. Quoted `'null'` remains text.

Representation table for the recommended design:

| boundary | present value | null value | missing JSON key |
|---|---|---|---|
| surface | `'commit-1'` | `null` | no field |
| oracle IR | atom/string value | `null_value(sql)` | failed lookup |
| SQLite | TEXT | SQL NULL | no source row or false presence predicate |
| TS driver | string | JS `null` | no projected row |
| tick-log JSON | `"commit-1"` | `null` | no row |

Required oracle clauses:

```prolog
json_canon(json_null_token, null_value(sql)).
json_decode(null_value(sql), null_value(sql)).
value_json(null_value(sql), 'null').
```

The first clause's input name is illustrative IR. The parser must produce a
reserved token so unquoted surface `null` differs from quoted text `'null'`.

### 4.2 Equality and solving

For the recommended total-equality design:

```prolog
null_value(sql) == null_value(sql).       % true
null_value(sql) \== present_value.        % true
is_null(null_value(sql)).                 % succeeds
present(null_value(sql)).                 % fails
```

Ordered comparison, arithmetic, string concatenation, and aggregate input over
a maybe-null value require a preceding `present(Value)` narrowing. The checker
can see that narrowing left to right, matching the current body-solving order.

For a SQL-3VL candidate, the oracle also needs a ground truth value
`truth_unknown` and comparison predicates returning one of
`truth_true | truth_false | truth_unknown`. Body filtering retains only
`truth_true`. This adds a truth domain alongside the null value domain.

### 4.3 Sign decomposition

`null_value(sql)` is an ordinary ground column value inside a row. The tick
phases remain:

```text
positive arrival: +row, including +[repo,null]
departure:        -row, including -[repo,null]
update:           -old followed by +new
complete:         the scope relation's own negative row
```

The oracle's `sort`, `memberchk`, set difference, and keyed `key_of` operations
all see a stable ground term. The emitter must preserve the same identity using
null-safe SQL comparisons and a tick-log encoder that renders JS null as JSON
`null`. The current TS tick encoder calls string-only
`canonicalJsonText(value)` after its number and Boolean branches at
`v6/tsv2/runtime/ticklog.ts:38-45`; widening `IRowValue` without adding a null
branch would throw on `value[0]`.

## 5. Prior art

### Classical Datalog

The classical meaning is a least model made of ground facts. Absence means a
ground atom is outside that model. Van Emden and Kowalski define the model,
fixpoint, and operational semantics of predicate-logic programs in
[The Semantics of Predicate Logic as a Programming Language](https://www.doc.ic.ac.uk/~rak/papers/kowalski-van_emden.pdf).
SQL-style UNKNOWN comparison is an additional semantic domain.

### Soufflé

Soufflé relations are sets of typed tuples whose elements belong to declared
domains, and its constraints return true or false. Its scalar domains have no
SQL nullable modifier. The exact nuance is that every **record** type has a
ground `nil` value, while ADTs require an explicit empty branch and cannot use
record `nil`. See the official [types](https://souffle-lang.github.io/types),
[relations](https://souffle-lang.github.io/relations), and
[constraints](https://souffle-lang.github.io/constraints) references.

Soufflé therefore supplies prior art for a typed empty value with ordinary
two-valued rule constraints. Its relation columns still use two-valued
constraints rather than SQL 3VL.

### Datomic

Datomic models ordinary optional attributes by absence of an E/A/V datom.
Queries can test `missing?`, `get-else` supplies a default, and pull omits
missing attributes. See the official
[query reference](https://docs.datomic.com/query/query-data-reference.html),
[pull missing-attribute rules](https://docs.datomic.com/query/query-pull.html),
and [outer-join note](https://docs.datomic.com/tech-notes/outer-joins.html).

The exact nuance is that Datomic permits `nil` in tuple slots, including
generated composite tuples for missing constituents. Its
[schema reference](https://docs.datomic.com/schema/schema-reference.html)
states that tuple nil sorts below all other values. Datomic's ordinary entity
attributes still use row absence.

### Flix

Flix fixpoint relations are strongly typed. Optional values use the explicit
`Option[t]` sum with `None` and `Some(t)`, and equality over Option can be
defined exhaustively with a Boolean result. See the official
[fixpoint documentation](https://doc.flix.dev/fixpoints.html),
[immutable data types](https://doc.flix.dev/immutable-data.html), and
[Option equality example](https://doc.flix.dev/traits.html).

### SQL and SQLite

SQL uses NULL plus 3VL. SQLite then applies separate null identity rules for
comparison, grouping, DISTINCT, and UNIQUE. Its own NULL note records the
UNIQUE versus DISTINCT split as arbitrary and puzzling. Database research has
also shown that a two-valued SQL can preserve expressiveness:
[Handling SQL Nulls with Two-Valued Logic](https://arxiv.org/abs/2012.13198).
That paper covers core SQL with subqueries, grouping, and recursion.

[SEQUEL 2](https://www.fsmwarden.com/relationnel/Sequel_2%281976%29Chamberlin_et_Cie.pdf)
records the original 1976 design: comparisons involving null produce UNKNOWN,
WHERE retains only TRUE, built-in functions ignore null, and an explicit null
test is an exception. Wang's
[No More Nulls!](https://arxiv.org/abs/2307.15751) reports that SQL
co-inventor Don Chamberlin later lamented the bugs caused by nulls in engines
and applications, citing his 2023 *49 Years of Queries* keynote. The reported
regret concerns the bug cost. Chamberlin has also defended null as a pragmatic
missing-information feature.

### Finding

Comparable systems contain several typed empty markers:

- Soufflé has typed record `nil`.
- Datomic tuples can contain `nil`.
- Flix has explicit `Option`.
- Classical Datalog and ordinary Datomic facts use absence.

They avoid making SQL 3VL the implicit logic of every rule comparison. The
smallest common pattern is a declared or typed empty value plus total
membership identity.

## 6. Smallest coherent designs

All syntax below is candidate spelling for analysis. No syntax lands in this
lab.

### Design A: JSON null stays inside JSON; columns stay total

Worked program:

```dl
rel response(body: json).
rel decoded_name(name: text).

decoded_name(Name) <-
  response(Body),
  decode(Body, {name: Name}).

# present JSON null at $.name:
# json_null_into_nonnullable(decoded_name/1, name)
```

Optionality uses two relations:

```dl
rel repo(name: text).
rel latest(repo: text, commit: text).
rel repo_with_latest(repo: text, commit: text).
rel repo_without_latest(repo: text).

repo_with_latest(Repo, Commit) <-
  repo(Repo),
  latest(Repo, Commit).

repo_without_latest(Repo) <-
  repo(Repo),
  not(latest(Repo, _)).
```

Pure RxJS lowering:

```ts
const partition$ = combineLatest([repoRows$, latestRows$]).pipe(
  map(([repoRows, latestRows]) => {
    const latestByRepo = new Map(
      latestRows.map(([repo, commit]) => [repo, commit]),
    );
    return {
      withLatest: repoRows.flatMap(([repo]) => {
        const commit = latestByRepo.get(repo);
        return commit === undefined ? [] : [[repo, commit] as const];
      }),
      withoutLatest: repoRows.flatMap(([repo]) =>
        latestByRepo.has(repo) ? [] : [[repo] as const]
      ),
    };
  }),
);
```

Checker gain: every relation column remains total. JSON decode must track path
presence and refuse a JSON null flowing to a total head column.

Rule-body cost: consumers branch by relation name.

Outer-join shape: expressible as two relations. A single fixed-arity output row
cannot carry the missing commit.

Delta and tick log: current contracts remain. The only new JSON work is the
absent-versus-null distinction and named refusal.

Tier classification: **(a)** for column optionality, because two-rel spelling
already exists; JSON null storage already exists.

### Design B: marked nullable columns with SQL 3VL

Worked program:

```dl
rel repo(name: text).
rel latest(repo: text, commit: text).
rel repo_latest(repo: text, commit: text?).

repo_latest(Repo, Commit) <-
  repo(Repo),
  latest(Repo, Commit).

repo_latest(Repo, null) <-
  repo(Repo),
  not(latest(Repo, _)).

rel missing_latest(repo: text).
missing_latest(Repo) <-
  repo_latest(Repo, Commit),
  Commit is null.
```

`Commit == null` evaluates UNKNOWN under this design, so the body needs
`Commit is null`. `Commit != 'blocked'` also drops null-bearing rows.

Pure RxJS lowering using `undefined`:

```ts
type SqlMaybe<Value> = Value | undefined;
type RepoLatest = readonly [repo: string, commit: SqlMaybe<string>];

const repoLatest$ = combineLatest([repoRows$, latestRows$]).pipe(
  map(([repoRows, latestRows]): readonly RepoLatest[] => {
    const latestByRepo = new Map(latestRows);
    return repoRows.map(([repo]) => [repo, latestByRepo.get(repo)]);
  }),
);
```

The SQL adapter maps `undefined` to SQL NULL and the tick encoder maps it to
JSON null. Array `JSON.stringify` already converts undefined to null, while
object `JSON.stringify` drops undefined properties. A row-only representation
must therefore be enforced.

Checker gain: nullability is local to `T?`; expressions propagate nullable
types; `is null` and `is not null` narrow.

Rule-body cost: authors need SQL null tests and must remember UNKNOWN behavior
for every equality, inequality, and negated atom touching a nullable value.

Outer-join shape: one relation is expressible.

Delta and tick log: membership identity still needs null-safe equality,
independent of rule comparison 3VL. Nullable keys require refusal or a separate
key design.

Tier classification: **(c)**.

### Design C: full SQL 3VL for every column

Worked program:

```dl
rel repo(name: text).
rel latest(repo: text, commit: text).
rel repo_latest(repo: text, commit: text).

repo_latest(Repo, Commit) <-
  repo(Repo),
  latest(Repo, Commit).

repo_latest(Repo, null) <-
  repo(Repo),
  not(latest(Repo, _)).

# Every declared column above accepts null.
```

Pure RxJS lowering with an explicit SQL truth domain:

```ts
type SqlValue<Value> = Value | null;
type SqlTruth = true | false | null;

const sqlEqual = <Value>(
  left: SqlValue<Value>,
  right: SqlValue<Value>,
): SqlTruth =>
  left === null || right === null ? null : left === right;

const filterSqlTruth = <Row>(
  rows: readonly Row[],
  predicate: (row: Row) => SqlTruth,
): readonly Row[] => rows.filter((row) => predicate(row) === true);

const selected$ = repoLatest$.pipe(
  map((rows) =>
    filterSqlTruth(rows, ([, commit]) => sqlEqual(commit, "commit-1"))
  ),
);
```

Checker gain: no nullable annotation is required. Every operator result becomes
potentially nullable, so the checker must propagate null through the entire
expression graph.

Rule-body cost: every comparison and negation inherits UNKNOWN. Existing
programs acquire new outcomes after any source admits null.

Outer-join shape: one relation is expressible.

Delta and tick log: all keys become potentially nullable. The current keyed
replace contract cannot use SQLite UNIQUE defaults, and every internal equality
must become type-independent null-safe identity even while surface equality
stays 3VL.

Tier classification: **(c)**.

### Design D: marked nullable columns with total language equality

This is the recommended design.

Worked program:

```dl
rel repo(name: text).
rel latest(repo: text, commit: text).
rel repo_latest(repo: text, commit: text?).

repo_latest(Repo, Commit) <-
  repo(Repo),
  latest(Repo, Commit).

repo_latest(Repo, null) <-
  repo(Repo),
  not(latest(Repo, _)).

rel missing_latest(repo: text).
missing_latest(Repo) <-
  repo_latest(Repo, Commit),
  Commit == null.

rel selected_latest(repo: text, commit: text).
selected_latest(Repo, Commit) <-
  repo_latest(Repo, Commit),
  present(Commit),
  Commit != 'blocked'.
```

Semantics:

```text
null == null       true
null != null       false
null == value      false
null != value      true
ordered comparison requires present(value)
arithmetic requires present(value)
aggregate input requires present(value)
```

SQL equality lowers to `IS NOT DISTINCT FROM`; inequality lowers to
`IS DISTINCT FROM`. `present(Value)` lowers to `Value IS NOT NULL`.

Pure RxJS lowering using JS null as the distinguished empty:

```ts
type Nullable<Value> = Value | null;
type RepoLatest = readonly [repo: string, commit: Nullable<string>];

const repoLatest$ = combineLatest([repoRows$, latestRows$]).pipe(
  map(([repoRows, latestRows]): readonly RepoLatest[] => {
    const latestByRepo = new Map(latestRows);
    return repoRows.map(([repo]) => [
      repo,
      latestByRepo.get(repo) ?? null,
    ]);
  }),
);

const missingLatest$ = repoLatest$.pipe(
  map((rows) => rows.filter(([, commit]) => commit === null)),
);

const selectedLatest$ = repoLatest$.pipe(
  map((rows) =>
    rows.flatMap(([repo, commit]) =>
      commit === null || commit === "blocked" ? [] : [[repo, commit] as const]
    )
  ),
);
```

Checker gain: `T?` localizes the new value domain. `present` narrows `T?` to
`T`. Keys reject `T?`. Ordered operations, arithmetic, concatenation, and
aggregates reject an unnarrowed `T?`.

Rule-body cost: equality remains Boolean. Operations that require a present
payload need one explicit narrowing.

Outer-join shape: one relation is expressible.

Delta and tick log: null-safe structural identity matches `GROUP BY`,
`DISTINCT`, Prolog ground-term equality, JS row encoding, and the tick model.
SQL 3VL remains an implementation detail at the SQLite boundary.

Tier classification: **(c)**.

### Design E: explicit Option-shaped columns

Worked program:

```dl
rel repo(name: text).
rel latest(repo: text, commit: text).
rel repo_latest(repo: text, commit: option(text)).

repo_latest(Repo, some(Commit)) <-
  repo(Repo),
  latest(Repo, Commit).

repo_latest(Repo, none) <-
  repo(Repo),
  not(latest(Repo, _)).
```

Pure RxJS lowering:

```ts
type Option<Value> =
  | { readonly tag: "none" }
  | { readonly tag: "some"; readonly value: Value };

const repoLatest$ = combineLatest([repoRows$, latestRows$]).pipe(
  map(([repoRows, latestRows]) => {
    const latestByRepo = new Map(latestRows);
    return repoRows.map(([repo]) => {
      const commit = latestByRepo.get(repo);
      const option: Option<string> = commit === undefined
        ? { tag: "none" }
        : { tag: "some", value: commit };
      return [repo, option] as const;
    });
  }),
);
```

Checker gain: pattern matching narrows exhaustively.

Rule-body cost: construction and matching use `some` and `none`.

Outer-join shape: one relation is expressible.

Delta and tick log: SQL NULL may encode `none`, but the row-key and tick-log
encoders need semantic Option encoding so `{tag:"none"}` becomes JSON null and
must be byte-identical across targets. An enum-as-separate-relations implementation
uses existing machinery but changes the output into multiple relations.

Tier classification: a column-valued Option is **(c)**; variant relations are
**(a)**.

### Design comparison

| design | one-row outer shape | surface comparison | nullable keys | JSON absent versus null | implementation seams |
|---|---|---|---|---|---:|
| A JSON-only | no | current 2VL | impossible | presence check plus refusal | 3 |
| B marked 3VL | yes | 3VL on `T?` | refuse or redesign | preserved | 13 |
| C full 3VL | yes | 3VL everywhere | redesign required | preserved | 16 |
| D marked total equality | yes | 2VL, explicit narrowing | refused | preserved | 12 |
| E Option column | yes | 2VL by constructor | checker choice | preserved | 14 |

The seam counts measure semantic obligations; line count is outside this lab.
A seam may touch multiple predicates and tests. In `lower.pl` alone, equality text is generated
by `where_text/2`, guard binding, `key_join_equalities/5`,
`eq_placeholder/2`, `qualified_equalities/4`,
`delta_reference_identity/8`, and boot key lookup. Nullable support must be
type-directed across these families.

## 7. Outer-join comparison

Question: every repo, with its latest commit if any.

### 7.1 Two relations

```dl
rel repo_with_latest(repo: text, commit: text).
rel repo_without_latest(repo: text).

repo_with_latest(Repo, Commit) <-
  repo(Repo),
  latest(Repo, Commit).

repo_without_latest(Repo) <-
  repo(Repo),
  not(latest(Repo, _)).
```

Result:

```text
repo_with_latest("alpha", "a1")
repo_without_latest("beta")
```

Cost: every downstream consumer has two input relations and two rule arms.
The tick log carries changes under two relation names.

### 7.2 Nullable column

```dl
rel repo_latest(repo: text, commit: text?).

repo_latest(Repo, Commit) <-
  repo(Repo),
  latest(Repo, Commit).

repo_latest(Repo, null) <-
  repo(Repo),
  not(latest(Repo, _)).
```

Result:

```text
repo_latest("alpha", "a1")
repo_latest("beta", null)
```

Cost: one result relation; nullability enters the type and every consumer of
`commit`. Design D confines that cost through `present`.

### 7.3 Explicit variants

```dl
rel repo_latest_some(repo: text, commit: text).
rel repo_latest_none(repo: text).
rel repo_latest_tag(repo: text, tag: latest_tag).

repo_latest_some(Repo, Commit) <-
  repo(Repo),
  latest(Repo, Commit).

repo_latest_none(Repo) <-
  repo(Repo),
  not(latest(Repo, _)).

repo_latest_tag(Repo, some) <- repo_latest_some(Repo, _).
repo_latest_tag(Repo, none) <- repo_latest_none(Repo).
```

Result:

```text
repo_latest_some("alpha", "a1")
repo_latest_none("beta")
```

Cost: exhaustive variant identity is explicit. Payload access still branches
by relation because current enum machinery expands variants into relations.

The nullable form removes one relation and one consumer arm. That is the
expressive win supplied by a null-bearing column.

## 8. Tier-0 verdict

The test:

- **(a)** sugar over an existing lowering
- **(b)** a new lowering of existing semantics
- **(c)** genuinely new semantics

Results:

| construct | class | reason |
|---|---|---|
| JSON null stored inside a JSON value | (a) | JSON text and JSON1 already carry it. |
| absent-versus-null JSON path test | (b) | `json_type/2`, `->`, and `json_each` already expose presence. |
| optional output as two relations | (a) | Current relations and negation express it. |
| null as a relation-column value | **(c)** | One B-plane row gains a value absent from every current column domain. |
| single-relation outer-join result | **(c)** | Current fixed-arity total rows cannot express the missing payload. |
| SQL 3VL comparisons | **(c)** | A new truth result UNKNOWN enters rule solving. |
| marked nullable column with total equality | **(c)** | The value domain grows while the truth domain stays Boolean. |

**Column null is tier 0.** Accepting Design D would accept the project's first
tier-0 surface construct identified by these labs. The new semantics are the
null-bearing column and its single-relation outer result. JSON storage and path
presence are existing mechanisms.

## 9. Decisions

Recommendation: **Design D, marked nullable columns with total language
equality**.

Reasons:

1. It satisfies the ruled JSON-to-SQL bridge. A present JSON null decodes to a
   nullable SQL column; an absent key produces no binding.
2. It supplies the one-relation outer-join result.
3. It keeps B-plane identity, Z-plane differentiation, Prolog ground-term
   identity, SQLite grouping, JS row keys, and tick-log bytes on one total
   equality.
4. It confines nullable flow to `T?`.
5. It lets the checker refuse nullable keys and unnarrowed ordered operations.
6. It avoids a second truth domain in the rule engine.

Alternative dispositions:

- Design A retains JSON null and two-rel optionality. It does not satisfy the
  ruled SQL-column half.
- Design B satisfies the ruling and imports UNKNOWN into comparisons on `T?`.
- Design C gives every existing column and comparison a new domain.
- Design E gives explicit case analysis and costs more boundary encoding than
  Design D.

## 10. Numbered cards

Each card records at least two bounded choices. Recommended choices are marked.

1. **Nullable type spelling**

   - A: `commit: text?` **recommended**
   - B: `commit: nullable(text)`
   - C: `commit: option(text)`

2. **Surface null token**

   - A: unquoted `null`; quoted `'null'` stays text **recommended**
   - B: `none`; conflicts with existing ordinary `none` values
   - C: `NULL`; SQL-style casing

3. **Language equality**

   - A: total equality, lowered with `IS NOT DISTINCT FROM` **recommended**
   - B: SQL 3VL on marked columns
   - C: SQL 3VL on every column

4. **Presence narrowing**

   - A: `present(Commit)` **recommended**
   - B: `Commit is not null`
   - C: `some(Commit)`

5. **Nullable key policy**

   - A: checker refusal `nullable_key_column(...)` **recommended**
   - B: nulls-not-distinct keys with matching indexes and conflict SQL
   - C: SQLite UNIQUE semantics, which permits multiple null keys

6. **JSON field decode**

   - A: implicit path presence via `json_type/2`, then value projection
     **recommended**
   - B: explicit `has(Body, '$.commit')` followed by `decode`
   - C: collapse absent and JSON null

7. **JSON null into a total column**

   - A: checker rejects the nullable flow before emission **recommended**
   - B: runtime diagnostic row
   - C: quiet body failure

8. **Aggregates over `T?`**

   - A: require `present` before `sum`, `min`, `max`, and `count(value)`
     **recommended**
   - B: inherit SQL skip-null behavior and nullable aggregate results
   - C: add null-handling arguments to each aggregate

9. **Ordering over `T?`**

   - A: require `present` before ordered comparison and sorting **recommended**
   - B: declaration-level `nulls first | last`
   - C: inherit SQLite null-first ascending order

10. **Runtime row representation**

    - A: JS `null` as `IRowValue` **recommended**
    - B: JS `undefined`, restricted to row arrays
    - C: tagged `Option`

11. **Oracle representation**

    - A: reserved ground `null_value(sql)` **recommended**
    - B: atom `null`, which collides with existing text rendering
    - C: unbound variable, which changes absence and unification

12. **Outer-result spelling**

    - A: two rules into one `T?` relation **recommended**
    - B: `left_join(repo(...), latest(...))`
    - C: two output relations

13. **Tick-log encoding**

    - A: JSON literal `null` in the existing row array **recommended**
    - B: tagged object `{"none":true}`
    - C: text `"null"`

14. **Candidate selection**

    - A: Design D, marked null plus total equality **recommended**
    - B: Design B, marked null plus SQL 3VL
    - C: Design C, full SQL 3VL

## Verification

Completed:

```text
git rev-parse HEAD
9beeab45314f0d9bd88c31ccc71ae4ac16228517

SPREFA_CONFIG=/nonexistent/null-coherence.toml
DL_NO_DAEMON=1
node plans/2026-07-30-null-coherence-receipts.mjs

sqlite3 CLI: 3.43.2
@libsql SQLite: 3.45.1
28 dual-engine assertions: PASS
```

Implementation gates for a future production arc:

1. Duplicate all 14 receipt cases in oracle/emitter byte-parity fixtures.
2. Add a present JSON-null fixture and a missing-key fixture in the same
   program.
3. Add nullable-key fail-first fixtures at both program gates.
4. Add null -> null, null -> value, and value -> null tick-log goldens.
5. Add sabotage fixtures replacing one null-safe join and one null-safe
   `NOT EXISTS` condition with `=`.
6. Add a tick-log encoder unit proving `null_value(sql)`, SQL NULL, JS null,
   and JSON `null` are byte-identical at the public boundary.

## Staffing

- Work type: research lab and plan only
- Worktree: yes, `codex/null-3vl`
- Base SHA: `9beeab45314f0d9bd88c31ccc71ae4ac16228517`
- Agent: Codex
- Production implementation: unstaffed pending card rulings
- Executed suite budget: 14 cases x 2 SQLite builds = 28 assertions
- Git actions: no commit, merge, or push
