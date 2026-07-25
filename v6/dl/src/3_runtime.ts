/** DlRuntime: one sqlite connection, one tick loop, one delta stream.
 *
 *    commits$ -> concatMap(applyEdbTxn) -> concatMap(applyDerivedTxn)
 *             -> tap(clearScratchRels) -> mergeMap(events) -> share()   = deltas$
 *
 *  concatMap is the lock. share() is the only fan-out. The fixpoint (evalProgramSql) is
 *  awaited inside applyDerivedTxn, so a settled answer is the only thing anyone observes.
 *
 *  Surface plane is text (rel_* decode views); storage plane is interned integers
 *  (relbase_*). internProgramForStorage crosses it for a PROGRAM,
 *  encodeSurfaceRowByColumns for a ROW. */

import {
  Subject,
  Subscription,
  concatMap,
  filter,
  firstValueFrom,
  from,
  map,
  mergeMap,
  share,
  tap,
  type Observable,
} from "rxjs";
import { differenceWith, isEqual } from "lodash-es";

import { cascade, type Db } from "sprefa-store-engine/src/engine/engine.ts";
const { with_txn, KEY_STRIDE } = cascade;
import { rels as rel_tables } from "sprefa-store-engine/src/engine/spine.ts";
import { RelStore, Store, open_db } from "sprefa-store-engine/src/engine/lib.ts";
import { evalProgramSql } from "sprefa-store-engine/src/lower/lowerSql.ts";
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


function normalizeValue(raw: unknown): Value {
  if (raw === undefined || raw === null) return null;
  if (typeof raw === "bigint") return Number(raw);
  if (typeof raw === "number" || typeof raw === "string" || typeof raw === "boolean") return raw;
  return String(raw);
}

