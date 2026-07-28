/**
 * types.ts — the tsv2 runtime surface, C-header style (mirrors v6/dl's
 * 0_types.ts / v6/sprefa-store's engine/types.ts). Every class/namespace
 * object in runtime/ binds to an interface declared here first; nothing in
 * this file has a body.
 *
 * PINNED SEAM (plan 2026-07-27-tsv2-compile-target-header.md, coordinator
 * addendum): `IGenProgram` is the contract every gen/*.ts file implements and
 * the only thing the tick fold consumes. Its five fields (name, ddl,
 * relColumns, arrivalTargets, tick) are frozen — extend by adding fields,
 * never by renaming these. A future prolog emitter's output runs on this
 * runtime unchanged.
 *
 * Reuse law (plan header "the reuse law"): this file imports its connection
 * and driver-seam types from sprefa-store-engine's own header
 * (engine/types.ts) rather than re-declaring them.
 */

import type { Observable } from "rxjs";
import type { ISqlRunner, QueryResult, SqliteDb, SqlStatement, TraceStatement } from "sprefa-store-engine/src/engine/types.ts";

// re-exported so runtime/gen files can name the connection and statement
// types without a second import line into the store package
export type { QueryResult, SqliteDb, SqlStatement, TraceStatement };

// ─────────────────────────────────────────────────────────────────────────────
// Values / rows.
// ─────────────────────────────────────────────────────────────────────────────

/**
 * One SQLite column value as it crosses the driver seam. Prolog integers
 * become JS numbers; every other value (atoms and canonicalized compound
 * terms alike) is already rendered to its final text by the SQL that
 * produced it, so there is no separate "compound" value shape here — see
 * FIXTURES.md / engine.pl body_atoms and the tick-log envelope in the plan
 * header, item 9: atoms and compound-term text both serialize as JSON
 * strings.
 */
export type IRowValue = string | number;

/** One relation row, columns in the rel's declared order (relColumns). */
export type IRow = readonly IRowValue[];

export type IArrivalSign = "add" | "del";

/**
 * One signed outside-arrival row for one tick, addressed by rel name (must
 * be one of the program's `arrivalTargets`). Mirrors engine.pl's
 * `absorb_arrivals`: `+Row` into a Log rel appends with an engine stamp,
 * `+Row` into a Set rel is membership add, `-Row` is exact-row removal
 * (never valid against a Log rel).
 */
export interface IArrivalRow {
  readonly rel: string;
  readonly sign: IArrivalSign;
  readonly row: IRow;
}

/**
 * The ordered, duplicate-preserving arrival list for one tick (rulings.pl
 * q1: outside arrivals are an ORDERED list; duplicates are meaningful for
 * Log rels).
 */
export type IArrivalBatch = readonly IArrivalRow[];

// ─────────────────────────────────────────────────────────────────────────────
// The driver seam.
// ─────────────────────────────────────────────────────────────────────────────

/**
 * The one way a gen/*.ts tick chain touches SQLite: `runner` is the store's
 * existing `SqlRunner`, `db` the scratch connection it runs against.
 * Generated code never imports `@libsql/client` directly (reuse law, plan
 * header item "MECHANICAL GATE").
 */
export interface ISqlSeam {
  readonly db: SqliteDb;
  readonly runner: ISqlRunner;
}

// ─────────────────────────────────────────────────────────────────────────────
// Per-tick deltas.
// ─────────────────────────────────────────────────────────────────────────────

/**
 * One rel's boundary delta for one tick: rows added / removed at the
 * boundary (engine.pl r7 — Log rels: one +row per new stamp, a multiset;
 * Set/level rels: a full row-set diff). Unsorted, exactly as the SQL
 * returned it; `runtime/ticklog.ts` sorts and serializes.
 */
export interface IRelDelta {
  readonly rel: string;
  readonly add: readonly IRow[];
  readonly del: readonly IRow[];
}

/**
 * One tick's full delta set plus the drain signal. `carryPending` is true
 * when this tick either wrote a row via an edge rule, or grew a level rel in
 * a way that itself surfaces as a +delta this tick (engine.pl q4/R2: writes
 * and post-write level growth are next-tick trigger occurrences). The
 * runtime keeps ticking with an empty arrival batch while `carryPending`
 * holds (engine.pl `drain_cap(100)`).
 */
