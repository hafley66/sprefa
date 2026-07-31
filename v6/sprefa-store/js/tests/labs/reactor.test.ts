/**
 * reactor.test.ts — a RUNNABLE, fully-tapped tick loop, so the marble trace is real
 * operator output, not a drawn diagram. It answers: when does the reconciler wake the
 * side-effectual calls, and what happens when an effect result invalidates a downstream
 * rel. Every operator has a tap() that records a marble; the test prints the timeline
 * and asserts the ordering that the walkthrough explains.
 *
 * The rel graph in this demo (in-memory store; the real engine keeps rows in SQLite):
 *   file      EDB   — file/folder change events (the source)
 *   matches   PORT  — an EXTRACTION rel filled by a jsonl stream (the side effect)
 *   count     IDB   — derived; depends on matches (the thing that gets invalidated)
 *
 * Two scenarios:
 *   A. a file edit + a folder rename COALESCE into one tick; the extract effect wakes;
 *      jsonl lines arrive incrementally; each line invalidates `count` -> it re-derives.
 *   B. a second file change lands MID-EXTRACT; switchMap CANCELS the stale jsonl stream
 *      (cancel-stale) and re-runs from the new state. The stale lines never reach `count`.
 *
 * ─────────────────────────────────────────────────────────────────────────────
 * VIRTUAL TIME (ARCH task reactor_buffertime_flake). Both scenarios ran on the
 * REAL clock until 2026-07-31: `interval(25)` + `timer(8*(i+1))` inside the reactor,
 * `await setTimeout(...)` inside the test. Scenario A was a 6/18 flake under 3x load
 * (AssertionError actual [1] vs expected [1,2,3]).
 *
 * THIS IS THE VIRTUAL-TIME CASE, not the deliberately-real-time case. The F3 arc
 * (v6/dl/tests/4_binds.test.ts) left two tests on the real scheduler because
 * `bucketFor` reads real `Date.now()` for the VALUES it commits, so collapsing two
 * firings onto one virtual flush would falsify the assertion for a reason unrelated
 * to the code. Nothing here has that property: the two assertions read the `counts`
 * array (an ordering property) and a count of ④ marbles (a coalescing property).
 * No asserted value is derived from a wall-clock reading. Timestamps appear only in
 * the printed timeline, and MarbleLog now takes its clock as an argument, so the
 * printout reports VIRTUAL frames and stays readable.
 *
 * The real-clock margin that was being lost, measured on the pre-rewrite file:
 *   scenario A wake at t=27ms, jsonl lines at t=35/53/77ms, stop$ at t≈126ms.
 * Every one of those four is an independent `setTimeout`; ~49ms of accumulated
 * scheduler slippage truncates `counts` to [1] or [1,2]. Scenario B carried the
 * same defect in the other direction (its `!counts.includes(5)` assertion needs the
 * 45ms nudge to land BEFORE the 5-line extract finishes at t≈145ms), so both
 * scenarios were moved; `buildReactor` takes the scheduler and the tests drive
 * `TestScheduler.flush()`. Zero real-clock sleep remains in this file.
 *
 * HEAD-TO-HEAD RECEIPT (12-core box; 48 busy-loop node processes as CPU load, the
 * test file run 8x in parallel, 5 rounds = 40 runs of each version under the SAME
 * load generator):
 *   pre-rewrite file (git HEAD copy)   36/40 green — 4 failures, signatures
 *                                      "actual [1,2] / expected [1,2,3]" and
 *                                      "actual [1] / expected [1,2,3]" (the second
 *                                      is the exact signature the ARCH row reports)
 *   this file                          40/40 green
 * Note the load level matters: at 3x parallel with no CPU hogs BOTH versions ran
 * 18/18 and 21/21 green, so a plain re-run count is not evidence here — the
 * discriminating comparison is the loaded one above. The virtual-time version
 * also runs in ~2.3ms + ~0.4ms instead of ~130ms + ~167ms.
 *
 * Only the raw `VirtualTimeScheduler` surface (`schedule`/`flush`/`frame`) is used;
 * `TestScheduler.run()`/`expectObservable` (the marble-diagram API) is not, since
 * the point of this file is that the marbles are RECORDED from live operator output
 * rather than declared. Same choice, same reason, as 4_binds.test.ts.
 *
 * SABOTAGE RECEIPTS (each applied to this file, run, reverted; outputs quoted as
 * observed, not as predicted):
 *  1. coalescing — `buffer(tick$)` replaced by `map((ev) => [ev])` (every event its
 *     own batch, the no-coalescing shape). Scenario A RED:
 *     "file + folder events coalesced into a single tick → one effect wake:
 *      actual 2, expected 1". Note which assertion caught it: `counts` STAYED
 *     [1,2,3] and passed, because the second event's own wake (frame 6) cancels
 *     the first extract before its first line (frame 8) — so the ④-count is the
 *     only assertion that discriminates coalescing here, and deleting it would
 *     leave the property untested.
 *  2. cancel-stale — `switchMap` replaced by `concatMap` (the stale extract is no
 *     longer cancelled, it runs to completion first). Scenario B RED:
 *     "stale extract must be cancelled before completing, saw counts=[1,2,3,4,5,1]".
 *     Scenario A stayed GREEN, as expected: nothing is cancelled there.
 *  3. vacuity — `scheduler.flush()` deleted from scenario A. RED with
 *     "actual: [], expected: [1,2,3]", confirming the assertions are not passing
 *     on an empty run (the standing risk when a timing test moves to virtual time).
 * All three were reverted before this file was finalized and both tests re-run GREEN.
 */

