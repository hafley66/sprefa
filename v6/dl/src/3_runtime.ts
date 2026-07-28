/** DlRuntime: one sqlite connection, one tick loop, one delta stream.
 *
 *    commits$ -> concatMap(applyEdbTxn) -> concatMap(applyDerivedTxn)
 *             -> tap(clearScratchRels) -> mergeMap(events) -> share()   = deltas$
 *
 *  concatMap is the lock. share() is the only fan-out. The fixpoint (evalProgramSql) is
 *  awaited inside applyDerivedTxn, so a settled answer is the only thing anyone observes.
 *
 *  THE TICK LOOP TURNS ONLY WHILE SOMETHING SUBSCRIBES `deltas$`. This class holds no
 *  subscription of its own (one-subscribe law): the app graph in 6_http.ts hands
 *  `deltas$` to main.ts's single terminal subscription for the life of the program, and
 *  a test fixture does the same for the life of the test. `share()` multicasts that one
 *  active source to every later reader (SSE, HostRunner), so no reader re-runs a tick.
 *  `commit()` refuses loudly rather than hanging when nothing is subscribed.
 *
 *  Surface plane is text (rel_* decode views); storage plane is interned integers
 *  (relbase_*). internProgramForStorage crosses it for a PROGRAM,
 *  encodeSurfaceRowByColumns for a ROW. */

import {
  EMPTY,
  Subject,
  catchError,
  concat,
  concatMap,
  defer,
  filter,
  firstValueFrom,
  forkJoin,
  from,
  map,
  mergeMap,
  of,
  reduce,
  share,
  tap,
  throwError,
  toArray,
  type Observable,
} from "rxjs";
import { differenceWith, isEqual } from "lodash-es";

import { cascade, type Db } from "sprefa-store-engine/src/engine/engine.ts";
const { KEY_STRIDE } = cascade;
import { rels as rel_tables } from "sprefa-store-engine/src/engine/spine.ts";
import { RelStore, Store, open_db } from "sprefa-store-engine/src/engine/lib.ts";
import { evalProgramSql } from "sprefa-store-engine/src/lower/lowerSql.ts";
import { RowCodec } from "./0_row.ts";
import type { RelTable, RelTables, SupportEdges } from "sprefa-store-engine/src/lower/types.ts";
import type { Arg, BodyPred, LitValue, Program, RelDecl, Rule } from "sprefa-store-engine/src/lower/ast.ts";

import { ROW_SURROGATE, ddl, relBaseColumns, rowDigest } from "./2_schema.ts";
import type {
  BridgeOk,
  ColumnType,
  DeltaEvent,
  DeltaRow,
  IDlRuntime,
  IDlRuntimeStatics,
  AssertTrue,
  EdbBatch,
  Retention,
  Row,
  TickReport,
  Value,
} from "./0_types.ts";


export interface RuntimeState {
  readonly db: Db;
  readonly store: Store;
  /** Rel name -> its DENSE TAG (0..n-1, declaration order), the tag half of
   *  `cascade.key(tag, row_id)`. Never a string-dictionary id: those are sparse. */
  readonly relTags: ReadonlyMap<string, number>;
  readonly columnTypes: ReadonlyMap<string, readonly ColumnType[]>;
  readonly relDecls: ReadonlyMap<string, RelDecl>;
  readonly retention: ReadonlyMap<string, Retention>;
  readonly derivedRelNames: readonly string[];
  /** The program as the STORAGE plane sees it: every body literal rewritten to its
   *  interned id (`internProgramForStorage`). This is what evalProgramSql compiles. */
  readonly storageProgram: Program;
  /** rel name -> the physical `relbase_*` table + its columns, evalProgramSql's map. */
  readonly relTables: RelTables;
  /** The store's Z-set fact plane: `cx_row(key, weight)` + `cx_dep(parent_key, child_key)`
   *  under the default `cx_`/`rx_` namespace, and the cycle-safe retraction algorithms over
   *  them (`retract_dred`, `retract_scc`, `assert`). Every address is
   *  `cascade.key(rel_tag, row_id)`: two dense integers packed into one i64. */
  readonly relStore: RelStore;
  /** How evalProgramSql should emit the support graph into `relStore`'s dep table. */
  readonly supportEdges: SupportEdges;
  /** Rules the support pass cannot cover soundly (negated body / aggregate head), so a
   *  reader knows which heads must not be retracted through the graph. */
  readonly rulesWithoutSupport: readonly { readonly head: string; readonly reason: string }[];
  /** Mirrors the current content of every derived (IDB) rel as of the end of the last
   *  tick, in the TEXT surface (read back through the `rel_*` decode views). The diff
   *  the tick publishes is taken against this, which is what makes a derived row's
   *  disappearance a weight -1 delta rather than a silent overwrite. */
  readonly derivedTableMirror: Map<string, Row[]>;
  readonly reportsSubject: Subject<{ readonly id: number; readonly report: TickReport }>;
  /** Rows the previous tick physically deleted for retention-0 rels. Non-zero means the
   *  EDB moved since the last fixpoint even though THIS tick's batch may be empty, so
   *  the recompute-skip below must not fire. */
  scratchClearedRows: number;
}

interface CommitRequest {
  readonly id: number;
  readonly batch: EdbBatch;
}

interface EdbTickOutcome {
  readonly id: number;
  readonly tick: number;
  readonly changedPairs: readonly [rel: string, delta: number][];
  readonly events: readonly DeltaEvent[];
  readonly changedRelNames: readonly string[];
}

