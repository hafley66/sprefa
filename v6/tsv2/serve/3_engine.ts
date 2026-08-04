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
import { selectRows } from "../runtime/rows.ts";
import { TickLogEmitter } from "../runtime/ticklog.ts";
import type {
  IArrivalBatch,
  IBootServedProgram,
  ILiveEngine,
  IRow,
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
  readonly tickNumber: number;
  readonly deltas: ITickDeltas | null;
  readonly drainsUsed: number;
};

function hasDeltas(step: FoldStep): step is FoldStep & { deltas: ITickDeltas } {
  return step.deltas !== null;
}

function deltaRowCount(deltas: ITickDeltas): number {
  return deltas.rels.reduce((total, rel) => total + rel.add.length + rel.del.length, 0);
}

export class LiveEngine implements ILiveEngine {
  readonly ticks$: Observable<ITickOutcome>;

  private readonly arrivals = new Subject<QueuedBatch>();
  private tickNumber = 0;
  private queuedBatches = 0;
  private running = false;

  constructor(
    readonly program: IServedProgram,
    private readonly seam: ISqlSeam,
  ) {
    this.ticks$ = this.arrivals.pipe(
      concatMap((queued) => this.runBatch(queued)),
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
      this.queuedBatches += 1;
      this.arrivals.next({ arrivals, subscriber });
    });
  }

  rows(rel: string): Observable<readonly IRow[]> {
    const sql = this.program.finalSelect[rel];
    const columns = this.program.relColumns[rel];
    if (sql === undefined || columns === undefined) {
      return throwError(() => new Error(`unknown rel '${rel}' in program '${this.program.name}'`));
    }
    return selectRows(this.seam, sql, columns, this.program.relColumnTypes?.[rel]);
  }

  /** One queued batch: its own tick, then drain ticks while the program carries
   *  AND nothing else waits behind it. Emits every tick it caused, to the
   *  submitter and (through `ticks$`) to the app. */
  private runBatch(queued: QueuedBatch): Observable<ITickOutcome> {
    const boot: FoldStep = { tickNumber: this.tickNumber, deltas: null, drainsUsed: 0 };
    this.queuedBatches -= 1;
    return of(boot).pipe(
      expand((step) => {
        if (step.deltas === null) return this.tickOnce(step, queued.arrivals);
        if (!step.deltas.carryPending || this.queuedBatches > 0) return EMPTY;
        if (step.drainsUsed >= DRAIN_CAP) {
          throw new Error(`tsv2 drain overflow: ${this.program.name} exceeded ${DRAIN_CAP} drain ticks`);
        }
        return this.tickOnce(step, []);
      }),
      filter(hasDeltas),
      tap((step) => {
        this.tickNumber = step.tickNumber;
      }),
      map((step): ITickOutcome => {
        const outcome = {
          tick: step.tickNumber,
          // The served log and the fixture-replay log are the SAME contract, so
          // this passes the program's column types exactly as tickLoop.ts does.
          // Without them a `json` or `ref` column prints as a JSON string here
          // and as a JSON value there, and the served leg's whole reason to
          // exist is that the two agree byte for byte.
          line: TickLogEmitter.line(step.tickNumber, step.deltas, this.program.relColumnTypes),
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
      // running", and because `runProgram$` merges `ticks$` into the app graph
      // the error reached serveTsv2's single subscriber and killed the process
      // (measured: the whole server went ECONNREFUSED). Receipt:
      // tests/engineFault.test.ts, red-first output in its header.
      catchError((failure: unknown) => {
        queued.subscriber.error(failure);
        return EMPTY;
      }),
    );
  }

  private tickOnce(step: FoldStep, arrivals: IArrivalBatch): Observable<FoldStep> {
    const startedAt = performance.now();
    const statementsBefore = stmt_counter.get();
    return this.program.tick(this.seam, arrivals).pipe(
      map((deltas): FoldStep => {
        const tickNumber = step.tickNumber + 1;
        ServeTrace.tick(
          tickNumber,
          deltas.rels.length,
          deltaRowCount(deltas),
          stmt_counter.get() - statementsBefore,
          performance.now() - startedAt,
        );
        return {
          tickNumber,
          deltas,
          drainsUsed: step.deltas === null ? 0 : step.drainsUsed + 1,
        };
      }),
    );
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// Boot: schema (restart-tolerant), the witness table, then the program's own
// boot statements.
// ─────────────────────────────────────────────────────────────────────────────

function isAlreadyExists(failure: unknown): boolean {
  return failure instanceof Error && /already exists/i.test(failure.message);
}

export const bootServedProgram: IBootServedProgram = (
  seam: ISqlSeam,
  program: IServedProgram,
): Observable<void> => {
  const statements = [...program.ddl, ...WitnessCache.ddl()];
  return from(statements).pipe(
    concatMap((sql) =>
      seam.runner.execute(seam.db, sql).pipe(
        catchError((failure: unknown) => (isAlreadyExists(failure) ? of(undefined) : throwError(() => failure))),
      ),
    ),
    toArray(),
    concatMap(() => BootRunner.run(seam, program.boot)),
  );
};
