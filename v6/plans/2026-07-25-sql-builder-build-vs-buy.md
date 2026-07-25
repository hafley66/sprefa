# SQL query-builder / templating: build-vs-buy for `lowerSql.ts`

Consumer: `v6/sprefa-store/js/src/lower/lowerSql.ts` (`DatalogEvaluator`, `RecursiveStratum`),
440 lines, generates 9 SQL statement shapes as template strings against tables/columns that
exist only at runtime (`RelTables = ReadonlyMap<string, {table, columns: readonly string[]}>`,
`lower/types.ts:9-14`). Driver: `@libsql/client@0.17.4` `Client` (`engine/types.ts:38`), executed
through `SqlRunner.execute(db, statement, trace)` (`engine/sqlRunner.ts:16-22`), which already
accepts either a bare string or `{sql, args}` (`SqlStatement = InStatement`, `engine/types.ts:41`).

No production code path in this tree was found that threads `.dl`-authored rel/column names
from `dl/src/0_ast_bridge.ts`'s `bridge()` into a `RelTables` instance — only
`labs/stress.ts:517,607` builds rel tables, from synthetic names. Flagged in "could not verify."

## Comparison table

| Candidate | Version | Weekly DL | Last publish | License | Install weight | SQLite | Dynamic schema | Owns connection? |
|---|---|---|---|---|---|---|---|---|
| Kysely | 0.29.4 | 12.1M | 2026-07-17 | MIT | 1.7MB, 610 files, 0 deps | via dialect pkg | first-class (`db.dynamic`) | no, dialect wraps a driver |
| Drizzle ORM | 0.45.2 | 16.4M | 2026-03-27 | Apache-2.0 | 10.4MB, 2666 files, 0 deps | built-in (`sqlite-core`) | function-based, but needs column *types* we don't have | no |
| Knex | 3.3.0 | 4.9M | 2026-06-26 | MIT | 941KB, 183 files, 13 deps (incl. `tarn` pool) | via `better-sqlite3`/`sqlite3` client | native (string-first API) | yes, by default (escape hatch: `.toSQL()`) |
| sql-template-tag | 5.2.1 | 56K | 2024-04-20 | MIT | 21KB, 0 deps | dialect-agnostic (plain `?`) | yes (that's all it is) | no |
| @databases/sql | 3.3.0 | 93.6K | 2023-01-12 | MIT | 65KB, 0 deps | needs a formatter you write | yes | no |
| @databases/escape-identifier | 1.0.3 | 89.8K | 2022-02-07 | MIT | 51KB, 1 dep | `escapeSQLiteIdentifier()` built in | n/a (identifier-only) | no |
| Slonik | 49.10.7 | 146.7K | 2026-06-16 | BSD-3-Clause | 1.0MB, 389 files | **no** — `@slonik/pg-driver` is a hard dep | n/a | yes |
| squel | 6.3.1 | 18K | 2026-06-17 | MIT | 532KB, 23 files, 0 deps | generic, no SQLite specifics found | yes (string-first) | no |
| TypeORM | 1.1.0 | 5.0M | 2026-07-13 | MIT | large (full ORM) | yes | no — entity classes are the whole model | yes |
| node-sql-parser | 5.4.0 | 1.6M | 2026-01-12 | Apache-2.0 | — | — | — | it's a **parser**, not a builder — wrong tool |

Shape support (from source, not docs prose):

| Candidate | INSERT…SELECT | EXCEPT | NOT EXISTS subquery | WITHOUT ROWID DDL | INSERT OR IGNORE | HAVING | n-way self-join, generated aliases |
|---|---|---|---|---|---|---|---|
| Kysely | `.expression(eb => eb.selectFrom(...))` | `.except()` | `eb.not(eb.exists(...))` | no native modifier; `.modifyEnd(sql\`without rowid\`)` escape hatch | no `OR IGNORE`; `.onConflict(oc => oc.columns(pk).doNothing())` — semantically identical on our WITHOUT ROWID tables, since PK is the only constraint | `.having()` | `.selectFrom([table(rel1).as('b0'), table(rel2).as('b1')])` |
| Drizzle | `.insert(t).select(qb)` style exists but is schema-object driven | `.except(qb)` in `sqlite-core` | raw `sql\`NOT EXISTS (...)\`` (no dedicated helper found) | raw fragment via `sql` in table config | `.onConflictDoNothing()` (same target-column shape as Kysely) | `.having()` | needs `alias()` per join member; not verified against comma-style FROM |
| Knex | `.insert(...).select(...)` chain | `.except()` (confirmed in `method-constants.js`) | `.whereNotExists(qb)` | none; needs `.raw()` | `.onConflict(cols).ignore()` → `on conflict (...) do nothing`, same target-column caveat as Kysely | `.having()` | `.select().from(t1).join(t2, ...)`, but generated-alias comma joins are not this builder's idiom — everything becomes explicit `.join()` |
| sql-template-tag | yes, it's just text | yes, just text | yes, just text | yes, just text | yes, just text | yes, just text | yes, just text — no structural help, no structural hindrance |
| @databases/sql | yes, just text + `sql.ident()` | yes | yes | yes | yes | yes | yes, same as above |
| squel | `.insert().fromQuery()` exists | not found in source scan | `.where('NOT EXISTS (?)', subquery)` string-composed | none | none native | `.having()` exists | string-first, same as squel's whole design |

## Per-candidate detail

### Kysely — first-class dynamic-schema escape hatch, but no current libsql dialect

`db.dynamic.ref(name)` and `db.dynamic.table(name)` exist precisely for "column/table not known
at compile time" (`src/dynamic/dynamic.ts`, verified from source). Both compile through the same
`IdentifierNode` path as static references, and `DefaultQueryCompiler.visitIdentifier` /
`sanitizeIdentifier` (`src/query-compiler/default-query-compiler.ts:492-505,1863-1878`) wraps
every identifier in `"..."` and **doubles an embedded quote** — dynamic refs are not a raw
string-splice, they go through the same escaping as static ones. The library's own doc comment
calls this out as a hazard anyway ("always validate the user input"), which is about defense in
depth, not about missing escaping.

Sketch against our shapes (rel names as strings, no compile-time `Database` type):

```ts
import { Kysely, sql } from "kysely";

// db: Kysely<any> — there is no static schema to type this against.
const { ref, table } = db.dynamic;

async function insertNewRows(headTable: string, headColumns: readonly string[], relOne: string, relTwo: string) {
  await db
    .insertInto(headTable as any)
    .columns(headColumns as any)
    .expression((expressionBuilder) =>
      expressionBuilder
        .selectFrom([table(relOne).as("b0"), table(relTwo).as("b1")])
        .select(headColumns.map((columnName) => ref(columnName)))
        .where(ref("b0.some_column"), "=", ref("b1.other_column")),
    )
    .onConflict((onConflictBuilder) => onConflictBuilder.columns(headColumns as any).doNothing())
    .execute();
}
```

Fit: every one of the 9 shapes is expressible (`.except()`, `eb.not(eb.exists(...))`,
`.modifyEnd(sql\`without rowid\`)`, `.having()`). The `as any` casts are not incidental — Kysely's
whole type-inference value proposition (the reason people pick it over hand-rolled SQL) requires
a `Database` interface mapping table names to column types; ours doesn't exist and can't, so every
call site loses that inference and falls back to the same runtime trust our current code already
has. Real cost found: the official `@libsql/kysely-libsql` dialect is stuck on
`@libsql/client@^0.8.0` and was last published 2024-07-30 (over two years stale against today);
the community fork `kysely-libsql` (unscoped, `ottomated/kysely-libsql`) is newer but its last
release was 2025-05-14 — also over a year stale, 2,192 weekly downloads. Either requires vetting
against our pinned `0.17.4` client, or writing a ~30-50 line custom `Dialect`/`DatabaseConnection`
adapter around `SqlRunner` ourselves.

### Drizzle ORM — heaviest, and its typed path needs column TYPES we don't carry

`sql` tag (`drizzle-orm/src/sql/sql.ts:478-551`, verified from source) ships `sql.identifier()`
(dialect-aware escaping), `sql.raw()`, `sql.join()` — a complete "smaller primitive" bundled
inside a much larger package. `sqliteTable(name, columns, extraConfig)` is a plain function
(`sqlite-core/src/table.ts:219-221`), so it CAN be called at runtime with a string name built
from our loop over `RelTables`. The blocker: `sqliteTable`'s second argument is a map of column
**builders** (`text()`, `integer()`, …) — Drizzle's model needs to know each column's SQL type.
Our `RelTable` (`lower/types.ts:9-12`) carries `columns: readonly string[]`, names only, no type.
Wiring this would mean threading `RelCol` (`engine/types.ts:368-371`, which spine.ts already has
at table-creation time) all the way into the lowering layer, a real, non-trivial input-shape
change that the string-only builders below don't need at all.

Fit for our 9 shapes: `.except()` exists in `sqlite-core`; `.onConflictDoNothing()` has the same
target-column shape as Kysely/Knex; `NOT EXISTS` and `WITHOUT ROWID` were not found as first-class
helpers in the scan and would drop to raw `sql` fragments regardless. Given the package is 10.4MB
unpacked across 2,666 files, and the typed path doesn't fit our input shape without new plumbing,
adopting Drizzle buys none of its headline benefit here.

### Knex — insists on owning the connection; that's the disqualifier, not maturity

Knex's core query builder is string-first (no compile-time schema at all, so the dynamic-schema
question is moot — it fits by default). Confirmed from source: `.except()`
(`lib/query/method-constants.js:69`), `.whereNotExists()` (standard Knex API), `.onConflict(cols)
.ignore()` compiling to `on conflict (...) do nothing` (`lib/dialects/sqlite3/query/sqlite-
querycompiler.js:96-98,177`, same target-column caveat as Kysely). `WITHOUT ROWID` has no schema-
builder modifier; would need `.raw()`.

