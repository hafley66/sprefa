/**
 * 3_runtime.ts - DlRuntime: attach db, run DDL, lower the program, run the tick loop,
 * apply derived diffs, publish deltas$.
 *
 * Contract (plan M2, tasks.d.ts): `DlRuntime.boot({dbPath, bridge, extraDdl})`;
 * `commit(batch)` is THE single write site (one call = one tick); `rows(rel)`;
 * `deltas$`. The loop is one visible rx graph of named exported operators:
 *
 *   commits$: Subject<{id, batch}>                     the ONE .next() site
 *   tick$    = commits$.pipe(concatMap(applyEdbTxn), tap(injectSources))
 *   derived$ = tick$.pipe(withLatestFrom(derivedSets$), map(diffAgainstTables),
 *                          concatMap(applyDerivedTxn))
 *   deltas$  = derived$.pipe(tap(clearScratchRels), mergeMap(events), share())
 *
 * concatMap IS the lock (ticks serialize; no interleaved commits). share() is the one
 * true fan-out point (SSE + LSP + HostRunner all read the same graph). derivedSets$
 * uses shareReplay(1): the honest spelling of "current derived state", kept hot by the
 * one permanent internal subscription boot() installs on deltas$ (never a second
 * subscription to the commits$ chain: that would double-execute every write).
 *
 * Deviation from the plan's literal marble diagram, documented: `clearScratchRels`
 * stays a synchronous `tap` (as written), but the actual `DELETE FROM rel_<name>` for a
 * retention-0 rel is issued and AWAITED inside `applyDerivedTxn`'s own transaction, one
 * stage earlier. A `tap` cannot safely await I/O before the downstream commit()
 * correlation resolves (test: "rel(0) scratch dies with its tick" requires the physical
 * delete to have already happened by the time `rows()` is called right after `await
 * commit(...)`), so `clearScratchRels` itself only does the synchronous JS-side
 * bookkeeping (resetting the source BehaviorSubject / derived mirror for the rels that
 * were just cleared) plus the commit() correlation emit. No new storage shape, no new
 * algorithm: same DELETE, sequenced to keep the tap synchronous and the promise honest.
 *
 * Conversion boundary (owned here, per the package brief): tasks.d.ts's `Row` is a named
 * record; lower.ts's `Row` (imported here as `LowerRow`) is a positional tuple ordered by
 * `RelDecl.columns`. `rowFromTuple`/tuple-building helpers below are the one place that
 * crosses it.
 */

import { createClient, type Client } from "@libsql/client";
import {
  BehaviorSubject,
  Subject,
  Subscription,
  combineLatest,
  concatMap,
  filter,
  firstValueFrom,
  from,
  map,
  mergeMap,
  of,
  share,
  shareReplay,
  tap,
  withLatestFrom,
  type Observable,
} from "rxjs";
import { differenceWith, isEqual } from "lodash-es";

import { cascade, type Db } from "sprefa-store-engine/src/engine/engine.ts";
const { with_txn } = cascade;
import { lowerProgram, type LoweredProgram, type Row as LowerRow, type Sources } from "sprefa-store-engine/src/lower/lower.ts";
import type { RelDecl } from "sprefa-store-engine/src/lower/ast.ts";

import { ddl, rowDigest } from "./2_schema.ts";
import type {
  BridgeOk,
  DeltaEvent,
  DeltaRow,
  DlRuntime as DlRuntimeContract,
  EdbBatch,
  Retention,
  Row,
  TickReport,
  Value,
} from "../tasks.d.ts";

// ─────────────────────────────────────────────────────────────────────────────
// Internal state bag. One instance lives for the life of a DlRuntime; every exported
// pipeline stage takes it explicitly (rather than a `this`), which is what makes each
// stage a plain, independently testable function.
// ─────────────────────────────────────────────────────────────────────────────

