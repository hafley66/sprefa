/**
 * 3_runtime.ts - DlRuntime: attach db, run DDL, evaluate the program's fixpoint in
 * SQLite, apply derived diffs, publish deltas$.
 *
 * Contract (plan M2, tasks.d.ts): `DlRuntime.boot({dbPath, bridge, extraDdl})`;
 * `commit(batch)` is THE single write site (one call = one tick); `rows(rel)`;
 * `deltas$`. The loop is one visible rx graph of named exported operators:
 *
 *   commits$: Subject<{id, batch}>                     the ONE .next() site
 *   tick$    = commits$.pipe(concatMap(applyEdbTxn))
 *   derived$ = tick$.pipe(concatMap(applyDerivedTxn))
 *   deltas$  = derived$.pipe(tap(clearScratchRels), mergeMap(events), share())
 *
 * concatMap IS the lock (ticks serialize; no interleaved commits). share() is the one
 * true fan-out point (SSE + LSP + HostRunner all read the same graph), kept hot by the
 * one permanent internal subscription the constructor installs on deltas$ (never a
 * second subscription to the commits$ chain: that would double-execute every write).
 *
 * M11 (2026-07-25) — THE SQL FIXPOINT IS THE EVALUATOR. `applyDerivedTxn` now calls
 * `evalProgramSql` (sprefa-store lower/lowerSql.ts): the program's strata are evaluated
 * by SQLite over the `relbase_*` tables, with a semi-naive delta loop for a recursive
 * stratum. What this replaced: `lowerProgram` (lower.ts's in-memory rx fixpoint) driving
 * a `combineLatest` of per-rel `BehaviorSubject`s, whose full row sets crossed into the
 * JS heap on every tick, and which THREW at boot on any recursive stratum
 * ("recursive strata not in this slice"). Recursion now just runs. Three pipeline stages
 * died with it and are deliberately not kept as no-ops: `injectSources` (there are no
 * source subjects left to push into), `derivedSets$`/`DerivedSnapshot` (the current
 * derived state is the tables, not a shareReplay), and the `generation` sync-settle
 * assertion (the fixpoint is now `await`ed inside the tick's own concatMap, so a stale
 * read is not expressible). `diffDerivedRel` and `diffAgainstTables` survive unchanged in
 * meaning: the tick still publishes a Z-set diff against the previous rowset, which is
 * what makes retraction visible to deltas$ and to the `delta` log.
 *
 * The fixpoint is a FULL RECOMPUTE per tick (evalProgramSql DELETEs and refills every
 * rule-headed IDB table), skipped entirely when the tick changed no EDB row. Incremental
 * maintenance is a later arc; the tick's OUTPUT is already incremental because the diff
 * is taken against `derivedTableMirror`.
 *
 * Conversion boundary (owned here, per the package brief): the surface plane is text
 * (0_types.ts's `Row`, a named record read back through the `rel_*` decode views); the
 * storage plane is interned integers (`relbase_*`). `internProgramForStorage` below is
 * the one place a PROGRAM crosses it — every literal in a rule body is rewritten to its
 * stored id so lowerSql, which is interning-agnostic by design, compiles integer-vs-
 * integer comparisons. `encodeSurfaceRowByColumns` is the same crossing for a ROW.
 */

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
const { with_txn } = cascade;
import { rels as rel_tables } from "sprefa-store-engine/src/engine/spine.ts";
import { Store, open_db } from "sprefa-store-engine/src/engine/lib.ts";
import { evalProgramSql, type RelTable, type RelTables } from "sprefa-store-engine/src/lower/lowerSql.ts";
import type { Arg, BodyPred, LitValue, Program, RelDecl, Rule } from "sprefa-store-engine/src/lower/ast.ts";

import { ddl, relBaseColumns, rowDigest } from "./2_schema.ts";
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

// ─────────────────────────────────────────────────────────────────────────────
// Internal state bag. One instance lives for the life of a DlRuntime; every exported
// pipeline stage takes it explicitly (rather than a `this`), which is what makes each
// stage a plain, independently testable function.
// ─────────────────────────────────────────────────────────────────────────────