The real issue is dependency shape: `knex` pulls `tarn` (its own connection-pool implementation),
`commander`, `interpret`, `rechoir`, `lodash` — 13 runtime deps (`registry.npmjs.org/knex/latest`
`dependencies`, verified) — because Knex wants to own a `client` (a dialect adapter over
`better-sqlite3`/`sqlite3`/`pg`/etc.) and its own pool, not just hand you a string. There is an
escape hatch: `queryBuilder.toSQL()` returns `{sql, bindings}` without executing, which we could
route through `SqlRunner` instead of Knex's own pool — but that means installing the whole
package (pool included) to use roughly the same 20% of it that sql-template-tag provides for
1/40th the weight. No libsql dialect for Knex was checked; unverified.

### sql-template-tag — the smaller primitive, values only

Read the whole source (`src/index.ts`, 21KB unpacked, zero deps). `Sql` class: `.sql` getter
emits positional `?` placeholders, `.values` is the parallel bound-args array — this is exactly
`@libsql/client`'s `{sql, args}` shape with **no formatter to write**. `raw(text)` is the
identifier/fragment escape hatch (splices `text` unescaped — the caller supplies already-safe
text, so it composes with a quoting helper rather than replacing one). `join()`/`bulk()` build
comma-separated fragments, useful for column lists and n-way FROM lists.