export interface RuntimeState {
  readonly db: Db;
  readonly relDecls: ReadonlyMap<string, RelDecl>;
  readonly retention: ReadonlyMap<string, Retention>;
  readonly sourceSubjects: ReadonlyMap<string, BehaviorSubject<LowerRow[]>>;
  readonly derivedRelNames: readonly string[];
  /** Mirrors the on-disk current content of every derived (IDB) rel. Updated only by
   *  applyDerivedTxn/clearScratchRels, right after the matching SQL commits, so
   *  diffAgainstTables can stay a synchronous `map` instead of an async DB read. */
  readonly derivedTableMirror: Map<string, Row[]>;
  readonly reportsSubject: Subject<{ readonly id: number; readonly report: TickReport }>;
  /** Bumped once per tick by injectSources; stamped onto derivedSets$ snapshots so
   *  diffAgainstTables can assert the sync-settle invariant (task 2.2). */
  generation: number;
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
  readonly newFullSets: ReadonlyMap<string, readonly LowerRow[]>;
  readonly changedRelNames: readonly string[];
  /** Set by injectSources (mutated in place: it runs as a `tap`, downstream of this
   *  emission, and must stamp the generation it just bumped onto the SAME object). */
  generationAfterInject: number;
}

interface DerivedSnapshot {
  readonly generation: number;
  readonly sets: Readonly<Record<string, readonly Row[]>>;
}

interface PerRelDiff {
  readonly insert: readonly Row[];
  readonly retract: readonly Row[];
  readonly newRows: readonly Row[];
}

