/** Runtime contracts shared by the hand-written and emitted tsv2 modules. */

import type { Observable, SchedulerLike } from "rxjs";
import type { ISqlRunner, QueryResult, SqliteDb, SqlStatement, TraceStatement } from "sprefa-store-engine/src/engine/types.ts";

export type { QueryResult, SqliteDb, SqlStatement, TraceStatement };

/**
 * One SQLite column value as it crosses the driver seam. Prolog integers
 * become JS numbers; every other value (atoms and canonicalized compound
 * terms alike) is already rendered to its final text by the SQL that
 * produced it, so there is no separate "compound" value shape here — see
 * FIXTURES.md / engine.pl body_atoms and the tick-log envelope in the plan
 * header, item 9: atoms and compound-term text both serialize as JSON
 * strings.
 */
export type IRowValue = string | number | boolean;

/** Storage type emitted for each public relation column. Relation references
 * still cross the boundary as their canonical text value.
 *
 * `json` and `text` are the SAME storage (TEXT, and `rowValueFromSql` hands
 * both back untouched) and differ only at the tick-log encoder, which is the
 * one place the difference is observable: a `json` column's value IS a json
 * value at any top level, a `text` column's value is a string even when its
 * bytes happen to parse. Before this member existed the encoder guessed by
 * looking at the first character and got both cases wrong (json_flex lab). */
export type IRowColumnType = "text" | "int" | "bool" | "float" | "ref" | "json";

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

export interface IIncrementalRelationPlan {
  readonly rel: string;
  readonly kind: "log" | "set";
  readonly tableName: string;
  readonly deltaTableName: string;
  readonly frontierTableName: string;
  readonly nextFrontierTableName: string;
  readonly columns: readonly string[];
  readonly columnTypes?: readonly IRowColumnType[];
  readonly keyIndices?: readonly number[];
  readonly arrivalAddSql: string | null;
  readonly arrivalDelSql: string | null;
  readonly boundarySql: string;
  /**
   * The DEPARTURE frontier: last tick's net -delta rows of this rel, waiting
   * to fire the arms that bind it with `finalize/1` (engine.pl:tick/7's
   * `DepartureCarry`, gated by `listened_departure_refs/2` -- a departure is a
   * NEXT-TICK occurrence, update-arm verdict). Present ONLY on a rel some rule
   * actually listens to that way, which is what keeps every other relation's
   * DDL and statement text byte-identical to a program with no `finalize` in
   * it at all. Its own DDL and SELECT shape are the same `_phase` / `_sequence`
   * / columns the arrival frontier uses, so the arm's projectSql is the same
   * text with one table name swapped.
   */
  readonly departureFrontierTableName?: string;
}

export interface IIncrementalEdgeStatement {
  readonly headRel: string;
  /** "<program>:<name>/<arity>#<ordinal>", lower.pl:statement_rule_ids/3. What
   *  a trace line names instead of counting statements. */
  readonly ruleId: string;
  readonly headKind: "log" | "set";
  readonly headTableName: string;
  readonly headDeltaTableName: string;
  readonly headColumns: readonly string[];
  readonly keyIndices: readonly number[];
  readonly projectSql: string;
}

/**
 * The group-scoped maintenance plan for an AGGREGATE level head
 * (count/sum/min/max). An aggregate row CHANGES rather than only arriving,
 * so neither the monotone delta-join insert nor refCount reconciliation
 * applies; `insertSql` and `supportSql` are null on such a statement and this
 * runs instead.
 *
 * Four steps, all SQL-side: clear the scope table, seed it with the group
 * keys this tick's staged deltas touched (both signs), DELETE the head's rows
 * for those groups RETURNING them (the -1 events), re-derive those groups
 * and INSERT RETURNING them (the +1 events). A group whose value did not move
 * emits a -1/+1 pair for the identical row, which `boundaryDelta` cancels to
 * weight zero, so over-approximating the scope is free and only
 * under-approximating would be wrong.
 */
export interface IAggregateLevelPlan {
  readonly scopeClearSql: string;
  readonly scopeSeedSql: readonly string[];
  readonly deleteScopedSql: string;
  readonly insertScopedSql: readonly string[];
  readonly deltaMaintained?: boolean;
}