```ts
import sql, { raw, join } from "sql-template-tag";
import { escapeSQLiteIdentifier as quoteIdent } from "@databases/escape-identifier";

function insertOrIgnoreSelect(headTable: string, headColumns: readonly string[], fromParts: readonly string[], whereConditions: readonly ReturnType<typeof sql>[]) {
  const columnList = raw(headColumns.map(quoteIdent).join(", "));
  const fromList = raw(fromParts.join(", ")); // fromParts already carries "quoted_table alias"
  const whereClause = whereConditions.length > 0 ? sql`WHERE ${join(whereConditions, " AND ")}` : sql``;
  return sql`INSERT OR IGNORE INTO ${raw(quoteIdent(headTable))}(${columnList}) SELECT ${columnList} FROM ${fromList} ${whereClause}`;
}
```

No structural help with alias bookkeeping, join compilation, or the semi-naive delta/next
rewriting `RecursiveStratum` does — and no structural hindrance either. It is a one-for-one
replacement for `sqlLit` plus manual string concatenation, nothing more.

### @databases/sql + @databases/escape-identifier — the same primitive, one more layer

`@databases/sql`'s `SQLQuery` (`lib/web.js`, read in full) has `sql.ident(...names)` for
identifier fragments and automatic value parameterization through a `.format(formatter)` call
you supply — the SQLite-specific formatter (`escapeIdentifier`, `formatValue`) is not bundled in
this package; it lives in `@databases/sqlite` (last published 2022-10-29, pulls the `sqlite3`
native driver we don't use) or must be hand-written (~10 lines) against
`@databases/escape-identifier`'s standalone `escapeSQLiteIdentifier()` (`lib/index.js`, read in
full — validates ASCII, length ≤ 63, wraps in `"..."`, doubles embedded quotes). Net: two small
zero/near-zero-dep packages (65KB + 51KB) plus a formatter object we write ourselves, versus
sql-template-tag's zero-config `?`/`values` output. Functionally equivalent; sql-template-tag
needs less glue for this specific driver.

