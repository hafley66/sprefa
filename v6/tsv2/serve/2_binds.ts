/**
 * 2_binds.ts — LIVE world PUSH sources: `bind interval(...)` (runtime-bridge
 * arc) and `bind watch(...)` (golden plan phase 2, extraction live). Both are
 * the same shape -- a cold observable of world events, each window committing
 * one arrival batch -- so they share a file.
 *
 *   interval(periodMs, scheduler) -> map(toBucketRow) -> mergeMap(submit)
 *   watchSource(root) -> bufferTime(coalesceMs) -> map(diffAgainstLast) -> submit
 *
 * WHICH INSTANCES. The bind DECLARATION authorizes a world source; the
 * program's own RULES say which instances it consumes, as literals in the bind
 * atom's first column (registry.pl calls it the configuration column):
 * `interval(300, Bucket)`, `watch("src/**\/*.ts", Path, Digest)`. emit_ts.pl
 * collects those into the emitted plan's `literals`, so this file starts
 * exactly what the program asked for and no default. A program that declares a
 * bind and reads no literal gets `literals: []` and therefore no live source at
 * all -- an honest zero, never an invented cadence or an invented glob.
 *
 * This closes v6/dl's own recorded gap in passing: `1_binds.ts` reads its
 * `clock_period` config from a rel ONCE at subscribe time, so a period added
 * mid-run spins nothing until reload. Here the cadence is a COMPILE-TIME fact
 * of the program text, so "read once per program load" is not a gap, it is the
 * whole truth: a new period is a new program.
 *
 * BUCKET LAW, unchanged from the shipped clock bind: floor(epoch_secs/period),
 * never a process-local counter, so a restart does not replay bucket 0,1,2...
 * The scheduler seam moves the FIRING; the bucket VALUE stays real wall clock
 * (a TestScheduler run therefore produces real-clock bucket values, which is
 * why the receipt asserts firing COUNT and column shape, not bucket numbers).
 *
 * TEARDOWN is unsubscription and nothing else: an rxjs `interval`'s unsubscribe
 * IS the underlying clearInterval, and the watch source's unsubscribe aborts
 * the `fsPromises.watch` iterator. A program swap's `switchMap` therefore stops
 * every world source with no Subscription field and no dispose() here.
 *
 * ── THE WATCH BIND ──────────────────────────────────────────────────────────
 *
 * WHAT CROSSES THE SEAM (slot SLOT-P2-WATCHER-EVENT-SHAPE, decided here): NOT
 * the backend's event vocabulary. node's `fs.watch` says "rename"/"change",
 * @parcel/watcher says "create"/"update"/"delete", and neither word survives
 * `NodeWatchSource` -- what leaves it is a bare PATH, and what leaves this
 * runner is one `(glob, path, digest)` row carrying an arrival SIGN. Three
 * consequences, all of them the reason for the choice:
 *
 *   - a RENAME is not a third case. The old path stops existing (`-` its row)
 *     and the new one starts (`+` its row); both land in the same batch, so a
 *     rename is one tick, and an editor's write-temp-then-rename atomic save is
 *     just "the real path's digest changed" -- exactly the case chokidar needs
 *     its `atomic: true` option to reconstruct.
 *   - a save that did not change CONTENT emits nothing. The digest is the row,
 *     so `distinctUntilChanged` at the rel boundary is free and nothing
 *     downstream re-derives (ruling salt_minting = content_addressed).
 *   - there is no null and no "kind" column: presence rides the delta sign,
 *     which is the language's own lifecycle decomposition (TICK-MODEL.md §3).
 *
 * BOOT RECONCILE, the sanctioned resolution of extraction ambiguity A12
 * (plans/2026-07-29-watch-boot-reconcile-brief.md): once per watched glob at
 * subscribe, read the engine's durable watch rows and compare them with the
 * tracked worktree set from `git ls-files`. Their difference is one arrival
 * batch, and the reconciled rows seed `lastDigest`. This is the named one-shot
 * crossing of A12's push/demand line; after boot, the live path remains bare
 * paths from the watch source, sign from digest comparison, and `bufferTime`
 * coalescing.
 *
 * COALESCE WINDOW = 100ms by default (`IServeConfig.watchCoalesceMs`), on the
 * injected scheduler so a test drives it on virtual time. `bufferTime` and not
 * `debounceTime`: a debounce never emits while a large `git checkout` keeps
 * firing, while a fixed window is bounded on BOTH sides -- a 2-second checkout
 * is ~20 ticks rather than one per file, and no burst can starve the engine.
 */