export interface RuntimeState {
  readonly db: Db;
  readonly store: Store;
  readonly relIds: ReadonlyMap<string, number>;
  readonly columnTypes: ReadonlyMap<string, readonly ColumnType[]>;
  readonly relDecls: ReadonlyMap<string, RelDecl>;
  readonly retention: ReadonlyMap<string, Retention>;
  readonly derivedRelNames: readonly string[];
  /** The program as the STORAGE plane sees it: every body literal rewritten to its
   *  interned id (`internProgramForStorage`). This is what evalProgramSql compiles. */
  readonly storageProgram: Program;
  /** rel name -> the physical `relbase_*` table + its columns, evalProgramSql's map. */
  readonly relTables: RelTables;
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

// ─────────────────────────────────────────────────────────────────────────────
// Value / row plumbing shared by every stage below.
// ─────────────────────────────────────────────────────────────────────────────

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

// ─────────────────────────────────────────────────────────────────────────────
// internProgramForStorage: the PROGRAM's crossing of the surface/storage boundary.
//
// lowerSql compiles against table + column names and joins on equality, so it runs
// unchanged over interned-INTEGER tables — with one exception it cannot handle itself:
// a LITERAL written in the source program is a surface value ("console.log", "warn"),
// and comparing it to a `relbase_*` column would compare a string to a dictionary id
// and match nothing. So every literal in a rule body is rewritten HERE, once at boot,
// to the id that column stores. Rewriting is pure data (the AST is plain JSON), and the
// program is static for the life of the runtime, so this runs exactly once.
//
// Two shapes are REFUSED rather than silently mis-answered, because interning destroys
// the property they need. Both throw at boot, naming the rel and the column:
//   - an ordering comparison (< <= > >=) against a TEXT column: dictionary ids are
//     assigned in first-intern order, so id order is not string order. `=`/`!=` are
//     safe (interning is injective) and stay.
//   - max/min/sum over a TEXT column: same reason, plus the sum of ids is meaningless.
//     `count` is safe on any column (the -1 NULL sentinel means no column is ever SQL
//     NULL, so COUNT(col) and COUNT(*) agree).
// ─────────────────────────────────────────────────────────────────────────────

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

// ─────────────────────────────────────────────────────────────────────────────
// Row-set matching via a shared temp table (M9-before fix, 2026-07-24): the prior
// scheme built one `(col IS lit AND ...)` group per candidate row, OR-joined into a
// single WHERE clause. `(cols) IN (VALUES ...)` was never usable here — SQL row-value
// comparison yields NULL (not true) when any column is NULL, so stored rows with NULL
// columns (node.name, site.callee_path, const.field are all pinned nullable) would
// silently never match, and the physical DELETE would skip them (M3 integration find,
// 2026-07-24) — so `IS` (SQLite's NULL-safe equality) stayed, but the OR-of-groups
// SHAPE scaled the SQL expression tree with candidate_rows x columns. An ordinary
// ~11KB source file (node_modules/rxjs/src/index.ts, 1783 span_line rows) blew past
// this build's compiled-in SQLITE_LIMIT_EXPR_DEPTH (`PRAGMA compile_options` reports
// MAX_EXPR_DEPTH=1000) on a single commit's pre-check, crashing ingest of any file
// whose single-rel insert batch cleared roughly 150-250 rows.
//
// Fix: load the candidate rows into ONE shared temp table (generic columns
// c0..c<width-1>, positionally matching whatever `columns` a call passes) and JOIN
// against it with the same NULL-safe `IS` per column, AND-joined — tree depth is now
// O(columns), independent of row count. ONE temp table name is safe to reuse across
// every rel and every call: a DlRuntime holds exactly one connection, and every
// commit is serialized through the tick pipeline's concatMap (file header above), so
// there is never a concurrent writer to race. Width is sized to the WIDEST rel this
// runtime's relDecls know about; a narrower rel's call just leaves the unused higher
// columns alone (never referenced in the join condition below).
// ─────────────────────────────────────────────────────────────────────────────

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
    .map((row) => `(${row.rel_id},${row.row_digest},${row.tick},${row.weight})`)
    .join(",");
  await db.execute(`INSERT INTO delta(rel_id,row_digest,tick,weight) VALUES ${values}`);
}