interface PerRelDiff {
  readonly insert: readonly Row[];
  readonly retract: readonly Row[];
  readonly newRows: readonly Row[];
}

interface SettledOutcome {
  readonly id: number;
  readonly report: TickReport;
  readonly events: readonly DeltaEvent[];
  /** Rels physically cleared this tick (retention 0); clearScratchRels resets their
   *  JS-side mirror/source after the DELETE (already awaited inside applyDerivedTxn). */
  readonly scratchRelNames: readonly string[];
}




type StoredRow = readonly number[];

function sqlTuple(row: StoredRow): string {
  return `(${row.join(",")})`;
}

function surfaceRowKey(row: Row, columns: readonly string[]): string {
  return JSON.stringify(columns.map((column) => row[column] ?? null));
}

function storedRowKey(row: StoredRow): string {
  return JSON.stringify(row);
}

function encodeSurfaceRowByColumns(
  store: Store,
  columnTypes: ReadonlyMap<string, readonly ColumnType[]>,
  relName: string,
  columns: readonly string[],
  row: Row,
): StoredRow {
  const types = columnTypes.get(relName);
  if (!types || types.length !== columns.length) throw new Error(`commit: invalid column types for rel '${relName}'`);
  return columns.map((column, index) => {
    const value = row[column] ?? null;
    if (types[index] === "int") {
      if (value === null) {
        // Numeric NULLs are outside this storage slice; the base table is NOT NULL.
        throw new Error(`commit: numeric NULL in rel '${relName}' column '${column}'`);
      }
      if (typeof value === "boolean") return value ? 1 : 0;
      if (typeof value !== "number") throw new Error(`commit: non-numeric value in rel '${relName}' column '${column}'`);
      return value;
    }
    return value === null ? -1 : store.intern(String(value));
  });
}

/** Rewrites every body literal to its interned id, once at boot, so lowerSql compares
 *  integers against integer columns. Refuses two shapes because interning destroys what
 *  they need: ordering comparisons and max/min/sum on a TEXT column (id order is intern
 *  order). Equality and count stay. */

function encodeLiteral(store: Store, type: ColumnType, value: LitValue, where: string): number {
  if (type === "int") {
    if (value === null) throw new Error(`${where}: numeric NULL literal (the stored column is NOT NULL)`);
    if (typeof value === "boolean") return value ? 1 : 0;
    if (typeof value !== "number") throw new Error(`${where}: non-numeric literal '${String(value)}' against an int column`);
    return value;
  }
  // -1 is the stored NULL sentinel for a text column (2_schema.ts's rel_* decode view).
  return value === null ? -1 : store.intern(String(value));
}

const ORDERING_OPS: ReadonlySet<string> = new Set(["lt", "le", "gt", "ge"]);

export function internProgramForStorage(
  program: Program,
  columnTypes: ReadonlyMap<string, readonly ColumnType[]>,
  store: Store,
): Program {
  const typesOf = (relName: string): readonly ColumnType[] => {
    const types = columnTypes.get(relName);
    if (!types) throw new Error(`internProgramForStorage: no column types for rel '${relName}'`);
    return types;
  };

  const rules: Rule[] = program.rules.map((rule) => {
    // Which column type binds each variable. First positive binding wins, matching
    // lowerSql's compileRuleSelect binding order exactly (same left-to-right sweep).
    const varType = new Map<string, ColumnType>();
    for (const pred of rule.body) {
      if (pred.kind !== "rel") continue;
      const types = typesOf(pred.rel);
      pred.args.forEach((arg, columnIndex) => {
        if (arg.kind !== "var" || varType.has(arg.name)) return;
        varType.set(arg.name, types[columnIndex] ?? "text");
      });
    }

    const body: BodyPred[] = rule.body.map((pred) => {
      if (pred.kind === "rel" || pred.kind === "notrel") {
        const types = typesOf(pred.rel);
        const args: Arg[] = pred.args.map((arg, columnIndex) =>
          arg.kind === "lit"
            ? {
                kind: "lit",
                value: encodeLiteral(store, types[columnIndex] ?? "text", arg.value, `rel '${pred.rel}' arg ${columnIndex}`),
              }
            : arg,
        );
        return { ...pred, args };
      }
      const type = varType.get(pred.lhs.name) ?? "text";
      if (type === "text" && ORDERING_OPS.has(pred.op)) {
        throw new Error(
          `rule '${rule.head}': ordering comparison '${pred.op}' on text variable '${pred.lhs.name}' is not supported ` +
            "by the interned storage plane (dictionary id order is intern order, not string order)",
        );
      }
      return {
        ...pred,
        rhs: { kind: "lit", value: encodeLiteral(store, type, pred.rhs.value, `rule '${rule.head}' comparison`) },
      };
    });

    for (const term of rule.headTerms) {
      if (term.kind !== "hagg" || term.fn === "count") continue;
      if ((varType.get(term.arg.name) ?? "text") === "text") {
        throw new Error(
          `rule '${rule.head}': aggregate '${term.fn}' over text variable '${term.arg.name}' is not supported by the ` +
            "interned storage plane (it would fold dictionary ids, not values)",
        );
      }
    }

    return { ...rule, body };
  });

  return { rels: program.rels, rules };
}