import { test } from "node:test";
import { strict as assert } from "node:assert";
import {
  Subject,
  interval,
  from,
  timer,
  buffer,
  filter,
  map,
  switchMap,
  concatMap,
  scan,
  tap,
  share,
  takeUntil,
  type SchedulerLike,
} from "rxjs";
import { TestScheduler } from "rxjs/testing";

// ── marble recorder ──────────────────────────────────────────────────────────
interface Marble {
  readonly t: number; // frames since scenario start (virtual ms)
  readonly lane: string;
  readonly v: string;
}
class MarbleLog {
  private readonly start: number;
  readonly events: Marble[] = [];
  /** `now` is the clock the marbles are stamped with — the virtual scheduler's frame
   *  counter, so a printed timeline still reads in ms even with zero real sleep. */
  constructor(private readonly now: () => number) {
    this.start = now();
  }
  rec(lane: string, v: string): void {
    this.events.push({ t: Math.round(this.now() - this.start), lane, v });
  }
  print(title: string): void {
    // eslint-disable-next-line no-console
    console.log(`\n  ── ${title} ──`);
    for (const e of this.events) {
      // eslint-disable-next-line no-console
      console.log(`  t=${String(e.t).padStart(3)}   ${e.lane.padEnd(28)} ${e.v}`);
    }
  }
}

interface FileEvent {
  readonly path: string;
  readonly op: "edit" | "create" | "rename";
  readonly isDir: boolean;
}

const TICK = 25; // the cadence drum (interval)

/** No-op assertDeepEqual: this file never calls `TestScheduler.run()`/`expectObservable`,
 *  only the `VirtualTimeScheduler` surface TestScheduler inherits — but the constructor
 *  still requires the callback positionally. (Same shim as v6/dl/tests/4_binds.test.ts.) */
function newTestScheduler(): TestScheduler {
  return new TestScheduler(() => {});
}

/**
 * Wire the tapped loop. `extract(path)` is the side-effectual jsonl stream: it emits
 * one parsed row per JSONL_GAP ms, simulating an incremental read of a jsonl file as
 * the extractor writes it. `stop$` lets a scenario halt the loop for a clean readout.
 * `scheduler` drives BOTH time sources (the cadence drum and the per-line gap), which
 * is what makes the assertions independent of machine load.
 */
function buildReactor(
  log: MarbleLog,
  fileEvents$: Subject<FileEvent>,
  jsonlFor: (path: string) => readonly string[],
  stop$: Subject<void>,
  scheduler: SchedulerLike,
) {
  const tick$ = interval(TICK, scheduler).pipe(share());

  const extract = (path: string) =>
    from(jsonlFor(path)).pipe(
      concatMap((line, i) => timer(8 * (i + 1), scheduler).pipe(map(() => line))), // incremental: 8ms/line
    );

  const engine$ = fileEvents$.pipe(
    tap((ev) => log.rec("① fileEvents$ (Subject)", `${ev.path}${ev.isDir ? "/" : ""} ${ev.op}`)),
    buffer(tick$), // coalesce every event since the last tick into ONE batch
    filter((batch) => batch.length > 0),
    tap((batch) => log.rec("② buffer(tick$)", `[${batch.map((e) => e.path).join(", ")}]`)),
    map((batch) => {
      // markChanged: which rels did this batch dirty? Any file/folder event dirties `file`,
      // which (per the dep graph) forces `matches` to re-extract.
      const dirty = new Set<string>(["file", "matches"]);
      return { dirty, cause: batch.map((e) => e.path) };
    }),
    tap((d) => log.rec("③ map: markChanged→dirty", `{${[...d.dirty].join(", ")}}`)),
    switchMap((d) => {
      // THE WAKE: the dirty frontier re-derives. `matches` is a PORT rel, so its rederive
      // IS the side effect (the jsonl extract). switchMap = cancel-stale: a newer tick
      // aborts an in-flight extract before it can reach `count`.
      log.rec("④ switchMap: WAKE effect", `re-extract matches from ${d.cause.join("+")}`);
      return extract(d.cause[d.cause.length - 1]!).pipe(
        tap((line) => log.rec("⑤ jsonl$ (concatMap, incremental)", line)),
        scan((acc, line) => [...acc, line], [] as string[]), // matches accumulates each line
        tap((matches) => log.rec("⑥ scan: matches rel", `n=${matches.length} ${JSON.stringify(matches)}`)),
        map((matches) => matches.length), // count(matches) — the derived rel
        tap((c) => log.rec("⑦ map: count rel (INVALIDATED→rederive)", `count=${c}`)),
      );
    }),
    share(), // multicast: one derivation, many readers
    takeUntil(stop$),
  );

  return engine$;
}

