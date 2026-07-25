# async -> rxjs: remaining targets

Counts from `grep -c` over `v6/**/src/*.ts` at b207c8ca. Total remaining surface: 606 hits
of `async ` / `await ` / `Promise<` across 17 files.

Landed already: `lower/lowerSql.ts` (the fixpoint is `expand()`), `dl/src/3_runtime.ts`
(both tick stages, all SQL helpers, and the sync control flow).

## The one legitimate async seam

`SqliteDb.execute()` from `@libsql/client` returns a `Promise`. That is the only real
asynchrony in the system. Every other `Promise` in the tree is contagion from it.

Rule: wrap it exactly once per package behind `defer(() => from(db.execute(...)))`.
Two such wrappers exist and are correct: `makeExec` (lowerSql.ts) and `execute$` (3_runtime.ts).
Nothing else may hold a `Promise`.

## Tier 1 — real control flow, convert to observables

| file | async | await | Promise | shape it wants |
|---|---:|---:|---:|---|
| `engine/engine.ts` | 44 | 99 | 41 | multi-round retract/assert/dred/scc bodies are `expand()`; `with_txn` is already reimplemented as `inTransaction` in 3_runtime.ts (duplicate, unify) |
| `engine/ingest.ts` | 8 | 23 | 8 | batch ingest pipeline, `from -> concatMap -> toArray` |
| `dl/src/6_http.ts` | 9 | 14 | 12 | `readBody` is `fromEvent(req,"data") -> reduce`; routing is a `merge` of route observables |
| `dl/src/1_hosts.ts` | 4 | 11 | 4 | 3 hand-rolled `new Promise` wrappers around `child_process.spawn` (`runShellLine`, `runSgProcess`, `runExtractProcess`) — textbook `Observable` + `fromEvent` |
| `dl/src/4_ingest.ts` | 2 | 4 | 2 | `extractFile` is an `async function*` yielding records. That is an Observable that was forced into a generator. |
| `engine/spine.ts` | 3 | 5 | 3 | DDL sequence, `inOrder` |
| `labs/stress.ts` | 8 | 16 | 8 | perf harness, low priority |

## Tier 2 — pass-through delegations, mechanical type flip only

These are the user's stated exemption ("would be a method call in any other lang"). Bodies
stay one-liners; only the return type flips `Promise<T>` -> `Observable<T>` when Tier 1 lands.

| file | Promise | note |
|---|---:|---|
| `engine/tasks.ts` | 42 | pure interface declarations (`Reach`, `Cascade`, `Reconcile`, `GraphStore`) |
| `engine/types.ts` | 66 | the contract header, all type-position |
| `engine/lib.ts` | 30 | `RelStore`/`Store` methods are `async x(a) { return engine.x(this.db, this.ns, a); }` |
| `engine/algo.ts` | 4 | 4 `async` with zero `await` — async for no reason at all |

## Tier 3 — the public contract

`dl/src/0_types.ts` `IDlRuntime`: `commit(): Promise<TickReport>`, `rows(): Promise<Row[]>`,
`dispose(): Promise<void>`. Flipping these flips every caller including the tests. Do it last,
in one commit, or the conformance test cannot be trusted as the arbiter.

`SqliteDb.execute` (0_types.ts:201) STAYS a `Promise`. It is the driver boundary.

## Traps

- `await someObservable` returns the observable without subscribing. TypeScript accepts it with
  no complaint. This shipped once already (`await insertRows(...)` in `boot`, literal seeds
  silently never inserted, caught only by the conformance test). Grep for `await` on any name
  ending `$` or any known observable-returning function before declaring a file done.
- Interning must be flushed before `BEGIN`. Moving an encode step inside the transaction leaves
  new string ids unflushed and every derived row decodes to `null`.

## Duplication ledger — delete these

Verified by reading both bodies, not by name match.