const ROW_MATCH_TEMP_TABLE = "_row_match_candidates";
/** SQLite's compiled-in compound-select/VALUES-row cap (default 500): the ceiling on
 *  how many `(...)` tuples one multi-row INSERT ... VALUES statement may hold. */
const ROW_MATCH_INSERT_CHUNK = 500;

function relMaxColumnWidth(relDecls: ReadonlyMap<string, RelDecl>): number {
  let max = 1;
  for (const decl of relDecls.values()) max = Math.max(max, decl.columns.length);
  return max;
}

type QueryResult = Awaited<ReturnType<Db["execute"]>>;

const execute$ = (db: Db, sql: string): Observable<QueryResult> => defer(() => from(db.execute(sql)));

/** Run each statement in order; one emission with every result once the last finishes
 *  (`toArray` emits `[]` even for an empty list, so callers can always sequence on it). */
const executeAll$ = (db: Db, statements: readonly string[]): Observable<QueryResult[]> =>
  concat(...statements.map((sql) => execute$(db, sql))).pipe(toArray());

/** BEGIN IMMEDIATE / COMMIT / ROLLBACK as an observable bracket. Single statements on the
 *  one pinned connection: `executeMultiple` would trip the adapter's rollback guard. */
function inTransaction<Value>(db: Db, body: () => Observable<Value>): Observable<Value> {
  return execute$(db, "BEGIN IMMEDIATE").pipe(
    concatMap(body),
    concatMap((value) => execute$(db, "COMMIT").pipe(map(() => value))),
    catchError((failure: unknown) =>
      execute$(db, "ROLLBACK").pipe(
        catchError(() => of(undefined)),
        concatMap(() => throwError(() => failure)),
      ),
    ),
  );
}

/** Clears the shared temp table (creating it on first use, sized to the widest rel this
 *  runtime declares) and loads `rows` into its generic c0..c<n-1> columns, chunked so no
 *  single INSERT's VALUES list trips the compound-select cap. */
function loadRowMatchCandidates(
  db: Db,
  relDecls: ReadonlyMap<string, RelDecl>,
  rows: readonly StoredRow[],
  rowWidth: number,
): Observable<QueryResult[]> {
  return defer(() => {
    const allColumns = Array.from({ length: relMaxColumnWidth(relDecls) }, (_, index) => `c${index}`).join(", ");
    const usedColumns = Array.from({ length: rowWidth }, (_, index) => `c${index}`).join(", ");
    const chunks: StoredRow[][] = [];
    for (let chunkStart = 0; chunkStart < rows.length; chunkStart += ROW_MATCH_INSERT_CHUNK) {
      chunks.push(rows.slice(chunkStart, chunkStart + ROW_MATCH_INSERT_CHUNK));
    }
    return executeAll$(db, [
      `CREATE TEMP TABLE IF NOT EXISTS ${ROW_MATCH_TEMP_TABLE} (${allColumns})`,
      `DELETE FROM ${ROW_MATCH_TEMP_TABLE}`,
      ...chunks.map(
        (chunk) => `INSERT INTO ${ROW_MATCH_TEMP_TABLE}(${usedColumns}) VALUES ${chunk.map(sqlTuple).join(",")}`,
      ),
    ]);
  });
}

/** NULL-safe AND-join between `relAlias`'s named columns and the temp table's positional
 *  c0..c<n-1> columns: O(columns) predicate depth, independent of row count. */
function rowMatchJoinCondition(columns: readonly string[], relAlias: string, tempAlias: string): string {
  return columns.map((column, index) => `${relAlias}.${column} IS ${tempAlias}.c${index}`).join(" AND ");
}

function selectAll(db: Db, relName: string, columns: readonly string[]): Observable<Row[]> {
  return execute$(db, `SELECT ${columns.join(",")} FROM rel_${relName}`).pipe(
    map((result) => result.rows.map((rawRow) => RowCodec.rowFromRaw(rawRow, columns))),
  );
}

/** One SELECT over the candidate tuples, never a per-row existence check. */
function preCheckExistingKeys(
  db: Db,
  relDecls: ReadonlyMap<string, RelDecl>,
  relName: string,
  columns: readonly string[],
  candidates: readonly StoredRow[],
): Observable<ReadonlySet<string>> {
  if (candidates.length === 0) return of(new Set<string>());
  const selectColumns = columns.map((column) => `t.${column}`).join(",");
  return loadRowMatchCandidates(db, relDecls, candidates, columns.length).pipe(
    concatMap(() =>
      execute$(
        db,
        `SELECT ${selectColumns} FROM relbase_${relName} t JOIN ${ROW_MATCH_TEMP_TABLE} c ON ${rowMatchJoinCondition(columns, "t", "c")}`,
      ),
    ),
    map(
      (result) =>
        new Set(
          result.rows.map((rawRow) =>
            storedRowKey(columns.map((column) => Number((rawRow as Record<string, unknown>)[column]))),
          ),
        ),
    ),
  );
}

function insertRows(db: Db, relName: string, columns: readonly string[], rows: readonly StoredRow[]): Observable<QueryResult[]> {
  const statements =
    rows.length === 0
      ? []
      : [
          `INSERT INTO relbase_${relName}(${columns.join(",")}) VALUES ${rows.map(sqlTuple).join(",")} ON CONFLICT DO NOTHING`,
        ];
  return executeAll$(db, statements);
}