export interface ITickDeltas {
  readonly rels: readonly IRelDelta[];
  readonly carryPending: boolean;
}

// ─────────────────────────────────────────────────────────────────────────────
// Incremental generated-program execution.
// ─────────────────────────────────────────────────────────────────────────────

export interface IIncrementalRelationPlan {
  readonly rel: string;
  readonly kind: "log" | "set";
  readonly tableName: string;
  readonly deltaTableName: string;
  readonly columns: readonly string[];
  readonly arrivalAddSql: string | null;
  readonly boundarySql: string;
}

export interface IIncrementalEdgeStatement {
  readonly headRel: string;
  readonly headKind: "log" | "set";
  readonly headTableName: string;
  readonly headDeltaTableName: string;
  readonly headColumns: readonly string[];
  readonly keyIndices: readonly number[];
  readonly projectSql: string;
}

export interface IIncrementalLevelStatement {
  readonly headRel: string;
  readonly headDeltaTableName: string;
  readonly headColumns: readonly string[];
  readonly insertSql: string;
}

export interface IIncrementalProgramPlan {
  readonly safe: boolean;
  readonly relations: readonly IIncrementalRelationPlan[];
  readonly edges: readonly IIncrementalEdgeStatement[];
  readonly levels: readonly IIncrementalLevelStatement[];
}

export interface IIncrementalRuntime {
  clearDeltas(seam: ISqlSeam, relations: readonly IIncrementalRelationPlan[]): Observable<void>;
  applyArrivals(
    seam: ISqlSeam,
    arrivals: IArrivalBatch,
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<void>;
  applyEdges(
    seam: ISqlSeam,
    statements: readonly IIncrementalEdgeStatement[],
  ): Observable<void>;
  applyLevels(
    seam: ISqlSeam,
    statements: readonly IIncrementalLevelStatement[],
  ): Observable<void>;
  readBoundary(
    seam: ISqlSeam,
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<readonly IRelDelta[]>;
}

// ─────────────────────────────────────────────────────────────────────────────
// PINNED: the generated-program contract (do not rename these five fields).
// ─────────────────────────────────────────────────────────────────────────────

export interface IGenProgram {
  readonly name: string;
  readonly ddl: readonly string[];
  readonly relColumns: Readonly<Record<string, readonly string[]>>;
  readonly arrivalTargets: readonly string[];
  tick(seam: ISqlSeam, arrivals: IArrivalBatch): Observable<ITickDeltas>;
}

// ─────────────────────────────────────────────────────────────────────────────
// The scratch store (boot the seam, run a program's DDL once).
// ─────────────────────────────────────────────────────────────────────────────

export interface IScratchStore {
  /** Open a fresh SQLite connection (`:memory:` or `file:...`) and wrap it
   *  in a seam. Does not run any DDL. */
  open(url: string): ISqlSeam;
  /** Run every statement in `ddl`, in order, once. */
  boot(seam: ISqlSeam, ddl: readonly string[]): Observable<void>;
}

// ─────────────────────────────────────────────────────────────────────────────
// The tick fold (generic: folds an arrival schedule over any IGenProgram).
// ─────────────────────────────────────────────────────────────────────────────

/** One line of the shared oracle/tsv2 log envelope (both sides agree on
 *  this exact text; see plan header item 9 + ticklog.pl). */
export type ITickLogLine = string;

export interface ITickFold {
  /**
   * Run `program` over `schedule` (one arrival batch per tick), then keep
   * ticking with an empty batch while the program reports `carryPending`,
   * capped at `drainCap` extra ticks (engine.pl `drain_cap(100)`). Emits one
   * formatted log line per tick, schedule ticks first, drain ticks after.
   */
  run(program: IGenProgram, seam: ISqlSeam, schedule: readonly IArrivalBatch[], drainCap?: number): Observable<ITickLogLine>;
}

// ─────────────────────────────────────────────────────────────────────────────
// The log emitter (formats one tick's deltas into the envelope line).
// ─────────────────────────────────────────────────────────────────────────────

export interface ITickLogEmitter {
  /** Format `deltas` for `tick` into the canonical envelope line: rel names
   *  ascending, only nonempty rels, rows sorted by their JSON text, no
   *  spaces, LF terminated by the caller (this returns the line WITHOUT a
   *  trailing newline). */
  line(tick: number, deltas: ITickDeltas): ITickLogLine;
}