export interface IIncrementalLevelStatement {
  readonly headRel: string;
  /** "<program>:<name>/<arity>#<ordinal>", lower.pl:statement_rule_ids/3. */
  readonly ruleId: string;
  readonly headDeltaTableName: string;
  readonly headColumns: readonly string[];
  /** null exactly when `aggregateSql` is present. */
  readonly insertSql: string | null;
  readonly selectSql: string;
  readonly recomputeSql: string;
  /** null exactly when `aggregateSql` is present. */
  readonly supportSql: readonly [
    clear: string,
    seed: string,
    update: string,
    collectZero: string,
    insertNew: string,
  ] | null;
  readonly aggregateSql: IAggregateLevelPlan | null;
}

export interface IIncrementalRetentionStatement {
  readonly rel: string;
  readonly count: number;
  readonly deleteSql: string;
}

export interface IIncrementalProgramPlan {
  readonly safe: boolean;
  readonly reconcileEveryTick: boolean;
  readonly retractionGuard: "plain-count-acyclic" | "recursive-cte-reseed";
  readonly relations: readonly IIncrementalRelationPlan[];
  readonly edges: readonly IIncrementalEdgeStatement[];
  readonly levels: readonly IIncrementalLevelStatement[];
  readonly retention?: readonly IIncrementalRetentionStatement[];
}