function deleteRows(
  db: Db,
  relDecls: ReadonlyMap<string, RelDecl>,
  relName: string,
  columns: readonly string[],
  rows: readonly StoredRow[],
): Observable<QueryResult[]> {
  if (rows.length === 0) return of([]);
  const tableRef = `relbase_${relName}`;
  return loadRowMatchCandidates(db, relDecls, rows, columns.length).pipe(
    concatMap(() =>
      executeAll$(db, [
        `DELETE FROM ${tableRef} WHERE EXISTS (SELECT 1 FROM ${ROW_MATCH_TEMP_TABLE} c WHERE ${rowMatchJoinCondition(columns, tableRef, "c")})`,
      ]),
    ),
  );
}

function insertDeltaRows(db: Db, rows: readonly DeltaRow[]): Observable<QueryResult[]> {
  const values = rows.map((row) => `(${row.rel_tag},${row.row_digest},${row.tick},${row.weight})`).join(",");
  const statements = rows.length === 0 ? [] : [`INSERT INTO delta(rel_tag,row_digest,tick,weight) VALUES ${values}`];
  return executeAll$(db, statements);
}

/** What one rel's EDB write resolved to, before any of it is applied. */
interface RelWriteOutcome {
  readonly relName: string;
  readonly columns: readonly string[];
  readonly inserted: readonly Row[];
  readonly retracted: readonly Row[];
}

/** Encode a batch side to stored rows, per rel. Throws on an unknown rel. */
function encodeBatchSide(
  state: RuntimeState,
  side: ReadonlyMap<string, readonly Row[]>,
): ReadonlyMap<string, readonly StoredRow[]> {
  return new Map(
    [...side].map(([relName, rows]) => {
      const decl = state.relDecls.get(relName);
      if (!decl) throw new Error(`commit: unknown rel '${relName}'`);
      return [
        relName,
        rows.map((row) => encodeSurfaceRowByColumns(state.store, state.columnTypes, relName, decl.columns, row)),
      ] as const;
    }),
  );
}

/** Resolve and apply one rel's writes: pre-check both sides, add the retention-1 sweep,
 *  then insert and delete. Emits what actually moved. */
function applyRelWrite(
  state: RuntimeState,
  request: CommitRequest,
  encodedInsert: ReadonlyMap<string, readonly StoredRow[]>,
  encodedRetract: ReadonlyMap<string, readonly StoredRow[]>,
  relName: string,
): Observable<RelWriteOutcome> {
  return defer(() => {
    const { db } = state;
    const decl = state.relDecls.get(relName);
    if (!decl) throw new Error(`commit: unknown rel '${relName}'`);
    const columns = decl.columns;
    const insertCandidates = request.batch.insert.get(relName) ?? [];
    const retractCandidates = request.batch.retract.get(relName) ?? [];
    const insertIds = encodedInsert.get(relName) ?? [];
    const retractIds = encodedRetract.get(relName) ?? [];
    const keepNewestOnly = (state.retention.get(relName) ?? "all") === 1;

    const pickAbsent = (ids: readonly StoredRow[], existing: ReadonlySet<string>): number[] =>
      ids.flatMap((row, index) => (existing.has(storedRowKey(row)) ? [] : [index]));
    const pickPresent = (ids: readonly StoredRow[], existing: ReadonlySet<string>): number[] =>
      ids.flatMap((row, index) => (existing.has(storedRowKey(row)) ? [index] : []));

    return forkJoin({
      existingForInsert: preCheckExistingKeys(db, state.relDecls, relName, columns, insertIds),
      existingForRetract: preCheckExistingKeys(db, state.relDecls, relName, columns, retractIds),
    }).pipe(
      concatMap(({ existingForInsert, existingForRetract }) => {
        const newIndexes = pickAbsent(insertIds, existingForInsert);
        const retractedIndexes = pickPresent(retractIds, existingForRetract);
        const inserted = newIndexes.map((index) => insertCandidates[index]!);
        const insertedIds = newIndexes.map((index) => insertIds[index]!);
        const retracted = retractedIndexes.map((index) => retractCandidates[index]!);
        const retractedIds = retractedIndexes.map((index) => retractIds[index]!);

        const supersededByRetention =
          keepNewestOnly && insertCandidates.length > 0
            ? selectAll(db, relName, columns).pipe(
                map((fullBefore) => {
                  const installed = new Set(insertCandidates.map((row) => surfaceRowKey(row, columns)));
                  const alreadyGoing = new Set(retracted.map((row) => surfaceRowKey(row, columns)));
                  return fullBefore.filter((row) => {
                    const key = surfaceRowKey(row, columns);
                    return !installed.has(key) && !alreadyGoing.has(key);
                  });
                }),
              )
            : of([] as Row[]);

        return supersededByRetention.pipe(
          concatMap((superseded) => {
            const allRetracted = [...retracted, ...superseded];
            const allRetractedIds = [
              ...retractedIds,
              ...superseded.map((row) => encodeSurfaceRowByColumns(state.store, state.columnTypes, relName, columns, row)),
            ];
            return concat(
              insertRows(db, relName, columns, insertedIds),
              deleteRows(db, state.relDecls, relName, columns, allRetractedIds),
            ).pipe(
              toArray(),
              map(() => ({ relName, columns, inserted, retracted: allRetracted })),
            );
          }),
        );
      }),
    );
  });
}

