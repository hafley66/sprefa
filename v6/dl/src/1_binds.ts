/**
 * BindDef registry for input-side world sources.
 * HostRunner. Where a HostDef answers OUTBOUND demand (`host?(args)` rows drive a
 * subprocess, whose response rows land back on `__resp_h`), a BindDef feeds
 * INBOUND arrival rows into an ordinary EDB rel with no demand row at all -- the
 * world pushes on its own schedule, the program just reads the latest state
 * (clock_residency = world_fed_bind_not_construct, rulings.pl: "cadence enters as
 * ordinary arrival rows; SWR policy is program rules over latest state, NEVER
 * runtime machinery").
 *
 * A bind activates by relation name. The kernel learns the word
 * "bind", never the word "clock" -- zero grammar changes this arc): a BindDef
 * activates for a loaded program iff the program declares an EDB rel whose name
 * matches `bind.rel` (BindRunner's constructor, below, checks this once per
 * program load against `program.rels`, never against a hardcoded name). An
 * inactive bind's `source$` is never subscribed, so it costs nothing (no timer,
 * no query) for a program that never mentions its rel.
 *
 * `BindRunner.commits$` is cold and is composed into the program graph.
 * app graph HostRunner's `effects$` lives in (runProgram$'s merge, 6_http.ts) --
 * one subscription per program load, torn down by the same switchMap unsubscribe
 * a program swap already triggers on the running program's whole branch. An
 * rxjs `interval` observable's unsubscribe IS the underlying `clearInterval`
 * call (rxjs's AsyncAction schedules `setInterval` and clears it on teardown) --
 * no bespoke timer bookkeeping, no dispose() method, no held Subscription/handle
 * field, matching IHostRunner's own no-bespoke-lifecycle shape exactly.
 *
 * This file imports only
 * 0_types.ts + 0_trace.ts + rxjs + sprefa-store-engine's ast types + node stdlib,
 * never 2_schema.ts or 3_runtime.ts (both numbered above it). BindRunner's
 * constructor therefore takes the runtime as a STRUCTURAL parameter typed by
 * 0_types.ts's `IDlRuntime` interface, never the concrete `DlRuntime` class.
 *
 * The bind and effect seams differ in two ways:
 *   - HostRunner reads its config (which requests are outstanding) from a LIVE
 *     watch of deltas$ merged with a one-time boot replay, so a request posted
 *     after boot is picked up immediately. The clock bind reads its config
 *     (`clock_period` rows) ONCE, at subscribe time, with no live watch -- a
 *     `clock_period` row inserted into an already-running program spins up no
 *     new interval until the next program (re)load. Closing this gap is a live
 *     bind-decl case, not a language change; not attempted this arc.
 *   - HostRunner's `?` probe fires once per WITNESS, deduped through the
 *     effect_cache digest table. A bind fires on WALL-CLOCK CADENCE, not in
 *     response to any row appearing, so there is no witness to dedupe against
 *     and no cache table -- "did this already run" is not a question a bind ever
 *     asks. This is a real asymmetry between the two seams, not a missing
 *     feature on the input side.
 */

import {
  EMPTY,
  type Observable,
  type SchedulerLike,
  asyncScheduler,
  defer,
  from,
  interval,
  map,
  merge,
  mergeMap,
  of,
  catchError,
} from "rxjs";

import type { Program } from "sprefa-store-engine/src/lower/ast.ts";

import { PerfTrace } from "./0_trace.ts";
import type {
  AssertTrue,
  BindCommitDone,
  BindConfig,
  BindDef,
  IBindRunner,
  IBindRunnerStatics,
  IDlRuntime,
  Row,
} from "./0_types.ts";

export type { BindCommitDone, BindConfig, BindDef };

// ─────────────────────────────────────────────────────────────────────────────
// clockBind: the first BindDef. Feeds `clock_period(period_secs)` rows ->
// `clock_bucket(period, bucket)` rows, one interval timer per distinct declared
// period.
// ─────────────────────────────────────────────────────────────────────────────

/** Bucket law: `floor(epoch_secs / periodSecs)`, NOT a process-local counter. A
 *  process-local counter restarts at 0 on every reboot, which would replay
 *  bucket 0/1/2/... after a crash even though wall-clock time (and therefore
 *  "how many periods have elapsed") kept moving regardless of the process's own
 *  lifetime. Deriving the bucket from epoch time is stable across restarts: two
 *  processes (or the same process before/after a kill -9) computing the bucket
 *  at the same wall-clock instant agree, with no persisted counter anywhere. */
function bucketFor(periodSecs: number): number {
  return Math.floor(Date.now() / 1000 / periodSecs);
}

/** One rows() read (N+1 law), folded to the distinct declared period values. */
function distinctPeriods(periodRows: readonly Row[]): readonly number[] {
  const seen = new Set<number>();
  for (const row of periodRows) {
    const periodSecs = Number(row.period_secs);
    if (Number.isFinite(periodSecs) && periodSecs > 0) seen.add(periodSecs);
  }
  return [...seen];
}