export interface IIncrementalRuntime {
  prepareTick(seam: ISqlSeam, relations: readonly IIncrementalRelationPlan[]): Observable<void>;
  applyArrivals(
    seam: ISqlSeam,
    arrivals: IArrivalBatch,
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<void>;
  applyEdges(
    seam: ISqlSeam,
    statements: readonly IIncrementalEdgeStatement[],
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<void>;
  applyLevelsBeforeEdges(
    seam: ISqlSeam,
    statements: readonly IIncrementalLevelStatement[],
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<void>;
  recomputeLevelsBeforeEdges(
    seam: ISqlSeam,
    statements: readonly IIncrementalLevelStatement[],
    relations: readonly IIncrementalRelationPlan[],
    reconcileEveryTick: boolean,
    arrivals: IArrivalBatch,
  ): Observable<void>;
  mergeNextIntoCurrent(
    seam: ISqlSeam,
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<void>;
  applyLevelsAfterEdges(
    seam: ISqlSeam,
    statements: readonly IIncrementalLevelStatement[],
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<void>;
  applyRetention(
    seam: ISqlSeam,
    statements: readonly IIncrementalRetentionStatement[],
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<void>;
  recomputeLevelsAfterEdges(
    seam: ISqlSeam,
    statements: readonly IIncrementalLevelStatement[],
    relations: readonly IIncrementalRelationPlan[],
    reconcileEveryTick: boolean,
  ): Observable<void>;
  readBoundary(
    seam: ISqlSeam,
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<readonly IRelDelta[]>;
  stageDepartures(
    seam: ISqlSeam,
    relations: readonly IIncrementalRelationPlan[],
    deltas: readonly IRelDelta[],
  ): Observable<void>;
  promoteFrontiers(
    seam: ISqlSeam,
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<boolean>;
}

/**
 * One referenced target rel's storage plan, emitted by
 * lower.pl:struct_type_plans/2 in TOPOLOGICAL order (children before parents),
 * which is what makes the post-order intern one left fold with no recursion at
 * the SQL layer.
 *
 * `refs[i]` is the declared type name a column points at, or `null` for a
 * scalar column. Both statements are set-based over a single `json_each(?)`
 * parameter, so a tick costs two statements per type regardless of how many
 * values arrive.
 */
export interface IStructTypePlan {
  readonly name: string;
  readonly columns: readonly string[];
  readonly refs: readonly (string | null)[];
  readonly keyIndices: readonly number[];
  /** Reports a requested row whose key exists with different non-key fields. */
  readonly conflictSql: string;
  /** Batched insertion of target rows absent at their declared key. */
  readonly internSql: string;
  /** Batched dense-ID lookup by declared key. */
  readonly lookupSql: string;
}

/** Per parent rel name, the target rel of each column position (`null` for
 *  a scalar column). Only rels with at least one ref column appear. */
export type IStructRefColumns = Readonly<Record<string, readonly (string | null)[]>>;

export interface IStructPlane {
  /**
   * Resolves every relation-shaped reference value and returns parent rows
   * with each reference replaced by the target relation's dense `__id`.
   * Returns the batch unchanged, running zero statements, when the program
   * declares no referenced relations or the batch carries no reference value.
   */
  intern(
    seam: ISqlSeam,
    types: readonly IStructTypePlan[],
    refColumns: IStructRefColumns,
    arrivals: IArrivalBatch,
    applyTargets?: (arrivals: IArrivalBatch) => Observable<unknown>,
  ): Observable<IArrivalBatch>;
  /** Canonical JSON: sorted object keys, no whitespace. Transient wire and
   * boundary comparison encoding; never a stored relation payload. */
  canonicalText(value: unknown): string;
}

/** Generated program contract. The five core fields are emitter-stable. */
export interface IGenProgram {
  readonly name: string;
  readonly ddl: readonly string[];
  readonly relColumns: Readonly<Record<string, readonly string[]>>;
  readonly relColumnTypes?: Readonly<Record<string, readonly IRowColumnType[]>>;
  readonly arrivalTargets: readonly string[];
  tick(seam: ISqlSeam, arrivals: IArrivalBatch): Observable<ITickDeltas>;
}

/**
 * One emitted boot statement: SQL plus the row values it binds. `boot` is an
 * extra field beyond IGenProgram's five pinned names ("extend by adding
 * fields, never renaming"); every harness that seeds a compiled program runs
 * these after DDL and before the tick fold.
 */
export interface IBootStatement {
  readonly sql: string;
  readonly params: readonly IRowValue[];
}

export interface IBootRunner {
  /**
   * Run every boot statement in order. Integer params cross the driver seam
   * as bigint, never as a plain JS `number`: `@libsql/client` binds a JS
   * number as SQLite REAL, so a bound `1` lands in a TEXT-affinity column as
   * the text "1.0" while `1n` lands as "1" (measured, not assumed --
   * v6/tsv2/tests/bootBind.test.ts is the receipt). Same rule the emitted
   * `bindArgs` helper and 1_incremental.ts already apply on every other bind
   * path; the boot path was the one seam that bound raw.
   */
  run(seam: ISqlSeam, statements: readonly IBootStatement[]): Observable<void>;
}

export interface IScratchStore {
  /** Open a fresh SQLite connection (`:memory:` or `file:...`) and wrap it
   *  in a seam. Does not run any DDL. */
  open(url: string): ISqlSeam;
  /** Run every statement in `ddl`, in order, once. */
  boot(seam: ISqlSeam, ddl: readonly string[]): Observable<void>;
}

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

export interface ITickLogEmitter {
  /** Format `deltas` for `tick` into the canonical envelope line: rel names
   *  ascending, only nonempty rels, rows sorted by their JSON text, no
   *  spaces, LF terminated by the caller (this returns the line WITHOUT a
   *  trailing newline). */
  line(tick: number, deltas: ITickDeltas, relColumnTypes?: Readonly<Record<string, readonly IRowColumnType[]>>): ITickLogLine;
  /** ONE column value as the log encodes it: an integer as a JSON number, a
   *  `json`-typed column as a json VALUE (any top level, canonicalized with
   *  sorted keys and no whitespace), anything else as a JSON string.
   *  Exported because the final-state grading leg has to encode by the same
   *  rule as the per-tick leg -- it did not, and a relation reference
   *  rendered as canonical JSON text was the first value to make the two
   *  legs disagree.
   *
   *  `type` is the column's declared type. Omitting it means "not a json
   *  column", which is what every non-json caller wants and what a `ref`
   *  column (already canonical text, rendered as a string) wants too. */
  valueText(value: IRowValue, type?: IRowColumnType): string;
}

// ─────────────────────────────────────────────────────────────────────────────
// THE SERVED ENGINE (serve/*.ts). One header per package, so the serve layer's
// contracts are declared here beside the runtime's, each name exactly once.
//
// Everything below is about running a compiled program as a LIVE process --
// arrivals over HTTP, sh hosts that really spawn, interval binds that really
// tick. The schedule-fed grading path above is untouched and stays the referee.
// ─────────────────────────────────────────────────────────────────────────────

/** One declared world column, exactly as emit_ts.pl writes it. */
export interface IHostColumnPlan {
  readonly name: string;
  /** Primitive storage name or a referenced target rel name. The compiler's
   * shared type-plane check refuses every other spelling before emission. */
  readonly type: string;
}

/**
 * One `sh_decl` as emitted data. `execution` is a target-neutral executor key.
 * `shell` preserves template execution; `sprefa_extract` selects the existing
 * extractor contract and subprocess behavior. Unknown keys are named.
 */
export interface IHostPlan {
  readonly name: string;
  readonly inputs: readonly IHostColumnPlan[];
  readonly outputs: readonly IHostColumnPlan[];
  readonly template: string;
  readonly demandRel: string;
  readonly responseRel: string;
  readonly execution: string;
}

/**
 * One `bind_decl` as emitted data. `literals` is the program's own statement
 * about WHICH INSTANCES of the world source it consumes: the distinct literals
 * its rules read in the bind atom's first column, which registry.pl calls the
 * configuration column (emit_ts.pl `bind_read_literals/4`). `interval` reads
 * integer seconds there, `watch` reads a glob string. An empty list means the
 * program declared the bind and never asked for an instance, so nothing is
 * owed -- never a default invented out here.
 *
 * This field was `periods: readonly number[]` while `interval` was the only
 * bind; it widened when `watch` arrived rather than growing a second field for
 * the same concept. `IBindPlan` is not one of IGenProgram's five pinned names,
 * so widening it is the sanctioned direction.
 */
export interface IBindPlan {
  readonly name: string;
  readonly columns: readonly IHostColumnPlan[];
  readonly literals: readonly IRowValue[];
  readonly execution: string;
}

export interface IQueryPlan {
  readonly rel: string;
  readonly arity: number;
  /** One entry per position of the query atom: the pinned literal, or null
   *  where the position is free. */
  readonly columns: readonly (IRowValue | null)[];
  /** The pinned positions, 0-based. These are the demand keys. */
  readonly bound: readonly number[];
  readonly snapshot: "current";
}

/** The demand cone as an emitted module spells it: one "name/arity" string per
 *  rel, sorted. Seeded by the module's own query decls and closed over the rule
 *  graph (2_demand_cone.pl); a module with no query decl lists every rel it
 *  declares or mentions. Nothing reads it yet. */
export type IDemandedRel = string;

/**
 * What a compiled `.dl6` module actually exports: `IGenProgram`'s five pinned
 * fields plus the extra fields emit_ts.pl adds. The sweep and run-emitted
 * harnesses each declared their own local slice of this shape; the served
 * engine needs all of it, so the whole thing is declared once, here.
 */
export interface IServedProgram extends IGenProgram {
  readonly boot: readonly IBootStatement[];
  readonly finalSelect: Readonly<Record<string, string>>;
  readonly hostPlans: readonly IHostPlan[];
  readonly bindPlans: readonly IBindPlan[];
  readonly queryPlans: readonly IQueryPlan[];
  readonly demandedRels: readonly IDemandedRel[];
  readonly unsupportedExecution: readonly string[];
}

export interface IProgramCompiler {
  /** `.dl6` source text -> the compiled, imported program module. One swipl
   *  run per call (compile.pl `compile_dl6/2`), then a dynamic import of the
   *  emitted file. Errors carry swipl's own stderr text. */
  compile(source: string): Observable<IServedProgram>;
}

/** One tick as the served engine reports it: the number, the deltas, and the
 *  canonical log line those deltas serialize to (the same envelope the
 *  schedule-fed grading path prints, so a served run diffs against the oracle
 *  byte-for-byte). */
export interface ITickOutcome {
  readonly tick: number;
  readonly line: ITickLogLine;
  readonly deltas: ITickDeltas;
}

/**
 * The live tick loop over one compiled program.
 *
 * `ticks$` is the single shared source; the loop turns only while something
 * subscribes it (the same law `DlRuntime.deltas$` states in v6/dl). `submit`
 * enqueues one arrival batch and reports the ticks THAT BATCH caused -- the
 * settle tick plus any drain ticks that follow it.
 */
export interface ILiveEngine {
  readonly program: IServedProgram;
  readonly ticks$: Observable<ITickOutcome>;
  submit(arrivals: IArrivalBatch): Observable<ITickOutcome>;
  /** Current rows of one rel, through the program's own emitted decode SELECT
   *  (`finalSelect`). Unknown rel names fail loudly. */
  rows(rel: string): Observable<readonly IRow[]>;
}

/** Durable per-witness effect record, in the served db beside the program's
 *  own tables. The response rel alone cannot serve as the cache: a host that
 *  legitimately answers with ZERO rows leaves no response row and would refire
 *  on every boot. */
export interface IWitnessCache {
  ddl(): readonly string[];
  /** Drop rows left in 'pending' by a process that died mid-run: the cache row
   *  IS the in-flight lock, and at subscribe time this single-process runner
   *  cannot have anything in flight. */
  clearDeadLocks(seam: ISqlSeam): Observable<void>;
  /** Witness digests already answered ('done') for one host. */
  answered(seam: ISqlSeam, host: string): Observable<ReadonlySet<string>>;
  claim(seam: ISqlSeam, host: string, witnessDigest: string): Observable<void>;
  settle(seam: ISqlSeam, host: string, witnessDigest: string, state: "done" | "error", rows: number): Observable<void>;
}

/** One finished host run, as the app graph carries it. */
export interface IHostEffectDone {
  readonly host: string;
  readonly witnessDigest: string;
  readonly responseRows: number;
  readonly outcome: "done" | "cache_hit" | "error";
}

export interface IHostRunner {
  /** Cold. Boot-replays every host demand row, then follows the tick stream's
   *  demand deltas. Compatible `sprefa_extract` witnesses in one frontier
   *  share one executor invocation; generic shell witnesses remain singleton.
   *  Every decoded projection lands as an EDB arrival on its response rel. */
  readonly effects$: Observable<IHostEffectDone>;
}

/** One finished bind firing. */
export interface IBindFired {
  readonly rel: string;
  readonly period: number;
  readonly bucket: number;
  readonly tick: number;
}

export interface IIntervalBindRunner {
  /** Cold. One rx `interval` per declared period; each firing commits one
   *  arrival row. Unsubscribing IS the whole of teardown (rxjs's own
   *  `clearInterval` on the scheduled action), so a program swap's `switchMap`
   *  stops every timer with no bookkeeping here. */
  readonly firings$: Observable<IBindFired>;
}

/** One committed watcher burst: the coalesced window's arrival batch, already
 *  landed. `added`/`removed` count ROWS, not filesystem events -- a save that
 *  did not change content contributes neither. */
export interface IWatchFired {
  readonly rel: string;
  readonly glob: string;
  readonly added: number;
  readonly removed: number;
  readonly tick: number;
}

/**
 * The `watch` bind: file-change rows into one EDB rel, coalesced per window.
 *
 * SEAM LAW (golden plan phase 2, coordinator's watcher_first_impl fork): the
 * watcher LIBRARY is behind this adapter and nothing above it names one. The
 * first implementation is node's own `fsPromises.watch` (zero dependencies);
 * swapping in `@parcel/watcher` later replaces `IWatchSource` alone, because
 * what crosses this seam is already collapsed to `(glob, path, digest)` rows
 * with an arrival sign -- never a library's create/update/delete/rename
 * vocabulary.
 */
export interface IWatchBindRunner {
  /** Cold. One filesystem watch per declared glob; each coalesced window
   *  commits ONE arrival batch, so a git checkout is a handful of ticks and
   *  not one per file. */
  readonly firings$: Observable<IWatchFired>;
}

/** The swappable half: a raw path notification stream, one per watched root.
 *  Implementations emit a path per underlying filesystem event, duplicates and
 *  all -- deduping, hashing and sign assignment are the runner's, above this
 *  line, so a backend change cannot alter what the engine sees. */
export interface IWatchSource {
  /** Cold. `root` is an absolute directory; emits absolute paths. Recursive.
   *  Unsubscribing must stop the underlying watch. */
  watch(root: string): Observable<string>;
}

/** Every value the served app's one stream carries. */
export type IServeEvent =
  | {
      readonly kind: "listening";
      readonly port: number;
      /** Live SSE client count; the leak receipt asserts it returns to zero. */
      readonly activeSubscribeCount: () => number;
      readonly close: () => Observable<void>;
    }
  | { readonly kind: "loaded"; readonly program: string }
  | { readonly kind: "tick"; readonly outcome: ITickOutcome }
  | { readonly kind: "effect"; readonly done: IHostEffectDone }
  | { readonly kind: "bind"; readonly fired: IBindFired }
  | { readonly kind: "watch"; readonly fired: IWatchFired }
  | { readonly kind: "served"; readonly method: string; readonly path: string };

export interface IServeConfig {
  readonly dbUrl: string;
  readonly port: number;
  /** rxjs's own scheduler seam (the F3 precedent in v6/dl's BindConfig): real
   *  serving defaults to `asyncScheduler`, a test injects a `TestScheduler` so
   *  a bind's cadence runs on virtual time and no test sleeps on a wall clock. */
  readonly scheduler?: SchedulerLike;
  /** Directory every `watch` glob and every emitted path is relative to.
   *  Defaults to `process.cwd()`. Rows carry ROOT-RELATIVE paths so a program's
   *  findings do not move when the server does. */
  readonly watchRoot?: string;
  /** Watcher coalesce window in milliseconds (default 100). One window is one
   *  arrival batch and therefore one tick; see serve/2_binds.ts. */
  readonly watchCoalesceMs?: number;
  /** The swappable watcher backend (default `NodeWatchSource`). Injected by
   *  the watcher receipt so the seam can be driven without touching a real
   *  filesystem, and the seam a future `@parcel/watcher` adapter replaces. */
  readonly watchSource?: IWatchSource;
}

export interface IServeTsv2 {
  (config: IServeConfig): Observable<IServeEvent>;
}

/** Wire keys follow registry.pl's `trace_event/2` table: lower snake case,
 *  elapsed values in `*_ms`, key order as declared there. The spelling is the
 *  contract a second emitter reproduces, so it does NOT follow this file's
 *  camelCase; locals and parameters still do. */
export interface IServeTickEvent {
  readonly tick: number;
  readonly rels: number;
  readonly rows: number;
  readonly statements: number;
  readonly wall_ms: number;
}

export interface IServeEffectEvent {
  readonly host: string;
  readonly witness_digest: string;
  readonly outcome: "done" | "cache_hit" | "error";
  readonly rows: number;
  readonly wall_ms: number;
  readonly error?: string;
}

/** One emitted statement that ran, named by the rule it was lowered from. */
export interface IServeRuleEvent {
  readonly rule: string;
  readonly rows: number;
  readonly wall_ms: number;
}

export interface IServeBindEvent {
  readonly rel: string;
  readonly period: number;
  readonly bucket: number;
}

/** One coalesced watcher window that committed. Counts are ROWS, so a window
 *  whose files were re-saved unchanged never produces one of these at all. */
export interface IServeWatchEvent {
  readonly rel: string;
  readonly glob: string;
  readonly added: number;
  readonly removed: number;
}

/** One JSONL line per tick: the tick's own span plus every effect, bind, and
 *  watcher firing observed since the previous line. */
export interface IServeTickLine extends IServeTickEvent {
  readonly rules: readonly IServeRuleEvent[];
  readonly effects: readonly IServeEffectEvent[];
  readonly binds: readonly IServeBindEvent[];
  readonly watches: readonly IServeWatchEvent[];
}

/** The runtime half of the trace spine. Publishes; never sinks. */
export interface IRuntimeTrace {
  readonly enabled: boolean;
  rule(ruleId: string, rows: number, wallMs: number): void;
}

export interface IServeTrace {
  tick(tick: number, rels: number, rows: number, statements: number, wallMs: number): void;
  effect(
    host: string,
    witnessDigest: string,
    outcome: "done" | "cache_hit" | "error",
    rows: number,
    wallMs: number,
    failure?: unknown,
  ): void;
  bind(rel: string, period: number, bucket: number): void;
  watch(rel: string, glob: string, added: number, removed: number): void;
  /** Idempotent. Installs the one subscriber per channel when DL_PERF_LOG names
   *  a file; a second call with the same env is a no-op. */
  installFromEnv(): void;
}

export interface IProcessMemorySnapshot {
  readonly rssBytes: number;
  readonly heapUsedBytes: number;
  readonly externalBytes: number;
}

export interface ISqliteObjectBytes {
  readonly name: string;
  readonly bytes: number;
}

export interface ISqliteStatsSnapshot {
  readonly pageCount: number;
  readonly pageSize: number;
  readonly freelistCount: number;
  readonly dbBytes: number;
  readonly freelistBytes: number;
  readonly dbstatAvailable: boolean;
  readonly objectBytes: readonly ISqliteObjectBytes[];
}

export interface IServeStatsSnapshot {
  readonly memory: IProcessMemorySnapshot;
  readonly sqlite: ISqliteStatsSnapshot;
}

export interface IServeStats {
  /** PRAGMA page_count/page_size/freelist_count, always; per-table `dbstat`
   *  page bytes for `tableNames` when this connection's SQLite build exposes
   *  the vtab (`dbstatAvailable` states which; `objectBytes` is empty when
   *  false, never a guess). One dbstat statement regardless of table count
   *  (the N+1 law applies to stats reads too). */
  sqliteSnapshot(seam: ISqlSeam, tableNames: readonly string[]): Observable<ISqliteStatsSnapshot>;
  /** `process.memoryUsage()`'s rss/heapUsed/external, read in whatever
   *  process calls this -- the served engine's own process when invoked from
   *  serve/4_http.ts's `GET /stats` handler. */
  processMemory(): IProcessMemorySnapshot;
}

/** Interfaces for standalone exports whose names are fixed by emitted imports. */
/** Boundary add/del rows for one rel: the result of one multiset diff. A plain
 *  data shape, declared here rather than in diff.ts so the header holds it
 *  once (review finding 7 named its absence alongside the eight functions). */
export interface IRowDiff {
  readonly add: readonly IRow[];
  readonly del: readonly IRow[];
}

/** The boundary-diff algorithm every emitted tick calls (`runtime/diff.ts`).
 *  `prevRows`/`nextRows` are the FULL row lists (duplicates allowed) for one
 *  rel, read before and after one tick's writes. */
export interface IMultisetDiff {
  (prevRows: readonly IRow[], nextRows: readonly IRow[]): IRowDiff;
}

/** One SQLite value coming back across the driver seam, converted by the
 *  column's DECLARED type (`runtime/rows.ts`). Bool and float are the two that
 *  refuse rather than coerce. */
export interface IRowValueFromSql {
  (type: IRowColumnType | undefined, value: unknown): IRowValue;
}

/** The read seam: run a SELECT and shape the result into `IRow[]` with columns
 *  in the caller's declared order (`runtime/rows.ts`). */
export interface ISelectRows {
  (
    seam: ISqlSeam,
    sql: string,
    columns: readonly string[],
    columnTypes?: readonly IRowColumnType[],
  ): Observable<IRow[]>;
}

/** Replace the carry-in frontiers used by emitted ordered-occurrence programs
 *  with this tick's boundary-visible additions (`runtime/1_incremental.ts`).
 *  Emits whether anything carries. Sits beside `IIncrementalRuntime` and is not
 *  one of its members for the emitted-import reason stated above. */
export interface IStageOrderedFrontiers {
  (
    seam: ISqlSeam,
    relations: readonly IIncrementalRelationPlan[],
    additions: readonly IRelDelta[],
  ): Observable<boolean>;
}

/** Boot one served program: its DDL (restart-tolerant, "already exists" is the
 *  one tolerated error), the witness table, then its own boot statements
 *  (`serve/3_engine.ts`). */
export interface IBootServedProgram {
  (seam: ISqlSeam, program: IServedProgram): Observable<void>;
}

/** The witness cache's current rows, ordered, for the endurance receipt that
 *  proves a cached witness did not refire (`serve/1_hosts.ts`). */
export interface IWitnessRows {
  (seam: ISqlSeam): Observable<readonly IRow[]>;
}

/** The directory a `watch` glob is rooted at: the glob's literal prefix
 *  resolved against the configured root (`serve/2_binds.ts`). */
export interface IWatchRootOf {
  (root: string, glob: string): string;
}

/** Does this worktree-relative path belong to this glob? The ONE membership
 *  test the `watch` bind has, called by its boot half and its live half alike
 *  (ruling `glob_dialect = node_matcher_both_halves`, `serve/2_binds.ts`).
 *  It is an interface and not a bare helper precisely so the two halves are
 *  bound to one contract: git pathspec used to answer this question at boot and
 *  disagreed with the live answer on 170 of 242 corpus globs. */
export interface IMatchesWatchGlob {
  (relativePath: string, glob: string): boolean;
}

/** The bind plans one executor owns, refusing any plan naming an executor that
 *  does not exist (`serve/2_binds.ts`). */
export interface IBindPlansFor {
  (plans: readonly IBindPlan[], execution: string): readonly IBindPlan[];
}