/** tick++, then every rel's write, then the delta log. One transaction. */
export function applyEdbTxn(state: RuntimeState, request: CommitRequest): Observable<EdbTickOutcome> {
  const { db } = state;
  const relNames = [...new Set([...request.batch.insert.keys(), ...request.batch.retract.keys()])];
  // Encode BEFORE the flush and before the transaction. Encoding interns new strings, and
  // flush_strings uses executeMultiple, which would roll back an open BEGIN. Interning
  // inside the transaction leaves the new ids unflushed, so the rel_* decode views read
  // them back as NULL.
  const encodedInsert = encodeBatchSide(state, request.batch.insert);
  const encodedRetract = encodeBatchSide(state, request.batch.retract);
  return state.store.flush_strings().pipe(
    concatMap(() =>
      inTransaction(db, () =>
        execute$(db, "UPDATE store_meta SET value = value + 1 WHERE key='tick' RETURNING value").pipe(
          map((result) => Number(result.rows[0]?.[0] ?? 0)),
          concatMap((tick) =>
            from(relNames).pipe(
              concatMap((relName) => applyRelWrite(state, request, encodedInsert, encodedRetract, relName)),
              toArray(),
              concatMap((written) => {
                const moved = written.filter((write) => write.inserted.length > 0 || write.retracted.length > 0);
                const deltaRows = moved.flatMap((write) => {
                  const relTag = state.relTags.get(write.relName)!;
                  return [
                    ...write.inserted.map((row) => deltaRowOf(relTag, row, write.columns, tick, 1)),
                    ...write.retracted.map((row) => deltaRowOf(relTag, row, write.columns, tick, -1)),
                  ];
                });
                const changedPairs = written
                  .map((write) => [write.relName, write.inserted.length - write.retracted.length] as [string, number])
                  .filter(([, net]) => net !== 0);
                return insertDeltaRows(db, deltaRows).pipe(
                  map(() => ({
                    id: request.id,
                    tick,
                    changedPairs,
                    events: moved.map((write) => ({
                      tick,
                      rel: write.relName,
                      inserts: write.inserted,
                      retracts: write.retracted,
                    })),
                    changedRelNames: moved.map((write) => write.relName),
                  })),
                );
              }),
            ),
          ),
        ),
      ),
    ),
  );
}

/** Synchronous tap. The retention-0 DELETE already ran inside applyDerivedTxn; this only
 *  resets the mirror for the rels it emptied, then unblocks commit()'s promise. */
export function clearScratchRels(state: RuntimeState, outcome: SettledOutcome): void {
  for (const relName of outcome.scratchRelNames) {
    const decl = state.relDecls.get(relName);
    if (!decl || decl.origin === "EDB") continue;
    state.derivedTableMirror.set(relName, []);
  }
  state.reportsSubject.next({ id: outcome.id, report: outcome.report });
}

const deltaRowOf = (relTag: number, row: Row, columns: readonly string[], tick: number, weight: 1 | -1): DeltaRow => ({
  rel_tag: relTag,
  row_digest: rowDigest(row, columns),
  tick,
  weight,
});

export function diffDerivedRel(
  oldRows: readonly Row[],
  newRows: readonly Row[],
): { readonly insert: readonly Row[]; readonly retract: readonly Row[] } {
  return {
    insert: differenceWith(newRows as Row[], oldRows as Row[], isEqual),
    retract: differenceWith(oldRows as Row[], newRows as Row[], isEqual),
  };
}

export function diffAgainstTables(state: RuntimeState): Observable<ReadonlyMap<string, PerRelDiff>> {
  return from(state.derivedRelNames).pipe(
    concatMap((relName) => {
      const decl = state.relDecls.get(relName);
      if (!decl) throw new Error(`diffAgainstTables: unknown derived rel '${relName}'`);
      return selectAll(state.db, relName, decl.columns).pipe(
        map((newRows) => {
          const diff = diffDerivedRel(state.derivedTableMirror.get(relName) ?? [], newRows);
          return [relName, { insert: diff.insert, retract: diff.retract, newRows }] as const;
        }),
      );
    }),
    filter(([, diff]) => diff.insert.length > 0 || diff.retract.length > 0),
    reduce((perRel: Map<string, PerRelDiff>, [relName, diff]) => perRel.set(relName, diff), new Map<string, PerRelDiff>()),
  );
}

/** One fact that died, resolved from its packed key back to a named rel and a surface row. */
export interface DeadFact {
  readonly rel: string;
  readonly row: Row;
}

/** The dense surrogates of `rows` in `relName`'s table, one SELECT for the whole set. */
function selectSurrogates(
  db: Db,
  relDecls: ReadonlyMap<string, RelDecl>,
  relName: string,
  columns: readonly string[],
  rows: readonly StoredRow[],
): Observable<number[]> {
  if (rows.length === 0) return of([]);
  return loadRowMatchCandidates(db, relDecls, rows, columns.length).pipe(
    concatMap(() =>
      execute$(
        db,
        `SELECT t.${ROW_SURROGATE} FROM relbase_${relName} t JOIN ${ROW_MATCH_TEMP_TABLE} c ON ${rowMatchJoinCondition(columns, "t", "c")}`,
      ),
    ),
    map((result) => result.rows.map((rawRow) => Number((rawRow as Record<string, unknown>)[ROW_SURROGATE]))),
  );
}

/** Unpack `key = rel_tag * KEY_STRIDE + row_id`, group by rel (plain Map — the keys are
 *  already in memory), one SELECT per rel, run in order. */