// =============================================================================
// Scenario A — coalesce + wake + incremental invalidation cascade.
// =============================================================================

test("reactor A: file+folder coalesce into one tick; extract wakes; each jsonl line re-derives count", () => {
  const scheduler = newTestScheduler();
  const log = new MarbleLog(() => scheduler.frame);
  const fileEvents$ = new Subject<FileEvent>();
  const stop$ = new Subject<void>();
  const jsonl = ["hit:foo", "hit:bar", "hit:baz"]; // 3 lines the extractor will stream
  const engine$ = buildReactor(log, fileEvents$, () => jsonl, stop$, scheduler);

  const counts: number[] = [];
  const sub = engine$.subscribe((c) => counts.push(c));

  // two events land in the SAME tick window (frame 0 and frame 6, both before the
  // first interval fires at frame 25):
  scheduler.schedule(() => fileEvents$.next({ path: "cli.ts", op: "edit", isDir: false }), 0);
  scheduler.schedule(() => fileEvents$.next({ path: "src", op: "rename", isDir: true }), 6); // a FOLDER change
  // frame 126: the same readout point the real-clock version used (wake at 25,
  // lines at 33/49/73), now reached without a single real millisecond of sleep.
  scheduler.schedule(() => stop$.next(), 126);
  scheduler.flush();
  sub.unsubscribe();

  log.print("Scenario A: edit cli.ts + rename src/  →  extract 3 jsonl lines");

  // count re-derives once per jsonl line: 1, 2, 3 (each line invalidates count).
  assert.deepEqual(counts, [1, 2, 3], "count re-derives incrementally as jsonl lines arrive");
  // the two events coalesced: exactly ONE extract wake (one ④), not two.
  const wakes = log.events.filter((e) => e.lane.startsWith("④")).length;
  assert.equal(wakes, 1, "file + folder events coalesced into a single tick → one effect wake");
});

// =============================================================================
// Scenario B — invalidation MID-EFFECT: switchMap cancels the stale extract.
// =============================================================================

test("reactor B: a change mid-extract cancels the stale jsonl stream (switchMap cancel-stale)", () => {
  const scheduler = newTestScheduler();
  const log = new MarbleLog(() => scheduler.frame);
  const fileEvents$ = new Subject<FileEvent>();
  const stop$ = new Subject<void>();
  // first extract streams a LONG jsonl (5 lines); the second is short (1 line).
  const jsonlByPath: Record<string, string[]> = {
    "old.ts": ["a", "b", "c", "d", "e"],
    "new.ts": ["Z"],
  };
  const engine$ = buildReactor(log, fileEvents$, (p) => jsonlByPath[p] ?? [], stop$, scheduler);

  const counts: number[] = [];
  const sub = engine$.subscribe((c) => counts.push(c));

  scheduler.schedule(() => fileEvents$.next({ path: "old.ts", op: "edit", isDir: false }), 0);
  // frame 45: the tick has fired (25) and two old.ts lines have streamed (33, 49 —
  // the second lands on the next tick boundary), so this change arrives MID-EXTRACT.
  // Without cancellation old.ts would keep going to its 5th line at frame 145.
  scheduler.schedule(() => fileEvents$.next({ path: "new.ts", op: "edit", isDir: false }), 45);
  scheduler.schedule(() => stop$.next(), 165);
  scheduler.flush();
  sub.unsubscribe();

  log.print("Scenario B: extract old.ts (5 lines), change to new.ts mid-stream");

  // The stale old.ts extract is cancelled: count never reaches 5. The new.ts extract runs
  // fresh from an empty accumulator → count=1.
  assert.ok(!counts.includes(5), `stale extract must be cancelled before completing, saw counts=${JSON.stringify(counts)}`);
  assert.equal(counts[counts.length - 1], 1, "the fresh new.ts extract yields count=1");
});