function rowFromRaw(rawRow: unknown, columns: readonly string[]): Row {
  const raw = rawRow as Record<string, unknown>;
  const row: Record<string, Value> = {};
  for (const column of columns) row[column] = normalizeValue(raw[column]);
  return row as Row;
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
      pred.args.forEach((arg, col) => {
        if (arg.kind !== "var" || varType.has(arg.name)) return;
        varType.set(arg.name, types[col] ?? "text");
      });
    }

    const body: BodyPred[] = rule.body.map((pred) => {
      if (pred.kind === "rel" || pred.kind === "notrel") {
        const types = typesOf(pred.rel);
        const args: Arg[] = pred.args.map((arg, col) =>
          arg.kind === "lit"
            ? {
                kind: "lit",
                value: encodeLiteral(store, types[col] ?? "text", arg.value, `rel '${pred.rel}' arg ${col}`),
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

/** Clears the shared temp table (creating it on first use, sized to the widest rel
 *  this runtime declares) and loads `rows` into its generic c0..c<n-1> columns,
 *  chunked so no single INSERT's VALUES list trips the compound-select cap. A no-op
 *  (leaves the temp table empty) when `rows` is empty. */
async function loadRowMatchCandidates(
  db: Db,
  relDecls: ReadonlyMap<string, RelDecl>,
  rows: readonly StoredRow[],
  rowWidth: number,
): Promise<void> {
  const width = relMaxColumnWidth(relDecls);
  const allColumns = Array.from({ length: width }, (_, i) => `c${i}`).join(", ");
  await db.execute(`CREATE TEMP TABLE IF NOT EXISTS ${ROW_MATCH_TEMP_TABLE} (${allColumns})`);
  await db.execute(`DELETE FROM ${ROW_MATCH_TEMP_TABLE}`);
  if (rows.length === 0) return;
  const usedColumns = Array.from({ length: rowWidth }, (_, i) => `c${i}`).join(", ");
  for (let start = 0; start < rows.length; start += ROW_MATCH_INSERT_CHUNK) {
    const chunk = rows.slice(start, start + ROW_MATCH_INSERT_CHUNK);
    const values = chunk.map((row) => sqlTuple(row)).join(",");
    await db.execute(`INSERT INTO ${ROW_MATCH_TEMP_TABLE}(${usedColumns}) VALUES ${values}`);
  }
}

/** NULL-safe AND-join condition between `relAlias`'s named columns and the temp
 *  table's positional c0..c<n-1> columns, referenced through `tempAlias` (same `IS`
 *  semantics the old OR-of-row-predicates form used, just an O(columns) join
 *  predicate instead of an O(rows) OR chain). */
function rowMatchJoinCondition(columns: readonly string[], relAlias: string, tempAlias: string): string {
  return columns.map((column, i) => `${relAlias}.${column} IS ${tempAlias}.c${i}`).join(" AND ");
}

async function selectAll(db: Db, relName: string, columns: readonly string[]): Promise<Row[]> {
  const res = await db.execute(`SELECT ${columns.join(",")} FROM rel_${relName}`);
  return res.rows.map((rawRow: unknown) => rowFromRaw(rawRow, columns));
}

/** The batched pre-check SELECT law (ingest.ts note 3): one SELECT over the candidate
 *  tuples, never a per-row existence check. Matches via the shared temp-table JOIN
 *  (see the row-set-matching block above), not an OR-of-row-predicates blob. */
async function preCheckExistingKeys(
  db: Db,
  relDecls: ReadonlyMap<string, RelDecl>,
  relName: string,
  columns: readonly string[],
  candidates: readonly StoredRow[],
): Promise<Set<string>> {
  if (candidates.length === 0) return new Set();
  await loadRowMatchCandidates(db, relDecls, candidates, columns.length);
  const selectColumns = columns.map((column) => `t.${column}`).join(",");
  const res = await db.execute(
    `SELECT ${selectColumns} FROM relbase_${relName} t JOIN ${ROW_MATCH_TEMP_TABLE} c ON ${rowMatchJoinCondition(columns, "t", "c")}`,
  );
  const keys = new Set<string>();
  for (const rawRow of res.rows) {
    const raw = rawRow as Record<string, unknown>;
    keys.add(storedRowKey(columns.map((column) => Number(raw[column]))));
  }
  return keys;
}

async function insertRows(db: Db, relName: string, columns: readonly string[], rows: readonly StoredRow[]): Promise<void> {
  if (rows.length === 0) return;
  const values = rows.map((row) => sqlTuple(row)).join(",");
  await db.execute(`INSERT INTO relbase_${relName}(${columns.join(",")}) VALUES ${values} ON CONFLICT DO NOTHING`);
}

async function deleteRows(
  db: Db,
  relDecls: ReadonlyMap<string, RelDecl>,
  relName: string,
  columns: readonly string[],
  rows: readonly StoredRow[],
): Promise<void> {
  if (rows.length === 0) return;
  await loadRowMatchCandidates(db, relDecls, rows, columns.length);
  const tableRef = `relbase_${relName}`;
  await db.execute(
    `DELETE FROM ${tableRef} WHERE EXISTS (SELECT 1 FROM ${ROW_MATCH_TEMP_TABLE} c WHERE ${rowMatchJoinCondition(columns, tableRef, "c")})`,
  );
}

async function insertDeltaRows(db: Db, rows: readonly DeltaRow[]): Promise<void> {
  if (rows.length === 0) return;
  const values = rows
    .map((row) => `(${row.rel_tag},${row.row_digest},${row.tick},${row.weight})`)
    .join(",");
  await db.execute(`INSERT INTO delta(rel_tag,row_digest,tick,weight) VALUES ${values}`);
}


export async function applyEdbTxn(state: RuntimeState, request: CommitRequest): Promise<EdbTickOutcome> {
  const { db } = state;
  const encodedInsert = new Map<string, readonly StoredRow[]>();
  const encodedRetract = new Map<string, readonly StoredRow[]>();
  for (const [relName, rows] of request.batch.insert) {
    const decl = state.relDecls.get(relName);
    if (!decl) throw new Error(`commit: unknown rel '${relName}'`);
    encodedInsert.set(
      relName,
      rows.map((row) => encodeSurfaceRowByColumns(state.store, state.columnTypes, relName, decl.columns, row)),
    );
  }
  for (const [relName, rows] of request.batch.retract) {
    const decl = state.relDecls.get(relName);
    if (!decl) throw new Error(`commit: unknown rel '${relName}'`);
    encodedRetract.set(
      relName,
      rows.map((row) => encodeSurfaceRowByColumns(state.store, state.columnTypes, relName, decl.columns, row)),
    );
  }
  // flush_strings uses executeMultiple, which rolls back an open BEGIN in the store
  // adapter. Its monotonic dictionary makes this pre-transaction flush safe.
  await state.store.flush_strings();

  return with_txn(db, async () => {
    const tickRes = await db.execute("UPDATE store_meta SET value = value + 1 WHERE key='tick' RETURNING value");
    const tick = Number(tickRes.rows[0]?.[0] ?? 0);

    const relNames = new Set<string>([...request.batch.insert.keys(), ...request.batch.retract.keys()]);
    const changedPairs: [string, number][] = [];
    const events: DeltaEvent[] = [];
    const changedRelNames: string[] = [];
    const deltaRows: DeltaRow[] = [];

    for (const relName of relNames) {
      const decl = state.relDecls.get(relName);
      if (!decl) throw new Error(`commit: unknown rel '${relName}'`);
      const columns = decl.columns;
      const insertCandidates = request.batch.insert.get(relName) ?? [];
      const retractCandidates = request.batch.retract.get(relName) ?? [];
      const insertIds = encodedInsert.get(relName) ?? [];
      const retractIds = encodedRetract.get(relName) ?? [];

      const existingForInsert = await preCheckExistingKeys(db, state.relDecls, relName, columns, insertIds);
      const genuinelyNewIndexes = insertIds.flatMap((row, index) =>
        existingForInsert.has(storedRowKey(row)) ? [] : [index],
      );
      const genuinelyNew = genuinelyNewIndexes.map((index) => insertCandidates[index]!);
      const genuinelyNewIds = genuinelyNewIndexes.map((index) => insertIds[index]!);

      const existingForRetract = await preCheckExistingKeys(db, state.relDecls, relName, columns, retractIds);
      const genuinelyRetractedIndexes = retractIds.flatMap((row, index) =>
        existingForRetract.has(storedRowKey(row)) ? [index] : [],
      );
      const genuinelyRetracted = genuinelyRetractedIndexes.map((index) => retractCandidates[index]!);
      const genuinelyRetractedIds = genuinelyRetractedIndexes.map((index) => retractIds[index]!);

      let additionalRetracts: Row[] = [];
      let additionalRetractIds: StoredRow[] = [];
      const isLatestOnly = (state.retention.get(relName) ?? "all") === 1;
      if (isLatestOnly && insertCandidates.length > 0) {
        const fullBefore = await selectAll(db, relName, columns);
        const installedKeys = new Set(insertCandidates.map((row) => surfaceRowKey(row, columns)));
        const alreadyRetractedKeys = new Set(genuinelyRetracted.map((row) => surfaceRowKey(row, columns)));
        additionalRetracts = fullBefore.filter(
          (row) => !installedKeys.has(surfaceRowKey(row, columns)) && !alreadyRetractedKeys.has(surfaceRowKey(row, columns)),
        );
        additionalRetractIds = additionalRetracts.map((row) =>
          encodeSurfaceRowByColumns(state.store, state.columnTypes, relName, columns, row),
        );
      }

      if (genuinelyNewIds.length > 0) await insertRows(db, relName, columns, genuinelyNewIds);
      const allRetracted = [...genuinelyRetracted, ...additionalRetracts];
      const allRetractedIds = [...genuinelyRetractedIds, ...additionalRetractIds];
      if (allRetractedIds.length > 0) await deleteRows(db, state.relDecls, relName, columns, allRetractedIds);

      const relTag = state.relTags.get(relName)!;
      for (const row of genuinelyNew) {
        deltaRows.push({ rel_tag: relTag, row_digest: rowDigest(row, columns), tick, weight: 1 });
      }
      for (const row of allRetracted) {
        deltaRows.push({ rel_tag: relTag, row_digest: rowDigest(row, columns), tick, weight: -1 });
      }

      const net = genuinelyNew.length - allRetracted.length;
      if (net !== 0) changedPairs.push([relName, net]);

      if (genuinelyNew.length > 0 || allRetracted.length > 0) {
        changedRelNames.push(relName);
        events.push({ tick, rel: relName, inserts: genuinelyNew, retracts: allRetracted });
      }
    }

    await insertDeltaRows(db, deltaRows);

    return { id: request.id, tick, changedPairs, events, changedRelNames };
  });
}


export function diffDerivedRel(
  oldRows: readonly Row[],
  newRows: readonly Row[],
): { readonly insert: readonly Row[]; readonly retract: readonly Row[] } {
  return {
    insert: differenceWith(newRows as Row[], oldRows as Row[], isEqual),
    retract: differenceWith(oldRows as Row[], newRows as Row[], isEqual),
  };
}


export async function diffAgainstTables(state: RuntimeState): Promise<ReadonlyMap<string, PerRelDiff>> {
  const perRel = new Map<string, PerRelDiff>();
  for (const relName of state.derivedRelNames) {
    const decl = state.relDecls.get(relName);
    if (!decl) throw new Error(`diffAgainstTables: unknown derived rel '${relName}'`);
    const newRows = await selectAll(state.db, relName, decl.columns);
    const oldRows = state.derivedTableMirror.get(relName) ?? [];
    const diff = diffDerivedRel(oldRows, newRows);
    if (diff.insert.length > 0 || diff.retract.length > 0) {
      perRel.set(relName, { insert: diff.insert, retract: diff.retract, newRows });
    }
  }
  return perRel;
}


/** One fact that died, resolved from its packed key back to a named rel and a surface row. */
export interface DeadFact {
  readonly rel: string;
  readonly row: Row;
}

/** The dense surrogates of `rows` in `relName`'s table, via the shared temp-table join
 *  (one SELECT for the whole candidate set, never a per-row lookup). A row that is not
 *  present yields nothing. */
async function selectSurrogates(
  db: Db,
  relDecls: ReadonlyMap<string, RelDecl>,
  relName: string,
  columns: readonly string[],
  rows: readonly StoredRow[],
): Promise<number[]> {
  if (rows.length === 0) return [];
  await loadRowMatchCandidates(db, relDecls, rows, columns.length);
  const tableRef = `relbase_${relName}`;
  const res = await db.execute(
    `SELECT t.${ROW_SURROGATE} FROM ${tableRef} t JOIN ${ROW_MATCH_TEMP_TABLE} c ON ${rowMatchJoinCondition(columns, "t", "c")}`,
  );
  return res.rows.map((raw) => Number((raw as Record<string, unknown>)[ROW_SURROGATE]));
}

/** Unpack `key = rel_tag * KEY_STRIDE + row_id` and read each row back through its
 *  `rel_*` decode view, grouped so there is one SELECT per rel rather than per key. */
async function resolveFactKeys(state: RuntimeState, keys: readonly number[]): Promise<DeadFact[]> {
  const byTag = new Map<number, number[]>();
  for (const key of keys) {
    const tag = Math.trunc(key / KEY_STRIDE);
    const rowId = key % KEY_STRIDE;
    const bucket = byTag.get(tag);
    if (bucket) bucket.push(rowId);
    else byTag.set(tag, [rowId]);
  }
  const nameOfTag = new Map<number, string>();
  for (const [relName, tag] of state.relTags) nameOfTag.set(tag, relName);

  const facts: DeadFact[] = [];
  for (const [tag, rowIds] of byTag) {
    const relName = nameOfTag.get(tag);
    if (relName === undefined) continue;
    const decl = state.relDecls.get(relName);
    if (!decl) continue;
    const res = await state.db.execute(
      `SELECT ${decl.columns.join(",")} FROM rel_${relName} WHERE ${ROW_SURROGATE} IN (${rowIds.join(",")})`,
    );
    for (const raw of res.rows) facts.push({ rel: relName, row: rowFromRaw(raw, decl.columns) });
  }
  return facts;
}


async function refreshFactPlane(state: RuntimeState): Promise<void> {
  const { db, relStore } = state;
  const ns = relStore.ns();
  const stride = state.supportEdges.stride;
  for (const [relName, table] of state.relTables) {
    const tag = state.relTags.get(relName);
    if (tag === undefined) continue;
    await db.execute(
      `INSERT INTO ${ns.row}(key, weight) SELECT ${tag} * ${stride} + ${ROW_SURROGATE}, 1 FROM ${table.table} ` +
        `WHERE true ON CONFLICT(key) DO UPDATE SET weight = 1`,
    );
  }
}

/** The fixpoint stage. One transaction: evalProgramSql, mirror into cx_row, diff the
 *  settled tables against derivedTableMirror, write delta rows, then the retention-0
 *  DELETE (awaited here because a tap cannot await before commit() resolves).
 *  Skipped whole when no EDB row moved and no scratch row was cleared. */

export async function applyDerivedTxn(state: RuntimeState, outcome: EdbTickOutcome): Promise<SettledOutcome> {
  const { db, relDecls } = state;
  const edbMoved = outcome.changedRelNames.length > 0 || state.scratchClearedRows > 0;
  state.scratchClearedRows = 0;

  return with_txn(db, async () => {
    if (edbMoved && state.derivedRelNames.length > 0) {
      await evalProgramSql(db, state.storageProgram, state.relTables, state.supportEdges);
      await refreshFactPlane(state);
    }
    const perRel = edbMoved ? await diffAgainstTables(state) : new Map<string, PerRelDiff>();

    const derivedEvents: DeltaEvent[] = [];
    const derivedPairs: [string, number][] = [];
    const deltaRows: DeltaRow[] = [];

    for (const [relName, { insert, retract, newRows }] of perRel) {
      const columns = relDecls.get(relName)!.columns;
      const relTag = state.relTags.get(relName)!;
      for (const row of insert) {
        deltaRows.push({ rel_tag: relTag, row_digest: rowDigest(row, columns), tick: outcome.tick, weight: 1 });
      }
      for (const row of retract) {
        deltaRows.push({ rel_tag: relTag, row_digest: rowDigest(row, columns), tick: outcome.tick, weight: -1 });
      }
      const net = insert.length - retract.length;
      if (net !== 0) derivedPairs.push([relName, net]);
      derivedEvents.push({ tick: outcome.tick, rel: relName, inserts: insert, retracts: retract });
      state.derivedTableMirror.set(relName, newRows.slice());
    }

    if (deltaRows.length > 0) await insertDeltaRows(db, deltaRows);

    const scratchRelNames: string[] = [];
    for (const [relName, retention] of state.retention) {
      if (retention === 0 && relDecls.has(relName)) scratchRelNames.push(relName);
    }
    let clearedRows = 0;
    for (const relName of scratchRelNames) {
      const res = await db.execute(`DELETE FROM relbase_${relName}`);
      clearedRows += Number(res.rowsAffected ?? 0);
    }
    state.scratchClearedRows = clearedRows;

    const changed = [...outcome.changedPairs, ...derivedPairs].sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0));
    const events = [...outcome.events, ...derivedEvents].sort((a, b) => (a.rel < b.rel ? -1 : a.rel > b.rel ? 1 : 0));

    return { id: outcome.id, report: { tick: outcome.tick, changed }, events, scratchRelNames };
  });
}


export function clearScratchRels(state: RuntimeState, outcome: SettledOutcome): void {
  for (const relName of outcome.scratchRelNames) {
    const decl = state.relDecls.get(relName);
    if (!decl || decl.origin === "EDB") continue;
    // A derived rel(0) rel's table was just emptied; the mirror must agree, or the next
    // tick's diff would see the recomputed rows as unchanged and publish no delta.
    state.derivedTableMirror.set(relName, []);
  }
  state.reportsSubject.next({ id: outcome.id, report: outcome.report });
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
  private readonly keepAlive: Subscription;
  private nextCommitId: number;
  readonly deltas$: Observable<DeltaEvent>;

  private constructor(private readonly state: RuntimeState) {
    this.commits$ = new Subject<CommitRequest>();
    this.nextCommitId = 1;

    const tick$ = this.commits$.pipe(concatMap((request) => applyEdbTxn(state, request)));

    const derived$ = tick$.pipe(concatMap((outcome) => applyDerivedTxn(state, outcome)));

    const settled$ = derived$.pipe(tap((outcome) => clearScratchRels(state, outcome)));

    this.deltas$ = settled$.pipe(
      mergeMap((outcome) => from(outcome.events)),
      share(),
    );

    // The one permanent subscription: keeps commits$ -> ... -> deltas$ hot for the life
    // of the runtime, regardless of how many external readers (SSE/LSP/HostRunner)
    // subscribe to deltas$ afterward. Without this, every external subscriber would
    // re-run applyEdbTxn/applyDerivedTxn itself (a double-write bug) since share()
    // only multicasts an ALREADY-active source.
    this.keepAlive = this.deltas$.subscribe({
      error: (err: unknown) => {
        throw err;
      },
    });
  }

  static async boot(cfg: { dbPath: string; bridge: BridgeOk; extraDdl?: readonly string[] }): Promise<DlRuntime> {
    const db = open_db(`file:${cfg.dbPath}`);
    const store = await Store.open(db);

    const relDecls = new Map<string, RelDecl>();
    for (const decl of cfg.bridge.program.rels) relDecls.set(decl.name, decl);

    // Rel identity is a DENSE TAG, 0..n-1 in declaration order, not the rel name's
    // string-dictionary id. The dictionary id is sparse (it shares one id space with
    // every interned string in the database) and cannot be packed: `cascade.key(tag, id)`
    // is `tag * KEY_STRIDE + id` with KEY_STRIDE = 1e9, so a tag has to be small and
    // dense for the i64 to hold anything. `rel_tag` persists tag -> name_id so a reader
    // can still resolve a tag back to a name without the tag itself carrying text.
    const relTags = new Map<string, number>();
    cfg.bridge.program.rels.forEach((decl, tag) => relTags.set(decl.name, tag));
    const tagRows = cfg.bridge.program.rels.map((decl, tag) => `(${tag},${store.intern(decl.name)})`);
    await store.flush_strings();

    for (const decl of cfg.bridge.program.rels) {
      await rel_tables.create_rel_table(
        db,
        `relbase_${decl.name}`,
        relBaseColumns(decl, cfg.bridge.columnTypes),
        decl.columns,
        ROW_SURROGATE,
      );
    }

    const ddlStatements = [
      ...ddl(cfg.bridge.program.rels, cfg.bridge.retention, cfg.bridge.columnTypes),
      ...(cfg.extraDdl ?? []),
    ];
    await db.batch(ddlStatements, "write");

    // Persist the dense tag -> name_id mapping, one batched insert (never per-rel).
    if (tagRows.length > 0) {
      await db.execute(`INSERT INTO rel_tag(tag,name_id) VALUES ${tagRows.join(",")} ON CONFLICT DO NOTHING`);
    }

    const seedRows = literalSeedRows(store, cfg.bridge.columnTypes, cfg.bridge.literalSeeds, relDecls);
    await store.flush_strings();
    for (const [relName, rows] of seedRows) {
      const decl = relDecls.get(relName)!;
      await insertRows(db, relName, decl.columns, rows);
    }

    // The program the SQL fixpoint compiles: same rules, every body literal rewritten
    // to its interned id. Interning here may mint new dictionary strings, so flush
    // before the first evaluation reads them back.
    const storageProgram = internProgramForStorage(cfg.bridge.program, cfg.bridge.columnTypes, store);
    await store.flush_strings();

    const relTables = new Map<string, RelTable>();
    for (const decl of cfg.bridge.program.rels) {
      relTables.set(decl.name, { table: `relbase_${decl.name}`, columns: decl.columns });
    }

    // The store's Z-set fact plane. `attach` stamps cascade's cx_* and reconcile's rx_*
    // schema under the default namespace onto the same connection dl already owns.
    const relStore = await RelStore.attach(db);
    const supportEdges: SupportEdges = {
      table: relStore.ns().dep,
      tagOf: relTags,
      stride: KEY_STRIDE,
      surrogate: ROW_SURROGATE,
    };
    // Report the coverage limit once, at boot, from the real rule set rather than from a
    // guess: a negated body or an aggregate head has non-monotone support, so those heads
    // are not retractable through the graph.
    const { rulesWithoutSupport } = await evalProgramSql(db, storageProgram, relTables, supportEdges);

    const derivedRelNames = cfg.bridge.program.rels.filter((decl) => decl.origin === "IDB").map((decl) => decl.name);
    const derivedTableMirror = new Map<string, Row[]>();
    for (const relName of derivedRelNames) {
      const decl = relDecls.get(relName)!;
      derivedTableMirror.set(relName, await selectAll(db, relName, decl.columns));
    }

    const state: RuntimeState = {
      db,
      store,
      relTags,
      columnTypes: cfg.bridge.columnTypes,
      relDecls,
      retention: cfg.bridge.retention,
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
    await state.store.flush_strings();
    const rowIds = await selectSurrogates(state.db, state.relDecls, rel, decl.columns, encoded);
    if (rowIds.length === 0) return { rounds: 0, dead: [] };

    const before = new Set(await state.relStore.alive_keys());
    const rounds = await state.relStore.retract_dred(rowIds.map((rowId) => [tag, rowId] as const));
    const after = new Set(await state.relStore.alive_keys());

    const deadKeys = [...before].filter((key) => !after.has(key));
    return { rounds, dead: await resolveFactKeys(state, deadKeys) };
  }

  /** Rules whose heads carry no support edges, so they are not retractable through the
   *  graph. Empty means the whole program is covered. */
  supportCoverageGaps(): readonly { readonly head: string; readonly reason: string }[] {
    return this.state.rulesWithoutSupport;
  }

  async rows(rel: string): Promise<Row[]> {
    const decl = this.state.relDecls.get(rel);
    if (!decl) throw new Error(`DlRuntime.rows: unknown rel '${rel}'`);
    return selectAll(this.state.db, rel, decl.columns);
  }

  async dispose(): Promise<void> {
    this.keepAlive.unsubscribe();
    this.commits$.complete();
    this.state.reportsSubject.complete();
    this.state.db.close();
  }
}

// ---- static-side proof (src/0_types.ts) --------------------------------------
// `implements` above covers the instance side; this covers `boot`.
export type DlRuntimeStaticsHold = AssertTrue<typeof DlRuntime extends IDlRuntimeStatics ? true : false>;