function resolveFactKeys(state: RuntimeState, keys: readonly number[]): Observable<DeadFact[]> {
  const nameOfTag = new Map([...state.relTags].map(([relName, tag]) => [tag, relName] as const));
  const rowIdsByTag = new Map<number, number[]>();
  for (const key of keys) {
    const tag = Math.trunc(key / KEY_STRIDE);
    const rowIds = rowIdsByTag.get(tag);
    if (rowIds) rowIds.push(key % KEY_STRIDE);
    else rowIdsByTag.set(tag, [key % KEY_STRIDE]);
  }
  const selects = [...rowIdsByTag].flatMap(([tag, rowIds]) => {
    const relName = nameOfTag.get(tag);
    const decl = relName === undefined ? undefined : state.relDecls.get(relName);
    if (relName === undefined || !decl) return [];
    return [
      execute$(
        state.db,
        `SELECT ${decl.columns.join(",")} FROM rel_${relName} WHERE ${ROW_SURROGATE} IN (${rowIds.join(",")})`,
      ).pipe(
        map((result) =>
          result.rows.map((rawRow) => ({ rel: relName, row: RowCodec.rowFromRaw(rawRow, decl.columns) })),
        ),
      ),
    ];
  });
  return concat(...selects).pipe(
    toArray(),
    map((factsPerRel) => factsPerRel.flat()),
  );
}

/** Mirror the settled model into the store's Z-set fact plane, SQL-to-SQL, one statement
 *  per rel. `key = rel_tag * KEY_STRIDE + row_id`, computed from two dense integer
 *  columns, so no row ever enters the JS heap here. */
function refreshFactPlane(state: RuntimeState): Observable<QueryResult[]> {
  const rowTable = state.relStore.ns().row;
  const stride = state.supportEdges.stride;
  const statements = [...state.relTables].flatMap(([relName, table]) => {
    const tag = state.relTags.get(relName);
    if (tag === undefined) return [];
    return [
      `INSERT INTO ${rowTable}(key, weight) SELECT ${tag} * ${stride} + ${ROW_SURROGATE}, 1 FROM ${table.table} ` +
        `WHERE true ON CONFLICT(key) DO UPDATE SET weight = 1`,
    ];
  });
  return executeAll$(state.db, statements);
}

/** The fixpoint stage. One transaction: evalProgramSql, mirror into cx_row, diff the
 *  settled tables against derivedTableMirror, write delta rows, then the retention-0
 *  DELETE (here rather than in the later tap, which cannot await before commit()
 *  resolves). Skipped whole when no EDB row moved and no scratch row was cleared. */
export function applyDerivedTxn(state: RuntimeState, outcome: EdbTickOutcome): Observable<SettledOutcome> {
  const { db, relDecls } = state;
  const edbMoved = outcome.changedRelNames.length > 0 || state.scratchClearedRows > 0;
  state.scratchClearedRows = 0;

  const scratchRelNames = [...state.retention]
    .filter(([relName, retention]) => retention === 0 && relDecls.has(relName))
    .map(([relName]) => relName);

  return inTransaction(db, () => {
    const fixpoint: Observable<QueryResult[]> =
      edbMoved && state.derivedRelNames.length > 0
        ? evalProgramSql(db, state.storageProgram, state.relTables, state.supportEdges).pipe(
            concatMap(() => refreshFactPlane(state)),
          )
        : of([]);

    return fixpoint.pipe(
      concatMap(() => (edbMoved ? diffAgainstTables(state) : of(new Map<string, PerRelDiff>()))),
      concatMap((perRel) => {
        const entries = [...perRel];
        const deltaRows = entries.flatMap(([relName, diff]) => {
          const columns = relDecls.get(relName)!.columns;
          const relTag = state.relTags.get(relName)!;
          return [
            ...diff.insert.map((row) => deltaRowOf(relTag, row, columns, outcome.tick, 1)),
            ...diff.retract.map((row) => deltaRowOf(relTag, row, columns, outcome.tick, -1)),
          ];
        });
        const derivedPairs = entries
          .map(([relName, diff]) => [relName, diff.insert.length - diff.retract.length] as [string, number])
          .filter(([, net]) => net !== 0);
        const derivedEvents = entries.map(([relName, diff]) => ({
          tick: outcome.tick,
          rel: relName,
          inserts: diff.insert,
          retracts: diff.retract,
        }));
        for (const [relName, diff] of entries) state.derivedTableMirror.set(relName, diff.newRows.slice());

        return insertDeltaRows(db, deltaRows).pipe(
          concatMap(() =>
            from(scratchRelNames).pipe(
              concatMap((relName) => execute$(db, `DELETE FROM relbase_${relName}`)),
              reduce((cleared: number, result) => cleared + Number(result.rowsAffected ?? 0), 0),
            ),
          ),
          map((clearedRows) => {
            state.scratchClearedRows = clearedRows;
            const byRelName = (left: string, right: string): number => (left < right ? -1 : left > right ? 1 : 0);
            return {
              id: outcome.id,
              report: {
                tick: outcome.tick,
                changed: [...outcome.changedPairs, ...derivedPairs].sort((a, b) => byRelName(a[0], b[0])),
              },
              events: [...outcome.events, ...derivedEvents].sort((a, b) => byRelName(a.rel, b.rel)),
              scratchRelNames,
            };
          }),
        );
      }),
    );
  });
}