/** One period's ticking clock: an `interval` firing every `periodSecs` seconds,
 *  each firing emitting exactly one `clock_bucket` row for that period. `interval`
 *  (not `timer`) so every period's first row lands one period after activation,
 *  the same as every later one -- no special-cased "fire immediately" branch.
 *  `scheduler` is rxjs's own `SchedulerLike` (0_types.ts's `BindConfig` seam) --
 *  this is the ONE place wall-clock cadence enters the process; a test passes a
 *  `TestScheduler` here to run the cadence on virtual time. `bucketFor` below
 *  still reads real `Date.now()` regardless of which scheduler drives the firing
 *  (this file's header + 0_types.ts's Binds section: bucket VALUES stay real
 *  wall-clock on purpose, only the firing CADENCE is injected). */
function clockIntervalFor(periodSecs: number, scheduler: SchedulerLike): Observable<readonly Row[]> {
  return interval(periodSecs * 1000, scheduler).pipe(
    map((): readonly Row[] => [{ period: periodSecs, bucket: bucketFor(periodSecs) }]),
  );
}

export const clockBind: BindDef = {
  rel: "clock_bucket",
  columns: ["period", "bucket"],
  source$(config: BindConfig): Observable<readonly Row[]> {
    // Boot-scan, not a live watch (see this file's header for the honest gap vs
    // HostRunner). `clock_period` may be a rel the loaded program never declares
    // at all (a program can activate the clock bind by declaring `clock_bucket`
    // without also declaring `clock_period` -- a config typo, not a crash): an
    // "unknown rel" read is treated as zero configured periods, matching
    // 1_hosts.ts's replayableRequests tolerance for the same error shape.
    return defer(() => from(config.runtime.rows("clock_period"))).pipe(
      catchError((failure: unknown) => {
        if (failure instanceof Error && failure.message.includes("unknown rel")) return of([] as Row[]);
        throw failure;
      }),
      mergeMap((periodRows) => {
        const periods = distinctPeriods(periodRows);
        return periods.length > 0 ? merge(...periods.map((periodSecs) => clockIntervalFor(periodSecs, config.scheduler))) : EMPTY;
      }),
    );
  },
};

/** Every bind shipped in-box. No grammar surface exists this arc for a program
 *  to declare its OWN bind (the same "in-box, no user-authored decl" shape
 *  builtinSg/builtinExtract have on the host side) -- a program activates
 *  clockBind purely by declaring an EDB rel named `clock_bucket`. */
export const builtinBinds: readonly BindDef[] = [clockBind];

// ─────────────────────────────────────────────────────────────────────────────
// BindRunner: activates the registered BindDefs whose rel the loaded program
// declares (EDB only -- a program that heads the same name with a derived rule
// instead is the same one-rel-one-rule-kind hazard 3_runtime.ts already guards
// generically, not something this file special-cases), merges their `source$`
// firings, and commits each firing through the ordinary EdbBatch insert path --
// zero retract: a rel needing "latest wins" declares its OWN `rel(1)` retention
// marker, the same knob any EDB write already respects (3_runtime.ts's
// applyRelWrite keepNewestOnly sweep), rather than the bind tracking prior state.
// ─────────────────────────────────────────────────────────────────────────────

export class BindRunner implements IBindRunner {
  /** Cold: nothing runs until the program's cold graph region (or a test
   *  fixture) subscribes, and unsubscribing is the whole of teardown -- no
   *  Subscription field, no dispose(), the same contract IHostRunner's
   *  effects$ makes. One value per finished bind commit. */
  readonly commits$: Observable<BindCommitDone>;

  constructor(
    private readonly runtime: IDlRuntime,
    binds: readonly BindDef[],
    program: Program,
    private readonly scheduler: SchedulerLike = asyncScheduler,
  ) {
    const edbRelNames = new Set(
      program.rels.filter((decl) => decl.origin === "EDB").map((decl) => decl.name),
    );
    const activeBinds = binds.filter((bind) => edbRelNames.has(bind.rel));

    this.commits$ =
      activeBinds.length > 0
        ? merge(
            ...activeBinds.map((bind) =>
              bind
                .source$({ runtime: this.runtime, scheduler: this.scheduler })
                .pipe(mergeMap((rows) => from(this.commitOnce(bind, rows)))),
            ),
          )
        : EMPTY;
  }

  /** One firing -> one commit. `insert`-only (see file header): a bind never
   *  computes its own retract, so this only ever calls commit with an empty
   *  retract map. */
  private async commitOnce(bind: BindDef, rows: readonly Row[]): Promise<BindCommitDone> {
    const startedAt = performance.now();
    const report = await this.runtime.commit({
      insert: new Map([[bind.rel, rows]]),
      retract: new Map(),
    });
    // Self-diagnosis (0_trace.ts sprefa:bind channel): one event per firing,
    // tagged with the firing's own commit tick -- there is no earlier "demand
    // tick" for a bind the way there is for an effect (HostRunner.runEffectOnce
    // tags cache_hit/error with the DEMAND tick because those two never reach a
    // commit); every bind firing reaches a commit, so this tag is unambiguous.
    PerfTrace.bindDone(report.tick, bind.rel, rows.length, performance.now() - startedAt);
    return { rel: bind.rel, rows: rows.length };
  }
}

// ---- contract proofs (src/0_types.ts) ----------------------------------------
export type BindRunnerStaticsHold = AssertTrue<typeof BindRunner extends IBindRunnerStatics ? true : false>;