import { spawnSync } from "node:child_process";
import { readFileSync, statSync } from "node:fs";
import * as path from "node:path";
import { watch as watchDirectory } from "node:fs/promises";

import { sha256 } from "@noble/hashes/sha2.js";
import { bytesToHex } from "@noble/hashes/utils.js";
import {
  EMPTY,
  Observable,
  type SchedulerLike,
  asyncScheduler,
  bufferTime,
  catchError,
  concatMap,
  defer,
  filter,
  finalize,
  from,
  interval,
  map,
  merge,
  mergeMap,
  scan,
  take,
  throwError,
} from "rxjs";

import type {
  IArrivalBatch,
  IArrivalRow,
  IBindFired,
  IBindPlan,
  IIntervalBindRunner,
  ILiveEngine,
  IRowValue,
  IRow,
  IWatchBindRunner,
  IWatchFired,
  IWatchSource,
} from "../runtime/types.ts";
import { ServeTrace } from "./0_trace.ts";

function bucketFor(periodSecs: number): number {
  return Math.floor(Date.now() / 1000 / periodSecs);
}

/** The `interval` bind's row is (period, bucket) by registry definition
 *  (registry.pl `bind_definition/2`), so the row is built positionally from the
 *  plan's own declared columns: first column carries the period, second the
 *  bucket. A future bind with another column shape needs its own row builder
 *  and would be refused by name below rather than filled by guess. */
function bucketRow(plan: IBindPlan, periodSecs: number): readonly IRowValue[] {
  return plan.columns.map((_column, index) => (index === 0 ? periodSecs : bucketFor(periodSecs)));
}

/** `interval`'s configuration column is `period: int`, so a non-integer literal
 *  there is a program the compiler's own column typing should already have
 *  rejected. Named rather than filtered: a silently dropped cadence is a timer
 *  that never fires and a program that looks like it is running. */
function intervalPeriods(plan: IBindPlan): readonly number[] {
  return plan.literals.map((literal) => {
    if (typeof literal !== "number" || !Number.isInteger(literal)) {
      throw new Error(`bind '${plan.name}' read a non-integer period literal ${JSON.stringify(literal)}`);
    }
    return literal;
  });
}

export class IntervalBindRunner implements IIntervalBindRunner {
  readonly firings$: Observable<IBindFired>;