function literalSeedRows(
  store: Store,
  columnTypes: ReadonlyMap<string, readonly ColumnType[]>,
  literalSeeds: ReadonlyMap<string, Value>,
  relDecls: ReadonlyMap<string, RelDecl>,
): ReadonlyMap<string, readonly StoredRow[]> {
  const rows = new Map<string, StoredRow[]>();
  for (const [relName, value] of literalSeeds) {
    const decl = relDecls.get(relName);
    if (!decl) throw new Error(`DlRuntime.boot: literal seed rel '${relName}' has no decl`);
    const column = decl.columns[0];
    if (column === undefined) throw new Error(`DlRuntime.boot: literal seed rel '${relName}' has no columns`);
    const row: Row = { [column]: value };
    const encoded = encodeSurfaceRowByColumns(store, columnTypes, relName, decl.columns, row);
    const current = rows.get(relName) ?? [];
    current.push(encoded);
    rows.set(relName, current);
  }
  return rows;
}


export class DlRuntime implements IDlRuntime {
  private readonly commits$: Subject<CommitRequest>;
  private nextCommitId: number;
  readonly deltas$: Observable<DeltaEvent>;

  private constructor(private readonly state: RuntimeState) {
    this.commits$ = new Subject<CommitRequest>();
    this.nextCommitId = 1;

    const tick$ = this.commits$.pipe(concatMap((request) => applyEdbTxn(state, request)));

    const derived$ = tick$.pipe(concatMap((outcome) => applyDerivedTxn(state, outcome)));

    const settled$ = derived$.pipe(tap((outcome) => clearScratchRels(state, outcome)));

    // Cold until someone subscribes; share() then multicasts that one active source to
    // every later reader instead of re-running applyEdbTxn/applyDerivedTxn per reader.
    this.deltas$ = settled$.pipe(
      mergeMap((outcome) => from(outcome.events)),
      share(),
    );
  }

  static async boot(config: { dbPath: string; bridge: BridgeOk; extraDdl?: readonly string[] }): Promise<DlRuntime> {
    const db = open_db(`file:${config.dbPath}`);
    const store = await firstValueFrom(Store.open(db));

    const relDecls = new Map<string, RelDecl>();
    for (const decl of config.bridge.program.rels) relDecls.set(decl.name, decl);

    // Rel identity is a DENSE TAG, 0..n-1 in declaration order, not the rel name's
    // string-dictionary id. The dictionary id is sparse (it shares one id space with
    // every interned string in the database) and cannot be packed: `cascade.key(tag, id)`
    // is `tag * KEY_STRIDE + id` with KEY_STRIDE = 1e9, so a tag has to be small and
    // dense for the i64 to hold anything. `rel_tag` persists tag -> name_id so a reader
    // can still resolve a tag back to a name without the tag itself carrying text.
    const relTags = new Map<string, number>();
    config.bridge.program.rels.forEach((decl, tag) => relTags.set(decl.name, tag));
    const tagRows = config.bridge.program.rels.map((decl, tag) => `(${tag},${store.intern(decl.name)})`);
    await firstValueFrom(store.flush_strings());

    for (const decl of config.bridge.program.rels) {
      await firstValueFrom(
        rel_tables.create_rel_table(
          db,
          `relbase_${decl.name}`,
          relBaseColumns(decl, config.bridge.columnTypes),
          decl.columns,
          ROW_SURROGATE,
        ),
      );
    }

    const ddlStatements = [
      ...ddl(config.bridge.program.rels, config.bridge.retention, config.bridge.columnTypes),
      ...(config.extraDdl ?? []),
    ];
    await db.batch(ddlStatements, "write");

    // Persist the dense tag -> name_id mapping, one batched insert (never per-rel).
    if (tagRows.length > 0) {
      await db.execute(`INSERT INTO rel_tag(tag,name_id) VALUES ${tagRows.join(",")} ON CONFLICT DO NOTHING`);
    }

    const seedRows = literalSeedRows(store, config.bridge.columnTypes, config.bridge.literalSeeds, relDecls);
    await firstValueFrom(store.flush_strings());
    for (const [relName, rows] of seedRows) {
      const decl = relDecls.get(relName)!;
      await firstValueFrom(insertRows(db, relName, decl.columns, rows));
    }

    // The program the SQL fixpoint compiles: same rules, every body literal rewritten
    // to its interned id. Interning here may mint new dictionary strings, so flush
    // before the first evaluation reads them back.
    const storageProgram = internProgramForStorage(config.bridge.program, config.bridge.columnTypes, store);
    await firstValueFrom(store.flush_strings());

    const relTables = new Map<string, RelTable>();
    for (const decl of config.bridge.program.rels) {
      relTables.set(decl.name, { table: `relbase_${decl.name}`, columns: decl.columns });
    }

    // The store's Z-set fact plane. `attach` stamps cascade's cx_* and reconcile's rx_*
    // schema under the default namespace onto the same connection dl already owns.
    const relStore = await firstValueFrom(RelStore.attach(db));
    const supportEdges: SupportEdges = {
      table: relStore.ns().dep,
      tagOf: relTags,
      stride: KEY_STRIDE,
      surrogate: ROW_SURROGATE,
    };
    // Report the coverage limit once, at boot, from the real rule set rather than from a
    // guess: a negated body or an aggregate head has non-monotone support, so those heads
    // are not retractable through the graph.
    const { rulesWithoutSupport } = await firstValueFrom(evalProgramSql(db, storageProgram, relTables, supportEdges));

    const derivedRelNames = config.bridge.program.rels.filter((decl) => decl.origin === "IDB").map((decl) => decl.name);
    const derivedTableMirror = new Map<string, Row[]>();
    for (const relName of derivedRelNames) {
      const decl = relDecls.get(relName)!;
      derivedTableMirror.set(relName, await firstValueFrom(selectAll(db, relName, decl.columns)));
    }

    const state: RuntimeState = {
      db,
      store,
      relTags,
      columnTypes: config.bridge.columnTypes,
      relDecls,
      retention: config.bridge.retention,
      derivedRelNames,
      storageProgram,
      relTables,
      relStore,
      supportEdges,
      rulesWithoutSupport,
      derivedTableMirror,
      reportsSubject: new Subject(),
      scratchClearedRows: 0,
    };

    return new DlRuntime(state);
  }