interface DerivedDiff {
  readonly id: number;
  readonly tick: number;
  readonly edbChangedPairs: readonly [rel: string, delta: number][];
  readonly edbEvents: readonly DeltaEvent[];
  readonly perRel: ReadonlyMap<string, PerRelDiff>;
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

/** The one conversion-boundary crossing: a positional lower.ts tuple -> a named Row. */
function rowFromTuple(tuple: LowerRow, columns: readonly string[]): Row {
  const row: Record<string, Value> = {};
  columns.forEach((column, i) => {
    row[column] = normalizeValue(tuple[i]);
  });
  return row as Row;
}

function sqlLiteral(value: Value): string {
  if (value === null || value === undefined) return "NULL";
  if (typeof value === "number") return String(value);
  if (typeof value === "boolean") return value ? "1" : "0";
  return `'${value.replace(/'/g, "''")}'`;
}

function sqlTuple(row: Row, columns: readonly string[]): string {
  return `(${columns.map((column) => sqlLiteral(row[column] ?? null)).join(",")})`;
}

function rowKeyOf(row: Row, columns: readonly string[]): string {
  return JSON.stringify(columns.map((column) => row[column] ?? null));
}

async function selectAll(db: Db, relName: string, columns: readonly string[]): Promise<Row[]> {
  const res = await db.execute(`SELECT ${columns.join(",")} FROM rel_${relName}`);
  return res.rows.map((rawRow) => rowFromRaw(rawRow, columns));
}

async function selectAllTuples(db: Db, relName: string, columns: readonly string[]): Promise<LowerRow[]> {
  const res = await db.execute(`SELECT ${columns.join(",")} FROM rel_${relName}`);
  return res.rows.map((rawRow) => {
    const raw = rawRow as Record<string, unknown>;
    return columns.map((column) => normalizeValue(raw[column])) as LowerRow;
  });
}

/** The batched pre-check SELECT law (ingest.ts note 3): one SELECT over the candidate
 *  tuples, never a per-row existence check. */
async function preCheckExistingKeys(
  db: Db,
  relName: string,
  columns: readonly string[],
  candidates: readonly Row[],
): Promise<Set<string>> {
  if (candidates.length === 0) return new Set();
  const values = candidates.map((row) => sqlTuple(row, columns)).join(",");
  const res = await db.execute(
    `SELECT ${columns.join(",")} FROM rel_${relName} WHERE (${columns.join(",")}) IN (VALUES ${values})`,
  );
  const keys = new Set<string>();
  for (const rawRow of res.rows) keys.add(rowKeyOf(rowFromRaw(rawRow, columns), columns));
  return keys;
}

async function insertRows(db: Db, relName: string, columns: readonly string[], rows: readonly Row[]): Promise<void> {
  if (rows.length === 0) return;
  const values = rows.map((row) => sqlTuple(row, columns)).join(",");
  await db.execute(`INSERT INTO rel_${relName}(${columns.join(",")}) VALUES ${values} ON CONFLICT DO NOTHING`);
}

async function deleteRows(db: Db, relName: string, columns: readonly string[], rows: readonly Row[]): Promise<void> {
  if (rows.length === 0) return;
  const values = rows.map((row) => sqlTuple(row, columns)).join(",");
  await db.execute(`DELETE FROM rel_${relName} WHERE (${columns.join(",")}) IN (VALUES ${values})`);
}

async function insertDeltaRows(db: Db, rows: readonly DeltaRow[]): Promise<void> {
  if (rows.length === 0) return;
  const values = rows
    .map((row) => `('${row.rel.replace(/'/g, "''")}',${row.row_digest},${row.tick},${row.weight})`)
    .join(",");
  await db.execute(`INSERT INTO delta(rel,row_digest,tick,weight) VALUES ${values}`);
}

// ─────────────────────────────────────────────────────────────────────────────
// applyEdbTxn: tick++, EDB pre-check + writes, delta rows, one atomic transaction.
// rel(1) retention ("keep only the newest row") is enforced here, on the EDB write.
// ─────────────────────────────────────────────────────────────────────────────

export async function applyEdbTxn(state: RuntimeState, request: CommitRequest): Promise<EdbTickOutcome> {
  const { db } = state;
  return with_txn(db, async () => {
    const tickRes = await db.execute("UPDATE store_meta SET value = value + 1 WHERE key='tick' RETURNING value");
    const tick = Number(tickRes.rows[0]?.[0] ?? 0);

    const relNames = new Set<string>([...request.batch.insert.keys(), ...request.batch.retract.keys()]);
    const changedPairs: [string, number][] = [];
    const events: DeltaEvent[] = [];
    const newFullSets = new Map<string, readonly LowerRow[]>();
    const changedRelNames: string[] = [];
    const deltaRows: DeltaRow[] = [];

    for (const relName of relNames) {
      const decl = state.relDecls.get(relName);
      if (!decl) throw new Error(`commit: unknown rel '${relName}'`);
      const columns = decl.columns;
      const insertCandidates = request.batch.insert.get(relName) ?? [];
      const retractCandidates = request.batch.retract.get(relName) ?? [];

      const existingForInsert = await preCheckExistingKeys(db, relName, columns, insertCandidates);
      const genuinelyNew = insertCandidates.filter((row) => !existingForInsert.has(rowKeyOf(row, columns)));

      const existingForRetract = await preCheckExistingKeys(db, relName, columns, retractCandidates);
      const genuinelyRetracted = retractCandidates.filter((row) => existingForRetract.has(rowKeyOf(row, columns)));

      let additionalRetracts: Row[] = [];
      const isLatestOnly = (state.retention.get(relName) ?? "all") === 1;
      if (isLatestOnly && insertCandidates.length > 0) {
        const fullBefore = await selectAll(db, relName, columns);
        const installedKeys = new Set(insertCandidates.map((row) => rowKeyOf(row, columns)));
        const alreadyRetractedKeys = new Set(genuinelyRetracted.map((row) => rowKeyOf(row, columns)));
        additionalRetracts = fullBefore.filter(
          (row) => !installedKeys.has(rowKeyOf(row, columns)) && !alreadyRetractedKeys.has(rowKeyOf(row, columns)),
        );
      }

      if (genuinelyNew.length > 0) await insertRows(db, relName, columns, genuinelyNew);
      const allRetracted = [...genuinelyRetracted, ...additionalRetracts];
      if (allRetracted.length > 0) await deleteRows(db, relName, columns, allRetracted);

      for (const row of genuinelyNew) {
        deltaRows.push({ rel: relName, row_digest: rowDigest(row, columns), tick, weight: 1 });
      }
      for (const row of allRetracted) {
        deltaRows.push({ rel: relName, row_digest: rowDigest(row, columns), tick, weight: -1 });
      }

      const net = genuinelyNew.length - allRetracted.length;
      if (net !== 0) changedPairs.push([relName, net]);

      if (genuinelyNew.length > 0 || allRetracted.length > 0) {
        changedRelNames.push(relName);
        newFullSets.set(relName, await selectAllTuples(db, relName, columns));
        events.push({ tick, rel: relName, inserts: genuinelyNew, retracts: allRetracted });
      }
    }

    await insertDeltaRows(db, deltaRows);

    return {
      id: request.id,
      tick,
      changedPairs,
      events,
      newFullSets,
      changedRelNames,
      generationAfterInject: -1,
    };
  });
}

// ─────────────────────────────────────────────────────────────────────────────
// injectSources: BehaviorSubject.next(full current tuple set) for each changed EDB rel,
// synchronously. No I/O here: applyEdbTxn already computed the fresh tuple sets.
// ─────────────────────────────────────────────────────────────────────────────

export function injectSources(state: RuntimeState, outcome: EdbTickOutcome): void {
  if (outcome.changedRelNames.length === 0) {
    // nothing pushed this tick (e.g. an idempotent re-commit): derivedSets$'s cached
    // snapshot is still correct as-is, so there is nothing new to wait for.
    outcome.generationAfterInject = state.generation;
    return;
  }
  // Bump BEFORE pushing: each BehaviorSubject.next() below synchronously cascades all
  // the way through derivedSets$'s map stage (combineLatest -> map -> shareReplay(1)
  // are all sync operators over sync sources), which stamps whatever `state.generation`
  // reads AT THAT MOMENT. Bumping first is what lets the stamped value already reflect
  // this tick by the time this function returns.
  state.generation += 1;
  for (const relName of outcome.changedRelNames) {
    const subject = state.sourceSubjects.get(relName);
    const tuples = outcome.newFullSets.get(relName);
    if (subject && tuples) subject.next(tuples as LowerRow[]);
  }
  outcome.generationAfterInject = state.generation;
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
// diffAgainstTables: the rx pipeline stage. Asserts the sync-settle invariant (task
// 2.2), then runs diffDerivedRel per derived rel against the in-memory table mirror.
// ─────────────────────────────────────────────────────────────────────────────

export function diffAgainstTables(state: RuntimeState, outcome: EdbTickOutcome, snapshot: DerivedSnapshot): DerivedDiff {
  // The assertion only means something when (a) an EDB rel actually changed this tick
  // (otherwise there was nothing to push, and the cached snapshot is trivially still
  // correct) and (b) at least one derived rel exists to have possibly reacted (a
  // program with none lowers derivedSets$ to a one-shot `of({})`, stamped once at boot
  // and never expected to move again).
  const somethingCouldHaveMoved = outcome.changedRelNames.length > 0 && state.derivedRelNames.length > 0;
  if (somethingCouldHaveMoved && snapshot.generation !== outcome.generationAfterInject) {
    throw new Error(
      `sync-settle assertion violated: derivedSets$ generation ${snapshot.generation} ` +
        `!= tick generation ${outcome.generationAfterInject} ` +
        "(a derived rel observed a stale source snapshot; the lowered rx graph did not settle synchronously)",
    );
  }
  const perRel = new Map<string, PerRelDiff>();
  for (const relName of state.derivedRelNames) {
    const newRows = snapshot.sets[relName] ?? [];
    const oldRows = state.derivedTableMirror.get(relName) ?? [];
    const diff = diffDerivedRel(oldRows, newRows);
    if (diff.insert.length > 0 || diff.retract.length > 0) {
      perRel.set(relName, { insert: diff.insert, retract: diff.retract, newRows });
    }
  }
  return { id: outcome.id, tick: outcome.tick, edbChangedPairs: outcome.changedPairs, edbEvents: outcome.events, perRel };
}

// ─────────────────────────────────────────────────────────────────────────────
// applyDerivedTxn: derived writes + delta rows, one atomic transaction. Also runs the
// retention-0 physical cleanup here (see the file header note on why the DELETE lands
// in this awaited transaction rather than in the later clearScratchRels tap).
// ─────────────────────────────────────────────────────────────────────────────

export async function applyDerivedTxn(state: RuntimeState, diff: DerivedDiff): Promise<SettledOutcome> {
  const { db } = state;
  return with_txn(db, async () => {
    const derivedEvents: DeltaEvent[] = [];
    const derivedPairs: [string, number][] = [];
    const deltaRows: DeltaRow[] = [];

    for (const [relName, { insert, retract, newRows }] of diff.perRel) {
      const decl = state.relDecls.get(relName);
      if (!decl) throw new Error(`applyDerivedTxn: unknown derived rel '${relName}'`);
      const columns = decl.columns;
      if (insert.length > 0) await insertRows(db, relName, columns, insert);
      if (retract.length > 0) await deleteRows(db, relName, columns, retract);
      for (const row of insert) deltaRows.push({ rel: relName, row_digest: rowDigest(row, columns), tick: diff.tick, weight: 1 });
      for (const row of retract) deltaRows.push({ rel: relName, row_digest: rowDigest(row, columns), tick: diff.tick, weight: -1 });
      const net = insert.length - retract.length;
      if (net !== 0) derivedPairs.push([relName, net]);
      derivedEvents.push({ tick: diff.tick, rel: relName, inserts: insert, retracts: retract });
      state.derivedTableMirror.set(relName, newRows.slice());
    }

    if (deltaRows.length > 0) await insertDeltaRows(db, deltaRows);

    const scratchRelNames: string[] = [];
    for (const [relName, retention] of state.retention) {
      if (retention === 0 && state.relDecls.has(relName)) scratchRelNames.push(relName);
    }
    for (const relName of scratchRelNames) {
      await db.execute(`DELETE FROM rel_${relName}`);
    }

    const changed = [...diff.edbChangedPairs, ...derivedPairs].sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0));
    const events = [...diff.edbEvents, ...derivedEvents].sort((a, b) => (a.rel < b.rel ? -1 : a.rel > b.rel ? 1 : 0));

    return { id: diff.id, report: { tick: diff.tick, changed }, events, scratchRelNames };
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
    if (!decl) continue;
    if (decl.origin === "EDB") {
      state.sourceSubjects.get(relName)?.next([]);
    } else {
      state.derivedTableMirror.set(relName, []);
    }
  }
  state.reportsSubject.next({ id: outcome.id, report: outcome.report });
}

// ─────────────────────────────────────────────────────────────────────────────
// boot() helpers.
// ─────────────────────────────────────────────────────────────────────────────

function literalSeedStatements(literalSeeds: ReadonlyMap<string, Value>, relDecls: ReadonlyMap<string, RelDecl>): string[] {
  const statements: string[] = [];
  for (const [relName, value] of literalSeeds) {
    const decl = relDecls.get(relName);
    if (!decl) throw new Error(`DlRuntime.boot: literal seed rel '${relName}' has no decl`);
    const column = decl.columns[0];
    if (column === undefined) throw new Error(`DlRuntime.boot: literal seed rel '${relName}' has no columns`);
    statements.push(`INSERT OR IGNORE INTO rel_${relName}(${column}) VALUES (${sqlLiteral(value)})`);
  }
  return statements;
}

// ─────────────────────────────────────────────────────────────────────────────
// DlRuntime: the public class. Owns exactly one RuntimeState + the commits$ Subject +
// the one permanent subscription that keeps the whole graph hot. No module-level
// mutable state; every field here is instance-scoped.
// ─────────────────────────────────────────────────────────────────────────────

export class DlRuntime implements DlRuntimeContract {
  private readonly commits$: Subject<CommitRequest>;
  private readonly keepAlive: Subscription;
  private nextCommitId: number;
  readonly deltas$: Observable<DeltaEvent>;