### Ruled out with a receipt

- **Slonik** (49.10.7, BSD-3-Clause, 146.7K/week): `@slonik/pg-driver` is a hard dependency
  (`registry.npmjs.org/slonik/latest` `dependencies`, verified) — Postgres-only, no SQLite path.
  Wrong dialect, not a maturity or API problem.
- **squel** (6.3.1, MIT, 18K/week): actually maintained — version history shows fresh 2026-06
  releases by the original author (`hiddentao/squel`, not archived, pushed 2026-06-17). But it
  carries an unpatched GitHub Security Advisory (`GHSA-4qhx-g9wp-g9m6`, "critical," `setFields`
  fails to escape a literal quote, "no fix at this time," recommends switching builders). Whether
  6.x actually fixes this is **unverified** — the advisory's tested range tops out at 5.13.0 and
  GitHub's record was not updated for later releases. Given the exact defect we're trying to fix
  is unescaped literal quoting, adopting a library with a documented history of exactly that bug,
  unconfirmed-fixed, is not a reasonable trade for the alternatives above.
- **TypeORM** (1.1.0, MIT, 5.0M/week): an ORM built around entity classes as the schema
  declaration; there is no dynamic-table code path that doesn't defeat the entity model, and it
  owns the connection. Wrong shape for a per-load runtime schema.
- **node-sql-parser** (5.4.0): parses SQL text into an AST ("simple node sql parser," registry
  description). It is the inverse tool — useful for validating/linting generated SQL, not for
  generating it. Not a query builder; excluded on category, not merit.

## The two defects

### 1. `sqlLit` inlines values as text (`lowerSql.ts:435-440`)