  async commit(batch: EdbBatch): Promise<TickReport> {
    // Without a live subscriber the request would sit in commits$ forever and this
    // promise would never settle. Say so instead of hanging.
    if (!this.commits$.observed) {
      throw new Error("DlRuntime.commit: nothing is subscribed to deltas$, so the tick loop is not running");
    }
    const id = this.nextCommitId++;
    const pending = firstValueFrom(
      this.state.reportsSubject.pipe(
        filter((entry) => entry.id === id),
        map((entry) => entry.report),
      ),
    );
    this.commits$.next({ id, batch });
    return pending;
  }

  /**
   * Retract `rows` from EDB rel `rel` THROUGH THE SUPPORT GRAPH, with no recompute:
   * resolve each row to its dense surrogate, pack `cascade.key(rel_tag, row_id)`, seed
   * `cascade.retract_dred`, and report every fact that died.
   *
   * DRed (Delete-and-Rederive) is the cycle-safe variant, and that choice is not
   * incidental. Row-level support in a least fixpoint over a CYCLIC EDB is itself cyclic
   * (`ancestor` over a graph with a cycle supports itself round-trip), and counting
   * retraction cannot tell a live cycle from a dead one: DRed over-deletes the forward
   * cone, then rederives anything still reachable from a surviving row, so a dead cycle
   * correctly stays dead because it has no surviving anchor.
   *
   * The dead set is measured, not inferred: `alive_keys()` before and after, differenced.
   * That reads the store's own answer rather than reconstructing what it should have done.
   *
   * COVERAGE: heads listed in `rulesWithoutSupport` have non-monotone support (negated
   * body predicate or aggregate head) and carry no edges, so they will not appear in the
   * dead set even when they should. `supportCoverageGaps()` reports them. This is why the
   * tick still recomputes; the graph path is proven against it before replacing it.
   */
  async retractThroughSupport(rel: string, rows: readonly Row[]): Promise<{ rounds: number; dead: readonly DeadFact[] }> {
    const state = this.state;
    const decl = state.relDecls.get(rel);
    if (!decl) throw new Error(`retractThroughSupport: unknown rel '${rel}'`);
    const tag = state.relTags.get(rel);
    if (tag === undefined) throw new Error(`retractThroughSupport: no dense tag for rel '${rel}'`);

    const encoded = rows.map((row) =>
      encodeSurfaceRowByColumns(state.store, state.columnTypes, rel, decl.columns, row),
    );
    await firstValueFrom(state.store.flush_strings());
    const rowIds = await firstValueFrom(selectSurrogates(state.db, state.relDecls, rel, decl.columns, encoded));
    if (rowIds.length === 0) return { rounds: 0, dead: [] };

    const before = new Set(await firstValueFrom(state.relStore.alive_keys()));
    const rounds = await firstValueFrom(
      state.relStore.retract_dred(rowIds.map((rowId) => [tag, rowId] as const)),
    );
    const after = new Set(await firstValueFrom(state.relStore.alive_keys()));

    const deadKeys = [...before].filter((key) => !after.has(key));
    return { rounds, dead: await firstValueFrom(resolveFactKeys(state, deadKeys)) };
  }

  /** Rules whose heads carry no support edges, so they are not retractable through the
   *  graph. Empty means the whole program is covered. */
  supportCoverageGaps(): readonly { readonly head: string; readonly reason: string }[] {
    return this.state.rulesWithoutSupport;
  }

  async rows(rel: string): Promise<Row[]> {
    return firstValueFrom(this.rows$(rel));
  }

  /** The observable read. `rows()` is the promise adapter IDlRuntime's contract declares. */
  rows$(rel: string): Observable<Row[]> {
    const decl = this.state.relDecls.get(rel);
    if (!decl) throw new Error(`DlRuntime.rows: unknown rel '${rel}'`);
    return selectAll(this.state.db, rel, decl.columns);
  }

  /** Completing commits$ ends the tick chain, which completes deltas$, which ends the
   *  app graph's (or a test fixture's) subscription on its own — nothing to unsubscribe
   *  here. */
  async dispose(): Promise<void> {
    this.commits$.complete();
    this.state.reportsSubject.complete();
    this.state.db.close();
  }
}

// ---- static-side proof (src/0_types.ts) --------------------------------------
// `implements` above covers the instance side; this covers `boot`.
export type DlRuntimeStaticsHold = AssertTrue<typeof DlRuntime extends IDlRuntimeStatics ? true : false>;
