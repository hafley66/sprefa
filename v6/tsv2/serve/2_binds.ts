/**
 * Live interval and filesystem watch bind sources.
 *
 *   interval(periodMs, scheduler) -> map(toBucketRow) -> mergeMap(submit)
 *   watchSource(root) -> bufferTime(coalesceMs) -> map(diffAgainstLast) -> submit
 *
 * The bind declaration authorizes a world source; the program's rules say
 * which instances it consumes, as literals in the bind
 * atom's first column (registry.pl calls it the configuration column):
 * `interval(300, Bucket)`, `watch("src/**\/*.ts", Path, Digest)`. emit_ts.pl
 * collects those into the emitted plan's `literals`, so this file starts
 * exactly what the program asked for and no default. A program that declares a
 * bind and reads no literal gets `literals: []` and therefore no live source at
 * all -- an honest zero, never an invented cadence or an invented glob.
 *
 * The cadence is a compile-time fact of the program text and is read once per
 * program load.
 *
 * Buckets use `floor(epoch_secs / period)`,
 * never a process-local counter, so a restart does not replay bucket 0,1,2...
 * The scheduler seam moves the FIRING; the bucket VALUE stays real wall clock
 * (a TestScheduler run therefore produces real-clock bucket values, which is
 * why the receipt asserts firing COUNT and column shape, not bucket numbers).
 *
 * Teardown is unsubscription: an rxjs `interval` clears its timer and the
 * IS the underlying clearInterval, and the watch source's unsubscribe aborts
 * the `fsPromises.watch` iterator. A program swap's `switchMap` therefore stops
 * every world source with no Subscription field and no dispose() here.
 * The watcher seam carries
 * paths and digest-based arrival signs instead of backend event vocabulary.
 * The backend's create/update/delete vocabulary does not survive
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
 * Boot reconciliation runs once per watched glob at
 * subscribe, read the engine's durable watch rows and compare them with the
 * tracked worktree set from `git ls-files`. Their difference is one arrival
 * batch, and the reconciled rows seed `lastDigest`. This is the named one-shot
 * crossing of A12's push/demand line; after boot, the live path remains bare
 * paths from the watch source, sign from digest comparison, and `bufferTime`
 * coalescing.
 *
 * One glob matcher serves both boot and live paths. `git ls-files` supplies the
 * tracked SET and nothing else -- it is called with no pathspec -- and
 * membership is decided for boot and for live by the same `matchesWatchGlob`
 * call. The boot==live property is covered by tests/watchGlobDialect.test.ts.
 *
 * The coalesce window is 100ms by default (`IServeConfig.watch_coalesce_ms`), on the
 * injected scheduler so a test drives it on virtual time. `bufferTime` and not
 * `debounceTime`: a debounce never emits while a large `git checkout` keeps
 * firing, while a fixed window is bounded on BOTH sides -- a 2-second checkout
 * is ~20 ticks rather than one per file, and no burst can starve the engine.
 */

import { spawnSync } from "node:child_process";
import { readFileSync, statSync } from "node:fs";
import * as path from "node:path";
import { watch as watch_directory } from "node:fs/promises";

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
  IBindPlansFor,
  IIntervalBindRunner,
  ILiveEngine,
  IMatchesWatchGlob,
  IRowValue,
  IRow,
  IWatchBindRunner,
  IWatchFired,
  IWatchRootOf,
  IWatchSource,
} from "../runtime/types.ts";
import { ServeTrace } from "./0_trace.ts";

function bucket_for(period_secs: number): number {
  return Math.floor(Date.now() / 1000 / period_secs);
}

/** The `interval` bind's row is (period, bucket) by registry definition
 *  (registry.pl `bind_definition/2`), so the row is built positionally from the
 *  plan's own declared columns: first column carries the period, second the
 *  bucket. A future bind with another column shape needs its own row builder
 *  and would be refused by name below rather than filled by guess. */
function bucket_row(plan: IBindPlan, period_secs: number): readonly IRowValue[] {
  return plan.columns.map((_column, index) => (index === 0 ? period_secs : bucket_for(period_secs)));
}

/** `interval`'s configuration column is `period: int`, so a non-integer literal
 *  there is a program the compiler's own column typing should already have
 *  rejected. Named rather than filtered: a silently dropped cadence is a timer
 *  that never fires and a program that looks like it is running. */