// ─────────────────────────────────────────────────────────────────────────────
// applyEdbTxn: tick++, EDB pre-check + writes, delta rows, one atomic transaction.
// rel(1) retention ("keep only the newest row") is enforced here, on the EDB write.
// ─────────────────────────────────────────────────────────────────────────────

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

      for (const row of genuinelyNew) {
        deltaRows.push({ rel_id: state.relIds.get(relName)!, row_digest: rowDigest(row, columns), tick, weight: 1 });
      }
      for (const row of allRetracted) {
        deltaRows.push({ rel_id: state.relIds.get(relName)!, row_digest: rowDigest(row, columns), tick, weight: -1 });
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

// ─────────────────────────────────────────────────────────────────────────────
// diffDerivedRel: the pure, exported, unit-testable diff primitive. lodash
// differenceWith over (old, new) per rel. The combinatorics: in-old/in-new = noop
// (appears in neither result), in-old-only = retract, in-new-only = insert.
// ─────────────────────────────────────────────────────────────────────────────

export function diffDerivedRel(
  oldRows: readonly Row[],
  newRows: readonly Row[],
): { readonly insert: readonly Row[]; readonly retract: readonly Row[] } {
  return {
    insert: differenceWith(newRows as Row[], oldRows as Row[], isEqual),
    retract: differenceWith(oldRows as Row[], newRows as Row[], isEqual),
  };
}

// ─────────────────────────────────────────────────────────────────────────────
// diffAgainstTables: read every derived rel back off disk (through the rel_* decode
// views, so the diff and the mirror both stay in the TEXT surface) and diffDerivedRel
// it against the mirror. Called AFTER evalProgramSql has refilled the tables, inside
// the same transaction, so "the tables" and "the fixpoint result" are the same thing.
// ─────────────────────────────────────────────────────────────────────────────

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

// ─────────────────────────────────────────────────────────────────────────────
// applyDerivedTxn: THE FIXPOINT STAGE. One atomic transaction holding, in order:
//   1. evalProgramSql  — the stratified least fixpoint, run by SQLite over relbase_*
//                        (semi-naive delta loop for a recursive stratum). Rows never
//                        enter the JS heap here.
//   2. diffAgainstTables — read the settled tables back, diff vs the previous rowset.
//   3. delta rows       — the Z-set log (+1 insert, -1 retract) the whole retraction
//                        story is read from.
//   4. retention-0 cleanup — the physical DELETE, awaited HERE rather than in the later
//                        clearScratchRels tap (see below).
//
// Deviation from the plan's literal marble diagram, documented since M2 and still true:
// a `tap` cannot safely await I/O before the downstream commit() correlation resolves
// (test "rel(0) scratch dies with its tick" calls rows() right after `await commit(...)`
// and requires the delete to have already happened), so the DELETE lives here and
// `clearScratchRels` only does the synchronous JS-side bookkeeping.
//
// The skip: when the tick changed no EDB row AND the previous tick cleared no scratch
// row, the EDB is bit-identical to the one the last fixpoint ran over, so the fixpoint
// is a provable no-op and is skipped whole. That is what keeps an idempotent re-commit
// at zero deltas without paying for a full recompute.
// ─────────────────────────────────────────────────────────────────────────────

export async function applyDerivedTxn(state: RuntimeState, outcome: EdbTickOutcome): Promise<SettledOutcome> {
  const { db, relDecls } = state;
  const edbMoved = outcome.changedRelNames.length > 0 || state.scratchClearedRows > 0;
  state.scratchClearedRows = 0;

  return with_txn(db, async () => {
    if (edbMoved && state.derivedRelNames.length > 0) {
      await evalProgramSql(db, state.storageProgram, state.relTables);
    }
    const perRel = edbMoved ? await diffAgainstTables(state) : new Map<string, PerRelDiff>();

    const derivedEvents: DeltaEvent[] = [];
    const derivedPairs: [string, number][] = [];
    const deltaRows: DeltaRow[] = [];

    for (const [relName, { insert, retract, newRows }] of perRel) {
      const columns = relDecls.get(relName)!.columns;
      const relId = state.relIds.get(relName)!;
      for (const row of insert) {
        deltaRows.push({ rel_id: relId, row_digest: rowDigest(row, columns), tick: outcome.tick, weight: 1 });
      }
      for (const row of retract) {
        deltaRows.push({ rel_id: relId, row_digest: rowDigest(row, columns), tick: outcome.tick, weight: -1 });
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

// ─────────────────────────────────────────────────────────────────────────────
// clearScratchRels: synchronous tap. The physical DELETE already happened (and was
// awaited) inside applyDerivedTxn; this stage only resets the JS-side mirror/source for
// the rels that were just cleared, then emits the settled report for commit()'s
// correlation (reportsSubject.next). Ordering here is what lets commit()'s promise
// resolve only after a rel(0) rel has truly gone empty.
// ─────────────────────────────────────────────────────────────────────────────

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

// ─────────────────────────────────────────────────────────────────────────────
// boot() helpers.
// ─────────────────────────────────────────────────────────────────────────────

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

// ─────────────────────────────────────────────────────────────────────────────
// DlRuntime: the public class. Owns exactly one RuntimeState + the commits$ Subject +
// the one permanent subscription that keeps the whole graph hot. No module-level
// mutable state; every field here is instance-scoped.
// ─────────────────────────────────────────────────────────────────────────────

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
    const relIds = new Map<string, number>();
    for (const decl of cfg.bridge.program.rels) relIds.set(decl.name, store.intern(decl.name));
    await store.flush_strings();

    for (const decl of cfg.bridge.program.rels) {
      await rel_tables.create_rel_table(
        db,
        `relbase_${decl.name}`,
        relBaseColumns(decl, cfg.bridge.columnTypes),
        decl.columns,
      );
    }

    const ddlStatements = [
      ...ddl(cfg.bridge.program.rels, cfg.bridge.retention, cfg.bridge.columnTypes),
      ...(cfg.extraDdl ?? []),
    ];
    await db.batch(ddlStatements, "write");

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

    const derivedRelNames = cfg.bridge.program.rels.filter((decl) => decl.origin === "IDB").map((decl) => decl.name);
    const derivedTableMirror = new Map<string, Row[]>();
    for (const relName of derivedRelNames) {
      const decl = relDecls.get(relName)!;
      derivedTableMirror.set(relName, await selectAll(db, relName, decl.columns));
    }

    const state: RuntimeState = {
      db,
      store,
      relIds,
      columnTypes: cfg.bridge.columnTypes,
      relDecls,
      retention: cfg.bridge.retention,
      derivedRelNames,
      storageProgram,
      relTables,
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
