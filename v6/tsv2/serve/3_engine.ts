/**
 * Serialized live tick loop for one compiled program and SQLite seam.
 * Serving receives batches from HTTP posts, bind timers, and host responses.
 *
 * The schedule-fed fold and drain rule are reused so served tick logs remain
 * byte-for-byte compatible with the oracle.
 *
 * `TickFold` drains (ticks with an empty
 * batch while `carryPending` holds) only AFTER the schedule is exhausted -- a
 * carrying tick that still has a scheduled batch waiting takes the batch, not a
 * drain tick. The live loop reproduces that with `queuedBatches`: a batch
 * drains only when nothing else is already queued behind it. Without that
 * counter a served run would drain between every pair of posts and print a
 * differently-numbered log than the oracle.
 *
 * Host responses re-enter the cyclic dataflow through `arrivals`, the merge
 * point that can be pushed into. A submitter's reply travels with
 * point. A submitter's reply travels with
 * its own request (the queued item carries its subscriber), so nothing pushes
 * into one Subject and awaits a matching id on another, and no caller is forced
 * back into `await`.
 *
 * The loop turns from the first `ticks$` subscription onward, INCLUDING while
 * nothing reads it: readers come and go, the engine does not. `submit` before
 * anything has ever subscribed is an error, because the arrivals Subject would
 * drop the batch with no lane behind it.
 *
 * File-backed boot tolerates existing permanent tables and recreates TEMP
 * tables on each boot.
 */

import {
  EMPTY,
  Observable,
  Subject,
  Subscriber,
  catchError,
  concatMap,
  expand,
  filter,
  finalize,
  from,
  map,
  of,
  share,
  tap,
  throwError,
  toArray,
} from "rxjs";

import { stmt_counter } from "sprefa-store-engine/src/engine/counter.ts";

import { BootRunner } from "../runtime/2_boot.ts";
import { EnumPlane } from "../runtime/enumPlane.ts";
import { select_rows } from "../runtime/rows.ts";
import { TickLogEmitter } from "../runtime/ticklog.ts";
import type {
  IArrivalBatch,
  IBootServedProgram,
  ILiveEngine,
  IReloadPlan,
  IRow,
  IRowColumnType,
  IServedProgram,
  ISqlSeam,
  ITickDeltas,
  ITickOutcome,
} from "../runtime/types.ts";
import { ServeTrace } from "./0_trace.ts";
import { WitnessCache } from "./1_hosts.ts";

const DRAIN_CAP = 100;

/** One enqueued batch and the subscriber owed its ticks. */
type QueuedBatch = {
  readonly arrivals: IArrivalBatch;
  readonly subscriber: Subscriber<ITickOutcome>;
};

/** Private fold state, mirroring tickLoop.ts's own `FoldStep`. */
type FoldStep = {
  readonly tick_number: number;
  readonly deltas: ITickDeltas | null;
  readonly drains_used: number;
};

function has_deltas(step: FoldStep): step is FoldStep & { deltas: ITickDeltas } {
  return step.deltas !== null;
}

function delta_row_count(deltas: ITickDeltas): number {
  return deltas.rels.reduce((total, rel) => total + rel.add.length + rel.del.length, 0);
}

export class LiveEngine implements ILiveEngine {
  readonly ticks$: Observable<ITickOutcome>;

  private readonly arrivals = new Subject<QueuedBatch>();
  private tick_number = 0;
  private queued_batches = 0;
  private running = false;

  constructor(
    readonly program: IServedProgram,
    private readonly seam: ISqlSeam,
  ) {
    this.ticks$ = this.arrivals.pipe(
      concatMap((queued) => this.run_batch(queued)),
      tap({
        subscribe: () => {
          this.running = true;
        },
        finalize: () => {
          this.running = false;
        },
      }),
      // `resetOnRefCountZero: false` KEEPS THE LANE CONNECTED once anything has
      // read `ticks$`, so `running` follows the lane and not the reader count.
      // Under the default reset the last reader leaving tore the concatMap lane
      // down, `finalize` flipped `running` false, and the next submit was
      // refused (receipt: tests/engineBetweenSubscribers.test.ts). The lane still
      // resets on complete or error, which is what the fault arm relies on.
      share({ resetOnRefCountZero: false }),
    );
  }

  submit(arrivals: IArrivalBatch): Observable<ITickOutcome> {
    return new Observable<ITickOutcome>((subscriber) => {
      if (!this.running) {
        subscriber.error(new Error("tsv2 engine is not running: nothing subscribes ticks$"));
        return;
      }
      this.queued_batches += 1;
      this.arrivals.next({ arrivals, subscriber });
    });
  }