```ts
function sqlLit(value: string | number | boolean | null): string {
  if (value === null) return "NULL";
  if (typeof value === "number") return String(value);
  if (typeof value === "boolean") return value ? "1" : "0";
  return `'${value.replace(/'/g, "''")}'`;
}
```

Called from three sites: rule-body literal args (`lowerSql.ts:241`), comparison RHS
(`lowerSql.ts:256`), negated-body literal args (`lowerSql.ts:266`). The string branch does the
standard single-quote doubling, which is syntactically correct SQLite string-literal escaping —
it is not a naive/broken escape. The risk is architectural, not that the escaping is wrong today:
every literal from a `.dl` rule body or from `match`/`ast`/`sg`/`json`-extracted source text is
value-interpolated into the SQL text rather than bound as a parameter. `@libsql/client`'s
`InStatement` already supports `{sql, args}` (`engine/types.ts:41`); `SqlRunner.execute` passes
whatever `SqlStatement` it's given straight to `db.execute` (`sqlRunner.ts:16-22`) — there is
already a parameter-passing path, it's simply unused.

Given ingested source text is attacker-influenced in the general case (a `.dl` rule can match
strings out of scanned repositories), and quoting bugs are a well-known class (squel's own
history above is exactly this bug), the honest read is: today's single-quote doubling is likely
correct for SQLite's grammar, but "likely correct, unparameterized, hand-rolled" is a standing
risk each new literal-producing call site can silently reintroduce. A parameter-binding fix (swap
`sqlLit` for a placeholder + args-array convention) removes the whole class rather than keeping it
correct by inspection. This is what sql-template-tag/@databases/sql buy, at ~20-65KB, zero
material install cost, versus a bespoke fix that has to be re-reviewed by hand indefinitely.

### 2. Identifiers are interpolated raw — bounded by grammar, not by escaping

Traced: `RelTable.table`/`.columns` (`lower/types.ts:9-12`) come from `RelTables`, built from
`Program.rels` (`RelDecl`s) produced by `bridge()` (`dl/src/0_ast_bridge.ts:899`). Column names
originate at `processRelDecl` (`0_ast_bridge.ts:446-448`): `decl.columns.map((column) =>
column.name)`, where `decl.columns` are `ColumnDecl`s parsed by the grammar
(`dl/grammar/dl.langium:32`). Both rel names and column names lex through one terminal:

```
terminal ID: /[a-z_][a-z0-9_/]*/;
```

That character class admits lowercase letters, digits, `_`, and `/` — no quote, no semicolon, no
whitespace, no backslash. A `.dl`-declared rel or column name **cannot** carry a SQL metacharacter;
the grammar itself is the escaping. This is a real structural finding, not a hand-wave: the
injection surface the task asked about is closed at the parser boundary, provided no code
downstream concatenates anything else into a table/column name before it reaches `lowerSql.ts`
(no such concatenation was found; the production `RelTables`-construction glue itself was not
found in this tree — see "could not verify").

What the grammar does NOT close: `/` is legal in an `ID` token but is **not** a legal character in
an unquoted SQLite identifier. A rel name containing `/` would produce a syntax error the moment
it reaches `CREATE TABLE ${name} (...)` in `spine.ts`'s `create_rel_table`
(`engine/spine.ts:223-247`, itself string-interpolated, same pattern) or any `DELETE FROM
${table}` in `lowerSql.ts`. That is a correctness/reserved-word hazard exactly as the task
anticipated, not an injection path — the failure mode is "the program fails to load," not "an
attacker controls the query." SQL reserved words used as rel/column names (`select`, `order`,
`group`, …) are the same class of hazard and are equally unguarded today.

## Recommendation

Keep `DatalogEvaluator`/`RecursiveStratum` as the compiler. Adopt a small, purpose-built
templating primitive for the two defects; do not adopt a query-builder library as the join/rule
compiler.

Reasoning stated plainly: none of the candidates model "compile a stratified datalog rule into a
FROM/WHERE/alias/support-edge bundle, then re-run it as a semi-naive delta/next fixpoint" as a
unit — that's what `compileRuleJoin`, `RecursiveStratum.deriveStatements`, and `supportPlan` do,
and it is genuinely bespoke (this is the "one legitimately bespoke layer" the standing law already
carves out). Every builder surveyed gives you a fluent way to write ONE statement; ours already
writes correct statements, one rule at a time. Bolting Kysely's `db.dynamic` API onto today's
`compileRuleJoin` would replace `${alias}.${column}` string templates with `ref(...)` calls of
identical shape and add a stale-dialect dependency risk (`@libsql/kysely-libsql` two years stale)
for no join-compilation benefit, since aliasing/binding bookkeeping (`bound: Map<string,
string>`) stays exactly as hand-rolled as it is now — Kysely doesn't know what a datalog variable
is. Drizzle needs column-type input we don't have. Knex wants to own a connection pool we don't
want. squel carries an unresolved SQLi advisory in the exact defect class we're fixing.

The two defects are fixed correctly with `sql-template-tag` (parameter binding, ~21KB, 0 deps,
`.sql`/`.values` map straight onto `@libsql/client`'s `{sql, args}`) plus
`@databases/escape-identifier`'s `escapeSQLiteIdentifier()` (identifier quoting, ~51KB, 1 tiny
dep) for every table/column splice. This is "buy," not "write our own": both are small, focused,
already-audited libraries solving exactly the two narrow problems (parameterizing a value,
quoting an identifier) rather than us hand-maintaining `sqlLit` and an unguarded interpolation
site. It is not a query-builder purchase because there is no off-the-shelf shape for the thing
that actually needs building here.

Trade-off stated plainly: this fixes the two named defects and removes the "is this quoting
still correct" review burden, at the cost of ~72KB and two new small dependencies. It does not
reduce the line count of `lowerSql.ts` materially (the join/alias/fixpoint logic is unchanged) and
does not buy compile-time type safety (impossible here regardless of candidate, since the schema
is runtime-only). Anyone who later wants Kysely-style fluent syntax on top of this can layer it
in later without touching the fixpoint/support-edge logic, since that logic is orthogonal to how
individual statements get their text.

## Migration sketch

Scope: `lower/lowerSql.ts` (440 lines) and the return-type signatures in `lower/types.ts`
(`IDatalogEvaluator`, `IRecursiveStratum`). `engine/sqlRunner.ts` and `engine/types.ts` do not
change — `SqlStatement` already includes the `{sql, args}` shape.

| Method | Change | Rough size |
|---|---|---|
| `sqlLit` (435-440) | deleted; literal values become pushed into a parallel `args` array instead of returned as text | -6 lines |
| new `quoteIdent(name: string)` | thin wrapper over `escapeSQLiteIdentifier`, or import directly | +1-5 lines |
| `compileRuleJoin` (220-281) | every `${alias}.${column}` / `${sourceTable} ${alias}` splice routes through `quoteIdent`; `sqlLit(argument.value)` calls become `sql\`${argument.value}\``-style parameter pushes returned via the `CompiledJoin` shape (needs a new field: accumulated params) | ~20-30 line diff |
| `compileRuleSelect` (283-317) | select-list identifiers quoted; no literal values in this method today | ~5-10 line diff |
| `insertNewRowsStatement` (126-134), `mergeStatement` (139-143), `createLikeStatements` (145-151), `clearStatements` (79-83), `acyclicStatements` (85-91) | table/column names quoted; each now returns `{sql, args}` instead of `string` | ~15-20 line diff across all five |
| `supportPlan` (155-216) | table/alias/column identifiers quoted; no user-controlled literals here today, so no param changes | ~10 line diff |
| `IDatalogEvaluator`/`IRecursiveStratum` (`lower/types.ts`) | every `: string[]` / `: string` / `: string | null` return that becomes SQL text changes to the new statement shape (`SqlStatement[]` etc.) | ~15 line diff, type-only |
| `RecursiveStratum.*Statements` (`lowerSql.ts:357-397`) | same identifier-quoting treatment; no new literals introduced in this class | ~10 line diff |