  private constructor(
    private readonly state: RuntimeState,
    lowered: LoweredProgram,
  ) {
    this.commits$ = new Subject<CommitRequest>();
    this.nextCommitId = 1;

    const namedDerived: Record<string, Observable<Row[]>> = {};
    for (const relName of state.derivedRelNames) {
      const decl = state.relDecls.get(relName)!;
      const raw$ = lowered.rels.get(relName);
      if (!raw$) throw new Error(`DlRuntime: derived rel '${relName}' missing from lowered program`);
      namedDerived[relName] = raw$.pipe(map((tuples) => tuples.map((tuple) => rowFromTuple(tuple, decl.columns))));
    }
    const derivedSets$: Observable<DerivedSnapshot> = (
      state.derivedRelNames.length > 0 ? combineLatest(namedDerived) : of({} as Record<string, Row[]>)
    ).pipe(
      map((sets) => ({ generation: state.generation, sets })),
      shareReplay(1),
    );

    const tick$ = this.commits$.pipe(
      concatMap((request) => applyEdbTxn(state, request)),
      tap((outcome) => injectSources(state, outcome)),
    );

    const derived$ = tick$.pipe(
      withLatestFrom(derivedSets$),
      map(([outcome, snapshot]) => diffAgainstTables(state, outcome, snapshot)),
      concatMap((diff) => applyDerivedTxn(state, diff)),
    );

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
    const db = createClient({ url: `file:${cfg.dbPath}` }) as unknown as Db;

    const relDecls = new Map<string, RelDecl>();
    for (const decl of cfg.bridge.program.rels) relDecls.set(decl.name, decl);

    const ddlStatements = [...ddl(cfg.bridge.program.rels, cfg.bridge.retention), ...(cfg.extraDdl ?? [])];
    await db.batch(ddlStatements, "write");

    const seedStatements = literalSeedStatements(cfg.bridge.literalSeeds, relDecls);
    if (seedStatements.length > 0) await db.batch(seedStatements, "write");

    const sourceSubjects = new Map<string, BehaviorSubject<LowerRow[]>>();
    const sourcesForLower: Sources = new Map();
    for (const decl of cfg.bridge.program.rels) {
      if (decl.origin !== "EDB") continue;
      const tuples = await selectAllTuples(db, decl.name, decl.columns);
      const subject = new BehaviorSubject<LowerRow[]>(tuples);
      sourceSubjects.set(decl.name, subject);
      sourcesForLower.set(decl.name, subject);
    }

    const lowered = lowerProgram(cfg.bridge.program, sourcesForLower);
    if (lowered.deferred.length > 0) {
      const names = lowered.deferred.map((deferred) => deferred.stratum.rels.join(",")).join("; ");
      throw new Error(`recursive strata not in this slice: ${names}`);
    }

    const derivedRelNames = cfg.bridge.program.rels.filter((decl) => decl.origin === "IDB").map((decl) => decl.name);
    const derivedTableMirror = new Map<string, Row[]>();
    for (const relName of derivedRelNames) {
      const decl = relDecls.get(relName)!;
      derivedTableMirror.set(relName, await selectAll(db, relName, decl.columns));
    }

    const state: RuntimeState = {
      db,
      relDecls,
      retention: cfg.bridge.retention,
      sourceSubjects,
      derivedRelNames,
      derivedTableMirror,
      reportsSubject: new Subject(),
      generation: 0,
    };

    return new DlRuntime(state, lowered);
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
    for (const subject of this.state.sourceSubjects.values()) subject.complete();
    this.state.db.close();
  }
}
