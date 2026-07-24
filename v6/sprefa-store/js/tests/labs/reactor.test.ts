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
  firstValueFrom,
} from "rxjs";

// ── marble recorder ──────────────────────────────────────────────────────────
interface Marble {
  readonly t: number; // ms since scenario start
  readonly lane: string;
  readonly v: string;
}
class MarbleLog {
  private start = performance.now();
  readonly events: Marble[] = [];
  rec(lane: string, v: string): void {
    this.events.push({ t: Math.round(performance.now() - this.start), lane, v });
  }
  print(title: string): void {
    // eslint-disable-next-line no-console
    console.log(`\n  ── ${title} ──`);
    for (const e of this.events) {
      // eslint-disable-next-line no-console
      console.log(`  t=${String(e.t).padStart(3)}ms  ${e.lane.padEnd(28)} ${e.v}`);
    }
  }
}

interface FileEvent {
  readonly path: string;
  readonly op: "edit" | "create" | "rename";
  readonly isDir: boolean;
}

const TICK = 25; // the cadence drum (interval)

/**
 * Wire the tapped loop. `extract(path)` is the side-effectual jsonl stream: it emits
 * one parsed row per JSONL_GAP ms, simulating an incremental read of a jsonl file as
 * the extractor writes it. `stop$` lets scenario B halt the loop for a clean readout.
 */
function buildReactor(
  log: MarbleLog,
  fileEvents$: Subject<FileEvent>,
  jsonlFor: (path: string) => readonly string[],
  stop$: Subject<void>,
) {
  const tick$ = interval(TICK).pipe(share());

  const extract = (path: string) =>
    from(jsonlFor(path)).pipe(
      concatMap((line, i) => timer(8 * (i + 1)).pipe(map(() => line))), // incremental: 8ms/line
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

test("reactor A: file+folder coalesce into one tick; extract wakes; each jsonl line re-derives count", async () => {
  const log = new MarbleLog();
  const fileEvents$ = new Subject<FileEvent>();
  const stop$ = new Subject<void>();
  const jsonl = ["hit:foo", "hit:bar", "hit:baz"]; // 3 lines the extractor will stream
  const engine$ = buildReactor(log, fileEvents$, () => jsonl, stop$);

  const counts: number[] = [];
  const sub = engine$.subscribe((c) => counts.push(c));

  // two events land in the SAME tick window (before the first interval fires):
  fileEvents$.next({ path: "cli.ts", op: "edit", isDir: false });
  await new Promise((r) => setTimeout(r, 6));
  fileEvents$.next({ path: "src", op: "rename", isDir: true }); // a FOLDER change
  // let the tick fire and the 3-line extract fully stream through
  await new Promise((r) => setTimeout(r, 120));
  stop$.next();
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

test("reactor B: a change mid-extract cancels the stale jsonl stream (switchMap cancel-stale)", async () => {
  const log = new MarbleLog();
  const fileEvents$ = new Subject<FileEvent>();
  const stop$ = new Subject<void>();
  // first extract streams a LONG jsonl (5 lines); the second is short (1 line).
  const jsonlByPath: Record<string, string[]> = {
    "old.ts": ["a", "b", "c", "d", "e"],
    "new.ts": ["Z"],
  };
  const engine$ = buildReactor(log, fileEvents$, (p) => jsonlByPath[p] ?? [], stop$);

  const counts: number[] = [];
  const sub = engine$.subscribe((c) => counts.push(c));

  fileEvents$.next({ path: "old.ts", op: "edit", isDir: false });
  // wait for the tick + a couple of old.ts lines to stream, THEN change again mid-extract
  await new Promise((r) => setTimeout(r, 45));
  fileEvents$.next({ path: "new.ts", op: "edit", isDir: false }); // invalidates mid-stream
  await new Promise((r) => setTimeout(r, 120));
  stop$.next();
  sub.unsubscribe();

  log.print("Scenario B: extract old.ts (5 lines), change to new.ts mid-stream");

  // The stale old.ts extract is cancelled: count never reaches 5. The new.ts extract runs
  // fresh from an empty accumulator → count=1.
  assert.ok(!counts.includes(5), `stale extract must be cancelled before completing, saw counts=${JSON.stringify(counts)}`);
  assert.equal(counts[counts.length - 1], 1, "the fresh new.ts extract yields count=1");
});
