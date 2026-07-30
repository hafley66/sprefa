/**
 * watchCounts.test.ts — COUNT TESTS for the watcher's per-file path
 * (user-set law 2026-07-28: "any path that was ever O(n^2) gets a test
 * asserting the operation count/plan, never end-state equality alone"; the
 * watcher is a new per-file path, so it is born with one).
 *
 * Three counts, none of them an end-state equality:
 *
 *   1  TICKS PER BURST IS ONE, at 5 files and at 50. The coalesce window is
 *      what makes a `git checkout` a handful of ticks rather than one per file,
 *      and "one window is one batch is one tick" is the whole of that claim.
 *   2  ARRIVAL ROWS PER BURST equal the FILE count, not the EVENT count, at 5
 *      and at 50 with three filesystem events per file. A save is several fs
 *      events; a file is one row.
 *   3  STATEMENTS PER TICK ARE FLAT from 5 files to 50 -- read off the served
 *      engine's own `sprefa:tick` channel (serve/0_trace.ts), not off a counter
 *      this test keeps. Flat is the claim: the tick cost is a function of the
 *      program's shape, never of how many files arrived in the batch.
 *
 * WHY NOT A SYSCALL COUNT. `GlobWatch.batchFor` reads each path at most once
 * per window; the obvious receipt would be counting `readFileSync`. It is not
 * assertable from here and this test does not pretend otherwise: node's ESM
 * named bindings for builtins are snapshotted at link time, so patching
 * `require("node:fs").readFileSync` does not intercept 2_binds.ts's imported
 * binding (measured: the patched counter sees 1 of 2 calls). Count 2 is the
 * observable half of the same claim.
 *
 * SABOTAGE RECEIPTS (run 2026-07-29, both reverted, both counts red in each):
 *  - replacing the watch runner's `bufferTime(coalesceMs, scheduler)` with
 *    `map(path => [path])` (no coalescing): "5 files in one window must be 1
 *    tick, got 5", and the span count follows with "one tick, one span".
 *  - dropping `filter(batch => batch.length > 0)`: "5 files in one window must
 *    be 1 tick, got 2" -- the empty window after the burst commits its own
 *    tick.
 */

import assert from "node:assert/strict";
import diagnostics_channel from "node:diagnostics_channel";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { Observable, Subject, VirtualTimeScheduler } from "rxjs";

import { SERVE_CHANNEL_NAMES } from "../serve/0_trace.ts";
import type { IServeTickEvent, IWatchSource } from "../runtime/types.ts";
import { postProgram, request, startServed, tickEvents } from "./serveHelpers.ts";

const WATCH_RAIL_DL6 = fileURLToPath(new URL("../../dl/fixtures/served-watch-rail.dl6", import.meta.url));
const COALESCE_MS = 50;

class ScriptedWatchSource implements IWatchSource {
  private readonly paths = new Subject<string>();

  constructor(private readonly scheduler: VirtualTimeScheduler) {}

  watch(): Observable<string> {
    return this.paths.asObservable();
  }

  notify(paths: readonly string[]): void {
    for (const path of paths) this.paths.next(path);
  }

  settle(): void {
    this.scheduler.maxFrames = this.scheduler.frame + COALESCE_MS * 2;
    this.scheduler.flush();
  }
}

/** The served engine publishes one of these per tick when anything listens
 *  (serve/0_trace.ts checks `hasSubscribers` before publishing). Listening here
 *  is what turns the statement counter on; nothing else changes. */
const tickSpans: IServeTickEvent[] = [];
diagnostics_channel.channel(SERVE_CHANNEL_NAMES.tick).subscribe((message) => {
  tickSpans.push(message as IServeTickEvent);
});

function settled(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 60));
}

interface BurstCounts {
  readonly ticks: number;
  readonly arrivalRows: number;
  readonly rows: number;
  readonly statementsPerTick: readonly number[];
}

/** One burst of `fileCount` files, each notified `eventsPerFile` times. */
/** Ports are ephemeral: every server here binds 0 and the receipt reads back
 *  `served.port`. Four hardcoded numbers here were four collisions waiting for a
 *  second lane (bug hostdecode_hardcoded_port_collision). */
async function burst(fileCount: number, eventsPerFile: number): Promise<BurstCounts> {
  const source = readFileSync(WATCH_RAIL_DL6, "utf8");
  const root = mkdtempSync(join(tmpdir(), "tsv2-watch-count-"));
  const scheduler = new VirtualTimeScheduler();
  const watchSource = new ScriptedWatchSource(scheduler);
  const served = await startServed(0, scheduler, ":memory:", {
    watchRoot: root,
    watchCoalesceMs: COALESCE_MS,
    watchSource,
  });
  try {
    assert.equal((await postProgram(served.port, source)).statusCode, 200);

    const paths: string[] = [];
    for (let index = 0; index < fileCount; index += 1) {
      const absolute = join(root, `file${index}.ts`);
      writeFileSync(absolute, `export const value${index} = ${index};\n`);
      for (let repeat = 0; repeat < eventsPerFile; repeat += 1) paths.push(absolute);
    }

    const spansBefore = tickSpans.length;
    watchSource.notify(paths);
    watchSource.settle();
    await settled();

    const outcomes = tickEvents(served.events);
    const watchDelta = outcomes[0]?.deltas.rels.find((delta) => delta.rel === "watch");
    const rowsReply = await request(served.port, "/idb/seen", "GET");
    return {
      ticks: outcomes.length,
      arrivalRows: (watchDelta?.add.length ?? 0) + (watchDelta?.del.length ?? 0),
      rows: (JSON.parse(rowsReply.body) as { rows: unknown[] }).rows.length,
      statementsPerTick: tickSpans.slice(spansBefore).map((span) => span.statements),
    };
  } finally {
    await served.stop();
    rmSync(root, { recursive: true, force: true });
  }
}

test("count: one coalesce window is ONE tick, and one file is ONE row however many events it fired", async () => {
  const small = await burst(5, 3);
  const large = await burst(50, 3);

  assert.equal(small.ticks, 1, `5 files in one window must be 1 tick, got ${small.ticks}`);
  assert.equal(large.ticks, 1, `50 files in one window must be 1 tick, got ${large.ticks}`);
  assert.equal(small.arrivalRows, 5, `5 files x 3 events must be 5 arrival rows, got ${small.arrivalRows}`);
  assert.equal(large.arrivalRows, 50, `50 files x 3 events must be 50 arrival rows, got ${large.arrivalRows}`);
  assert.equal(small.rows, 5);
  assert.equal(large.rows, 50);
});

test("count: statements per tick are FLAT from a 5-file burst to a 50-file burst", async () => {
  const small = await burst(5, 1);
  const large = await burst(50, 1);

  assert.equal(small.statementsPerTick.length, 1, "one tick, one span");
  assert.equal(large.statementsPerTick.length, 1, "one tick, one span");
  assert.ok((small.statementsPerTick[0] ?? 0) > 0, "the tracing channel produced no statement count at all");
  assert.equal(
    large.statementsPerTick[0],
    small.statementsPerTick[0],
    `statements per tick moved with corpus size: ${small.statementsPerTick[0]} at 5 files, ` +
      `${large.statementsPerTick[0]} at 50`,
  );
});