Total estimate: roughly 100-130 changed lines across two files, plus two new `package.json`
dependencies. No change to `strata`, `stratify`, `scc`, the fixpoint `expand()`/`round()` control
flow, or `SqlRunner`.

## Could not verify

- Whether the production `RelTables` construction (bridging `bridge()`'s `Program.rels` into
  `{table, columns}` for real `.dl` programs, as opposed to `labs/stress.ts`'s synthetic harness)
  exists anywhere in this tree. Not found by search; may be unbuilt, may live on the Rust side.
  This matters because it is the actual point where a rel/column name would enter `lowerSql.ts` —
  confirm no extra string concatenation happens there before trusting the grammar-closes-injection
  argument above in production.
- Whether `sql-template-tag`'s `raw()` and `@databases/escape-identifier`'s
  `escapeSQLiteIdentifier()` behave correctly under libsql's specific SQL dialect quirks (both were
  verified against generic SQLite grammar rules, not executed against `@libsql/client` directly).
- Whether Knex has any actively maintained libsql dialect; not checked (ruled out for other
  reasons before reaching this question).
- Whether squel 6.x actually patches GHSA-4qhx-g9wp-g9m6; GitHub's advisory record still shows
  "no fix at this time" against `<=5.13.0` with no later-version confirmation either way.
- Drizzle's `NOT EXISTS`/`WITHOUT ROWID` support beyond raw `sql` fragments: only a source grep
  was done (`.except()`, `.onConflictDoNothing()` confirmed present in `sqlite-core`); a dedicated
  `notExists()` helper and any DDL table-option modifier were not found by grep, which is weaker
  evidence than the direct source reads done for Kysely/Knex/sql-template-tag/@databases/sql.
- Exact bundle/install weight in terms of *added* transitive node_modules bytes inside this
  specific project (numbers above are each package's own registry-reported `unpackedSize`, not a
  full dependency-tree disk measurement).