  rows(rel: string): Observable<readonly IRow[]> {
    const sql = this.program.final_select[rel];
    const columns = this.program.rel_columns[rel];
    if (sql === undefined || columns === undefined) {
      return throwError(() => new Error(`unknown rel '${rel}' in program '${this.program.name}'`));
    }
    return select_rows(this.seam, sql, columns, this.program.rel_column_types?.[rel]).pipe(
      concatMap((rows) => EnumPlane.decode_rows(this.seam, this.program.enum_types ?? [], this.program.enum_ref_columns ?? {}, rel, rows)),
    );
  }

  /** One queued batch: its own tick, then drain ticks while the program carries
   *  AND nothing else waits behind it. Emits every tick it caused, to the
   *  submitter and (through `ticks$`) to the app. */
  private run_batch(queued: QueuedBatch): Observable<ITickOutcome> {
    const boot: FoldStep = { tick_number: this.tick_number, deltas: null, drains_used: 0 };
    this.queued_batches -= 1;
    return of(boot).pipe(
      expand((step) => {
        if (step.deltas === null) return this.tick_once(step, queued.arrivals);
        if (!step.deltas.carry_pending || this.queued_batches > 0) return EMPTY;
        if (step.drains_used >= DRAIN_CAP) {
          throw new Error(`tsv2 drain overflow: ${this.program.name} exceeded ${DRAIN_CAP} drain ticks`);
        }
        return this.tick_once(step, []);
      }),
      filter(has_deltas),
      tap((step) => {
        this.tick_number = step.tick_number;
      }),
      map((step): ITickOutcome => {
        const outcome = {
          tick: step.tick_number,
          // The served log and the fixture-replay log are the SAME contract, so
          // this passes the program's column types exactly as tickLoop.ts does.
          // Without them a `json` or `ref` column prints as a JSON string here
          // and as a JSON value there, and the served leg's whole reason to
          // exist is that the two agree byte for byte.
          line: TickLogEmitter.line(step.tick_number, step.deltas, boundary_types(this.program)),
          deltas: step.deltas,
        };
        queued.subscriber.next(outcome);
        return outcome;
      }),
      finalize(() => queued.subscriber.complete()),
      // A TICK FAULT IS THE SUBMITTER'S, NOT THE LANE'S. `queued.subscriber` is
      // this batch's own reply channel, so the failure reaches whoever caused it
      // (over http that is a 500 naming the fault, serve/4_http.ts routeRequest$).
      // The lane then absorbs it with EMPTY and keeps turning. It used to
      // re-throw here, and an error inside `concatMap` terminates the OUTER
      // observable: `ticks$` died on the first bad tick, `tap({ finalize })`
      // flipped `running` false so every later submit failed with "engine is not
      // running", and because `run_program$` merges `ticks$` into the app graph
      // the error reached serve_tsv2's single subscriber and killed the process
      // (measured: the whole server went ECONNREFUSED). Receipt:
      // tests/engineFault.test.ts, red-first output in its header.
      catchError((failure: unknown) => {
        queued.subscriber.error(failure);
        return EMPTY;
      }),
    );
  }

  private tick_once(step: FoldStep, arrivals: IArrivalBatch): Observable<FoldStep> {
    const started_at = performance.now();
    const statements_before = stmt_counter.get();
    return this.program.tick(this.seam, arrivals).pipe(
      map((deltas): FoldStep => {
        const tick_number = step.tick_number + 1;
        ServeTrace.tick(
          tick_number,
          deltas.rels.length,
          delta_row_count(deltas),
          stmt_counter.get() - statements_before,
          performance.now() - started_at,
        );
        return {
          tick_number,
          deltas,
          drains_used: step.deltas === null ? 0 : step.drains_used + 1,
        };
      }),
    );
  }
}

function boundary_types(program: IServedProgram): Readonly<Record<string, readonly IRowColumnType[]>> {
  const types: Record<string, readonly IRowColumnType[]> = { ...program.rel_column_types };
  for (const [rel, refs] of Object.entries(program.enum_ref_columns ?? {})) {
    const rel_types = [...(types[rel] ?? [])];
    refs.forEach((reference, index) => { if (reference !== null) rel_types[index] = "json"; });
    types[rel] = rel_types;
  }
  return types;
}

// ─────────────────────────────────────────────────────────────────────────────
// Boot: schema (restart-tolerant), the witness table, then the program's own
// boot statements.
// ─────────────────────────────────────────────────────────────────────────────

const QUOTED_IDENTIFIER = /"([^"]+)"/;