function interval_periods(plan: IBindPlan): readonly number[] {
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
      return interval_periods(plan).map((period_secs) =>
        interval(period_secs * 1000, scheduler).pipe(
          map((): IArrivalRow => ({ rel: plan.name, sign: "add", row: bucket_row(plan, period_secs) })),
          mergeMap((arrival) =>
            // One firing is one reported value: `take(1)` keeps the SETTLE tick
            // and lets any drain ticks the batch causes flow out on `ticks$`
            // like every other tick. The batch itself is already enqueued in
            // the engine, so completing early cannot cancel it.
            engine.submit([arrival]).pipe(
              take(1),
              map((outcome): IBindFired => {
                const bucket = Number(arrival.row[1] ?? 0);
                ServeTrace.bind(plan.name, period_secs, bucket);
                return { rel: plan.name, period: period_secs, bucket, tick: outcome.tick };
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
export const watch_root_of: IWatchRootOf = (root: string, glob: string): string => {
  const magic = glob.search(/[*?[\]{}]/);
  const literal_prefix = magic < 0 ? glob : glob.slice(0, magic);
  const last_slash = literal_prefix.lastIndexOf("/");
  const relative_directory = last_slash < 0 ? "" : literal_prefix.slice(0, last_slash);
  return path.resolve(root, relative_directory);
};

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
      return from(watch_directory(root, { recursive: true, signal: controller.signal })).pipe(
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
function digest_of(absolute_path: string): string | null {
  try {
    if (!statSync(absolute_path).isFile()) return null;
    return bytesToHex(sha256(readFileSync(absolute_path)));
  } catch {
    return null;
  }
}

/**
 * THE glob matcher, and the only one: ruling `glob_dialect =
 * node_matcher_both_halves` (rulings.pl, user 2026-07-31). Both halves of the
 * watch bind call THIS function, which is the whole point of it existing --
 * `bootBatch` and `batchFor` cannot drift into two dialects again without
 * deleting a call site, and the boot==live property test
 * (tests/watchGlobDialect.test.ts) fails the moment one of them does.
 *
 * WHY NODE'S MATCHER rather than git's pathspec, from the census in
 * plans/2026-07-31-scan-spelling-card.md §2: the two dialects disagreed on 170
 * of the v5 corpus's 242 globs, and `matchesGlob` is the one that agrees with
 * v5's globset on every measured case. Pathspec's `*` crosses `/`, its `**`
 * demands at least one directory (so `src/**\/*.rs` silently drops every direct
 * child of `src/` and `**\/*.md` drops every repo-root file), and it has no
 * brace alternation at all (`*.{rs,ts}` selects nothing). The matcher gets all
 * four right, so every v5 glob ports byte-unmodified.
 *
 * SCOPE, from the audit run alongside this fix: this is the ONLY glob consumer
 * in the tree that had two halves to reconcile. Every other one -- the
 * `files` / `files_at` / `resolve_at` / `grep_at` / `repo_grep_at` `sh`
 * hosts, and the receipt scripts that check them -- is a ONE-SHOT answer per
 * witness, has no live half to disagree with, and stays on git pathspec BY
 * DESIGN (`v6/dl/fixtures/files-hosts.dl6:19`, GETTING-STARTED §4). The
 * defect this file had is structurally impossible there.
 *
 * What the audit did leave open, and this function deliberately does NOT fix:
 * a brace glob posted to one of those pathspec-backed `want(glob)` demand rows
 * silently answers zero rows. `crawl-bench.sh` and `flagship-callgraph.sh` each
 * discovered that independently and hand-split the glob at the call site. The
 * fix there is a REFUSAL (pathspec is the intended dialect for a demand-row
 * host, so swapping the matcher would be wrong), which is a different change on
 * a different plane -- filed, not smuggled in here.
 */
export const matches_watch_glob: IMatchesWatchGlob = (relative_path: string, glob: string): boolean =>
  path.matchesGlob(relative_path, glob);

/** The tracked worktree, WHOLE: `git ls-files` with no pathspec, because the
 *  glob is not git's business any more (ruling above). Enumerating everything
 *  and filtering in JS keeps the files host's standing ignore decision --
 *  tracked-only, so an untracked `node_modules` is never walked, never listed
 *  and never hashed -- while leaving membership entirely to `matchesWatchGlob`.
 *  NUL framing preserves every path git permits. A non-repository root has the
 *  empty tracked set, which keeps injected filesystem tests independent of git
 *  while retaining the same tracked-only contract.
 *
 *  COST: one subprocess per watched glob at subscribe, unchanged from the
 *  pathspec call it replaces; what grows is the string list it returns (this
 *  repo: 3,906 tracked paths). Filtering happens BEFORE `digestOf`, so the
 *  number of files READ and hashed is still only the number the glob selects. */
function tracked_paths(root: string): readonly string[] {
  const result = spawnSync("git", ["ls-files", "-z"], {
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
  private readonly last_digest = new Map<string, string>();

  constructor(
    readonly glob: string,
    private readonly rel: string,
    private readonly root: string,
  ) {}

  /** Durable engine rows versus the tracked worktree at subscribe. The map is
   *  replaced with the reconciled state before the batch is returned, so an
   *  empty difference still seeds later live deletions. */
  boot_batch(stored_rows: readonly IRow[]): IArrivalBatch {
    const stored = new Map<string, string>();
    for (const row of stored_rows) {
      if (row[0] !== this.glob) continue;
      stored.set(String(row[1] ?? ""), String(row[2] ?? ""));
    }

    const disk = new Map<string, string>();
    for (const relative_path of tracked_paths(this.root)) {
      if (!matches_watch_glob(relative_path, this.glob)) continue;
      const digest = digest_of(path.resolve(this.root, relative_path));
      if (digest !== null) disk.set(relative_path, digest);
    }

    const arrivals: IArrivalRow[] = [];
    this.last_digest.clear();
    for (const [relative_path, previous] of [...stored].sort(([left], [right]) => left.localeCompare(right))) {
      const current = disk.get(relative_path);
      if (current === previous) {
        this.last_digest.set(relative_path, current);
      } else {
        arrivals.push({ rel: this.rel, sign: "del", row: [this.glob, relative_path, previous] });
        if (current !== undefined) {
          arrivals.push({ rel: this.rel, sign: "add", row: [this.glob, relative_path, current] });
          this.last_digest.set(relative_path, current);
        }
      }
      disk.delete(relative_path);
    }
    for (const [relative_path, current] of [...disk].sort(([left], [right]) => left.localeCompare(right))) {
      arrivals.push({ rel: this.rel, sign: "add", row: [this.glob, relative_path, current] });
      this.last_digest.set(relative_path, current);
    }
    return arrivals;
  }

  /** One coalesce window's paths -> the arrival batch it owes. Duplicated paths
   *  inside the window collapse first (a save is several fs events), so a file
   *  is read and hashed at most once per window. */
  batch_for(paths: readonly string[]): IArrivalBatch {
    const arrivals: IArrivalRow[] = [];
    for (const absolute_path of new Set(paths)) {
      const relative_path = path.relative(this.root, absolute_path);
      if (relative_path.startsWith("..") || path.isAbsolute(relative_path)) continue;
      if (!matches_watch_glob(relative_path, this.glob)) continue;
      const previous = this.last_digest.get(relative_path);
      const current = digest_of(absolute_path);
      if (current === previous) continue;
      if (previous !== undefined) {
        arrivals.push({ rel: this.rel, sign: "del", row: [this.glob, relative_path, previous] });
        this.last_digest.delete(relative_path);
      }
      if (current !== null) {
        arrivals.push({ rel: this.rel, sign: "add", row: [this.glob, relative_path, current] });
        this.last_digest.set(relative_path, current);
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
      readonly coalesce_ms: number;
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
          map((rows) => ({ kind: "boot" as const, batch: state.boot_batch(rows) })),
        );
        const live_windows = options.source.watch(watch_root_of(options.root, glob)).pipe(
          bufferTime(options.coalesce_ms, options.scheduler),
          filter((paths) => paths.length > 0),
          map((paths) => ({ kind: "paths" as const, paths })),
        );
        return merge(live_windows, boot).pipe(
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
                  batches: [input.batch, ...folded.pending.map((paths) => state.batch_for(paths))],
                };
              }
              return { booted: true, pending: [], batches: [state.batch_for(input.paths)] };
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
export const bind_plans_for: IBindPlansFor = (
  plans: readonly IBindPlan[],
  execution: string,
): readonly IBindPlan[] => {
  const known = new Set(["live_interval", "live_watch"]);
  for (const plan of plans) {
    if (!known.has(plan.execution)) {
      throw new Error(`unknown bind executor '${plan.execution}' for bind '${plan.name}'`);
    }
  }
  return plans.filter((plan) => plan.execution === execution);
};