  constructor(
    engine: ILiveEngine,
    plans: readonly IBindPlan[],
    scheduler: SchedulerLike = asyncScheduler,
  ) {
    const timers = plans.flatMap((plan) => {
      if (plan.execution !== "live_interval") {
        throw new Error(`unknown bind executor '${plan.execution}' for bind '${plan.name}'`);
      }
      if (plan.columns.length !== 2) {
        throw new Error(`bind '${plan.name}' has ${plan.columns.length} columns; the interval row shape is (period, bucket)`);
      }
      return intervalPeriods(plan).map((periodSecs) =>
        interval(periodSecs * 1000, scheduler).pipe(
          map((): IArrivalRow => ({ rel: plan.name, sign: "add", row: bucketRow(plan, periodSecs) })),
          mergeMap((arrival) =>
            // One firing is one reported value: `take(1)` keeps the SETTLE tick
            // and lets any drain ticks the batch causes flow out on `ticks$`
            // like every other tick. The batch itself is already enqueued in
            // the engine, so completing early cannot cancel it.
            engine.submit([arrival]).pipe(
              take(1),
              map((outcome): IBindFired => {
                const bucket = Number(arrival.row[1] ?? 0);
                ServeTrace.bind(plan.name, periodSecs, bucket);
                return { rel: plan.name, period: periodSecs, bucket, tick: outcome.tick };
              }),
            ),
          ),
        ),
      );
    });
    this.firings$ = timers.length > 0 ? merge(...timers) : EMPTY;
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// The watch bind.
// ─────────────────────────────────────────────────────────────────────────────

/** Everything up to the glob's first magic character, as a directory. That is
 *  the only subtree the watcher has to open: `v6/tsv2/**\/*.ts` watches
 *  `v6/tsv2`, not the repo. A glob with magic in its first segment watches the
 *  root, which is honest and loud in the trace rather than a silent narrowing. */
export function watchRootOf(root: string, glob: string): string {
  const magic = glob.search(/[*?[\]{}]/);
  const literalPrefix = magic < 0 ? glob : glob.slice(0, magic);
  const lastSlash = literalPrefix.lastIndexOf("/");
  const relativeDirectory = lastSlash < 0 ? "" : literalPrefix.slice(0, lastSlash);
  return path.resolve(root, relativeDirectory);
}

/**
 * node's own recursive watch, and the ONLY place a watcher library is named.
 *
 * BUY NOTE (plans/2026-07-29-watcher-buy-research.md, coordinator's
 * watcher_first_impl fork): the research verdict ranks @parcel/watcher first on
 * native event batching and chokidar second on ergonomics; this ships on node's
 * built-in `fsPromises.watch` because it is the zero-dependency member of the
 * same set (it IS chokidar v4/v5's own macOS/Windows backend) and because
 * nothing above `IWatchSource` can tell the difference. The upgrade is this
 * class, replaced.
 *
 * `from(asyncIterable)` is rxjs's own conversion, so no `await` appears here;
 * the AbortController is the teardown, which is what makes a program swap's
 * `switchMap` actually close the OS watch.
 */
export class NodeWatchSource implements IWatchSource {
  watch(root: string): Observable<string> {
    return defer(() => {
      const controller = new AbortController();
      return from(watchDirectory(root, { recursive: true, signal: controller.signal })).pipe(
        map((event) => path.resolve(root, event.filename ?? "")),
        // The abort we ourselves just performed comes back out of the iterator
        // as an error; it is this stream ENDING, not a fault to propagate.
        catchError((failure: unknown) =>
          failure instanceof Error && failure.name === "AbortError" ? EMPTY : throwError(() => failure),
        ),
        finalize(() => controller.abort()),
      );
    });
  }
}

/** Content digest of one path, or null when it is not a readable file. Sync on
 *  purpose ("sync stays sync"): this runs inside a `map` over one bounded
 *  coalesce window, so there is no Promise to lift and nothing to interleave. */
function digestOf(absolutePath: string): string | null {
  try {
    if (!statSync(absolutePath).isFile()) return null;
    return bytesToHex(sha256(readFileSync(absolutePath)));
  } catch {
    return null;
  }
}

/** The enumerate host's standing file-set decision, used here at boot without
 *  introducing another walker or ignore vocabulary: tracked worktree paths
 *  selected by the glob as a git pathspec. NUL framing preserves every path
 *  git permits. A non-repository root has the empty tracked set, which keeps
 *  injected filesystem tests independent of git while retaining the same
 *  tracked-only contract. */
function trackedPaths(root: string, glob: string): readonly string[] {
  const result = spawnSync("git", ["ls-files", "-z", "--", glob], {
    cwd: root,
    encoding: "utf8",
  });
  if (result.error) throw result.error;
  if (result.status !== 0) return [];
  return result.stdout.split("\0").filter((entry) => entry.length > 0);
}

/** One watched glob's own bookkeeping: the digest this runner last PUBLISHED
 *  per path, which is what makes a retraction possible (the `-` arrival needs
 *  the exact row that is there) and what makes an unchanged save free. */
class GlobWatch {
  private readonly lastDigest = new Map<string, string>();

  constructor(
    readonly glob: string,
    private readonly rel: string,
    private readonly root: string,
  ) {}

  /** Durable engine rows versus the tracked worktree at subscribe. The map is
   *  replaced with the reconciled state before the batch is returned, so an
   *  empty difference still seeds later live deletions. */
  bootBatch(storedRows: readonly IRow[]): IArrivalBatch {
    const stored = new Map<string, string>();
    for (const row of storedRows) {
      if (row[0] !== this.glob) continue;
      stored.set(String(row[1] ?? ""), String(row[2] ?? ""));
    }

    const disk = new Map<string, string>();
    for (const relativePath of trackedPaths(this.root, this.glob)) {
      const digest = digestOf(path.resolve(this.root, relativePath));
      if (digest !== null) disk.set(relativePath, digest);
    }

    const arrivals: IArrivalRow[] = [];
    this.lastDigest.clear();
    for (const [relativePath, previous] of [...stored].sort(([left], [right]) => left.localeCompare(right))) {
      const current = disk.get(relativePath);
      if (current === previous) {
        this.lastDigest.set(relativePath, current);
      } else {
        arrivals.push({ rel: this.rel, sign: "del", row: [this.glob, relativePath, previous] });
        if (current !== undefined) {
          arrivals.push({ rel: this.rel, sign: "add", row: [this.glob, relativePath, current] });
          this.lastDigest.set(relativePath, current);
        }
      }
      disk.delete(relativePath);
    }
    for (const [relativePath, current] of [...disk].sort(([left], [right]) => left.localeCompare(right))) {
      arrivals.push({ rel: this.rel, sign: "add", row: [this.glob, relativePath, current] });
      this.lastDigest.set(relativePath, current);
    }
    return arrivals;
  }

  /** One coalesce window's paths -> the arrival batch it owes. Duplicated paths
   *  inside the window collapse first (a save is several fs events), so a file
   *  is read and hashed at most once per window. */
  batchFor(paths: readonly string[]): IArrivalBatch {
    const arrivals: IArrivalRow[] = [];
    for (const absolutePath of new Set(paths)) {
      const relativePath = path.relative(this.root, absolutePath);
      if (relativePath.startsWith("..") || path.isAbsolute(relativePath)) continue;
      if (!path.matchesGlob(relativePath, this.glob)) continue;
      const previous = this.lastDigest.get(relativePath);
      const current = digestOf(absolutePath);
      if (current === previous) continue;
      if (previous !== undefined) {
        arrivals.push({ rel: this.rel, sign: "del", row: [this.glob, relativePath, previous] });
        this.lastDigest.delete(relativePath);
      }
      if (current !== null) {
        arrivals.push({ rel: this.rel, sign: "add", row: [this.glob, relativePath, current] });
        this.lastDigest.set(relativePath, current);
      }
    }
    return arrivals;
  }
}

export class WatchBindRunner implements IWatchBindRunner {
  readonly firings$: Observable<IWatchFired>;

  constructor(
    engine: ILiveEngine,
    plans: readonly IBindPlan[],
    options: {
      readonly root: string;
      readonly coalesceMs: number;
      readonly scheduler: SchedulerLike;
      readonly source: IWatchSource;
    },
  ) {
    const watches = plans.flatMap((plan) => {
      if (plan.columns.length !== 3) {
        throw new Error(
          `bind '${plan.name}' has ${plan.columns.length} columns; the watch row shape is (glob, path, digest)`,
        );
      }
      return plan.literals.map((literal) => {
        const glob = String(literal);
        const state = new GlobWatch(glob, plan.name, options.root);
        const commit = (batch: IArrivalBatch) =>
          engine.submit(batch).pipe(
            take(1),
            map((outcome): IWatchFired => {
              const added = batch.filter((arrival) => arrival.sign === "add").length;
              ServeTrace.watch(plan.name, glob, added, batch.length - added);
              return { rel: plan.name, glob, added, removed: batch.length - added, tick: outcome.tick };
            }),
          );
        const boot = defer(() => engine.rows(plan.name)).pipe(
          map((rows) => ({ kind: "boot" as const, batch: state.bootBatch(rows) })),
        );
        const liveWindows = options.source.watch(watchRootOf(options.root, glob)).pipe(
          bufferTime(options.coalesceMs, options.scheduler),
          filter((paths) => paths.length > 0),
          map((paths) => ({ kind: "paths" as const, paths })),
        );
        return merge(liveWindows, boot).pipe(
          scan(
            (
              folded: {
                readonly booted: boolean;
                readonly pending: readonly (readonly string[])[];
                readonly batches: readonly IArrivalBatch[];
              },
              input,
            ) => {
              if (input.kind === "paths" && !folded.booted) {
                return { booted: false, pending: [...folded.pending, input.paths], batches: [] };
              }
              if (input.kind === "boot") {
                return {
                  booted: true,
                  pending: [],
                  batches: [input.batch, ...folded.pending.map((paths) => state.batchFor(paths))],
                };
              }
              return { booted: true, pending: [], batches: [state.batchFor(input.paths)] };
            },
            { booted: false, pending: [], batches: [] } as {
              readonly booted: boolean;
              readonly pending: readonly (readonly string[])[];
              readonly batches: readonly IArrivalBatch[];
            },
          ),
          concatMap((folded) => from(folded.batches)),
          filter((batch) => batch.length > 0),
          concatMap(commit),
        );
      });
    });
    this.firings$ = watches.length > 0 ? merge(...watches) : EMPTY;
  }
}

/** The executor split, by name, so an unknown one is refused rather than run as
 *  something else. `4_http.ts` asks this before constructing either runner. */
export function bindPlansFor(plans: readonly IBindPlan[], execution: string): readonly IBindPlan[] {
  const known = new Set(["live_interval", "live_watch"]);
  for (const plan of plans) {
    if (!known.has(plan.execution)) {
      throw new Error(`unknown bind executor '${plan.execution}' for bind '${plan.name}'`);
    }
  }
  return plans.filter((plan) => plan.execution === execution);
}