| what | sites | bodies | fix |
|---|---|---|---|
| `inOrder` | `3_runtime.ts:285`, `lowerSql.ts:19` | identical | one copy in the store's rx module |
| `execBatch` | `engine.ts:187`, `engine.ts:923` | identical | hoist to file scope |
| `key_of` / `hex_of` | `lib.ts:542`, `ingest.ts:149` | identical, renamed | export from `lib.ts` |
| `with_txn` / `inTransaction` | `engine.ts:162`, `3_runtime.ts:290` | same bracket, Promise vs Observable | keep the observable one |
| `normalizeValue` | `3_runtime.ts:131`, `1_hosts.ts:59` | **divergent** | see below |
| `exec` | `engine.ts:142` (cascade), `engine.ts:868` (reconcile) | **divergent** | see below |
| `scalar` / `query_ids` / `query_bigints` | `engine.ts:179,873,880` | three shapes of "first column" | one reader, typed by caller |

Two of these are bugs, not style:

- `1_hosts.ts:59 normalizeValue` has no `bigint` branch, so a bigint falls through to
  `String(raw)` and lands in a `Value` as text. `3_runtime.ts:131` handles it. libsql runs
  with `intMode:"bigint"`, so every integer column arrives as a bigint. Unify on the runtime
  version.
- `engine.ts:868 exec` (reconcile) calls `db.executeMultiple` with no tracing and no statement
  split. `engine.ts:142 exec` (cascade) splits and honours `DL_CASCADE_TRACE`. Every reconcile
  statement is therefore invisible to the trace. Unify on the cascade version.

NOT duplication, leaving alone: `create_schema` appears three times (`engine.ts:198`, `:886`,
`:1083`) but each is a different namespace's DDL. Same name, different schema, correct.

## Where the shared code goes

Interface-bound, not bare exports, so the compiler actually checks the signatures.

The six helpers are two unrelated jobs, so they are two names, not one vague one.

**Running SQL.** `engine/types.ts` gains:

```ts
export interface ISqlRunner {
  execute(db: SqliteDb, statement: string | { sql: string; args: unknown[] }): Observable<ResultSet>;
  run(db: SqliteDb, statement: string): Observable<void>;
  scalar(db: SqliteDb, statement: string): Observable<number>;
  inTransaction<Value>(db: SqliteDb, body: () => Observable<Value>): Observable<Value>;
}
```

`engine/sqlRunner.ts` is `export const SqlRunner: ISqlRunner = { ... }`. `execute` is the single
`defer(() => from(db.execute(...)))` seam; `inTransaction` replaces `with_txn`.

**Sequencing observables.** `inOrder` and `forEachInOrder` are leaf helpers, the rx spelling of a
`for` loop. No interface, they fall under the leaf exemption. One shared copy in
`engine/sequence.ts`, imported by `lowerSql.ts` and `3_runtime.ts` in place of their local ones.

**Row decoding.** `IRowCodec` in `dl/src/0_types.ts`, `export const RowCodec: IRowCodec` in a new
`dl/src/0_row.ts`, carrying `normalizeValue` and `rowFromRaw`.

Interfaces keep the `I` prefix, which is what lets each interface and its object share a root
word. `lower/types.ts` is the inconsistent one: `RelTable`, `Graph`, `Stratum`, `SupportEdges`,
`EvalProgram` all want `I`, and `IDatalog` needs a name that says what it does.

## Adjacent, not yet ruled on

- 27 `private` modifiers across `dl/src` and `engine/` (user law: no `private`).
- `engine/lib.ts` uses `_db`/`_ns` prefixes and `by_id`; the store package is snake_case
  throughout (Rust port artifact) while `dl` is camelCase.
- Interface naming is split: `IGraphNs`/`IRelStore`/`IStore`/`IDlRuntime` carry `I`,
  `lower/types.ts` uses plain words (`RelTable`, `Graph`, `Stratum`).
- No header interface for `tasks.ts` `Namespaced`/`Independent`/`Evidence` or
  `engine.ts` `AscendingIdQueue`.