const CREATED_OBJECT = /^CREATE\s+(?:TEMP\s+)?(?:TABLE|VIEW|INDEX)\s+(?:IF\s+NOT\s+EXISTS\s+)?"([^"]+)"/i;

/** The planner writes every statement with its target table as the first quoted
 *  identifier (serve/reloadPlan.ts); nothing else on this path parses SQL. */
function plan_target(sql: string): string | null {
  return QUOTED_IDENTIFIER.exec(sql)?.[1] ?? null;
}

/** A swap against `:memory:` opens an EMPTY database, and `DELETE FROM` an
 *  absent table throws where the plan meant a no-op. */
function reload_discards(reload: IReloadPlan | undefined, table_names: readonly string[]): readonly string[] {
  if (reload === undefined) return [];
  const present = new Set(table_names);
  return reload.statements.filter((sql) => {
    const target = plan_target(sql);
    return target !== null && present.has(target);
  });
}

function dropped_tables(discards: readonly string[]): ReadonlySet<string> {
  const dropped = new Set<string>();
  for (const sql of discards) {
    if (!/^DROP\s+TABLE/i.test(sql)) continue;
    const target = plan_target(sql);
    if (target !== null) dropped.add(target);
  }
  return dropped;
}

/** A CREATE for a table the plan JUST DROPPED cannot legitimately already
 *  exist; swallowing that leaves the new rule writing into the old shape. */
function tolerates_existing(failure: unknown, sql: string, dropped: ReadonlySet<string>): boolean {
  if (!(failure instanceof Error) || !/already exists/i.test(failure.message)) return false;
  const created = CREATED_OBJECT.exec(sql)?.[1];
  return created === undefined || !dropped.has(created);
}

const DICTIONARY_TABLE = "__str";

const USER_TABLES_SQL =
  `SELECT "name" FROM "sqlite_master" WHERE "type" = 'table' AND "name" NOT LIKE 'sqlite_%'`;

/** Whether this module's own DDL builds the string dictionary. A dict-mode
 *  program with no text column builds none and reads either database. */
function program_builds_dictionary(program: IServedProgram): boolean {
  return program.ddl.some((sql) => sql.startsWith(`CREATE TABLE "${DICTIONARY_TABLE}" (`));
}

/** Refused before any DDL runs: affinity would store strings into id columns
 *  silently, and the corruption is recoverable only by rebuilding. */
function intern_crossing_failure(program: IServedProgram, tableNames: readonly string[]): Error | null {
  if (tableNames.length === 0) return null;
  const databaseIsInterned = tableNames.includes(DICTIONARY_TABLE);
  if (databaseIsInterned === program_builds_dictionary(program)) return null;
  const databaseMode = databaseIsInterned ? "dict" : "direct";
  return new Error(
    `intern mode crossing, and honey, no. 💅 WHAT: module "${program.name}" strutted in wearing ` +
      `intern(${program.internMode}) but this database is serving intern(${databaseMode}) — one of ` +
      `them speaks integer ids, the other speaks raw strings, and that situationship ends in NULL ` +
      `every single time. WHY: the dictionary is baked into every table shape at compile time; a ` +
      `runtime toggle cannot un-declare a column type. HOW TO FIX: recompile the module at ` +
      `intern(${databaseMode}) to match this database, or rebuild the database from source at ` +
      `intern(${program.internMode}) to match the module. Pick a lane, gorgeous — both are ` +
      `fabulous, they are just not each other's.`,
  );
}

export const boot_served_program: IBootServedProgram = (
  seam: ISqlSeam,
  program: IServedProgram,
  reload?: IReloadPlan,
): Observable<void> => {
  const statements = [...program.ddl, ...WitnessCache.ddl()];
  return select_rows(seam, USER_TABLES_SQL, ["name"], ["text"]).pipe(
    map((rows) => rows.map((row) => String(row[0]))),
    concatMap((tableNames) => {
      const failure = intern_crossing_failure(program, tableNames);
      return failure === null ? of(reload_discards(reload, tableNames)) : throwError(() => failure);
    }),
    concatMap((discards) =>
      from(discards).pipe(
        concatMap((sql) => seam.runner.execute(seam.db, sql)),
        toArray(),
        map(() => dropped_tables(discards)),
      ),
    ),
    concatMap((dropped) =>
      from(statements).pipe(
        concatMap((sql) =>
          seam.runner.execute(seam.db, sql).pipe(
            catchError((failure: unknown) =>
              tolerates_existing(failure, sql, dropped) ? of(undefined) : throwError(() => failure),
            ),
          ),
        ),
        toArray(),
      ),
    ),
    concatMap(() => BootRunner.run(seam, program.boot)),
  );
};
