/**
 * serveWatch.test.ts — the `watch` bind's receipt (golden plan phase 2).
 *
 * The FILESYSTEM here is real (real files, real bytes, real sha256); what is
 * injected is the NOTIFICATION source, so a window closes when this test says
 * so instead of when the OS gets around to it. That is the same split
 * serveHost.test.ts makes for the interval bind: the cadence seam is virtual,
 * the values are real. `NodeWatchSource` -- the one class that names a watcher
 * library -- is covered end to end by scripts/extraction-live.sh, which drives
 * real editor-shaped saves through the actual kernel notifications.
 *
 * GRADING is the runtime bridge's TOTAL replay: every world push is read back
 * out of the tick's own boundary delta and fed to the oracle as its schedule,
 * so the diff below covers every column of every tick including the digests
 * this process computed live.
 *
 * SABOTAGE RECEIPTS (run 2026-07-29, both reverted):
 *  - dropping the `del` half of `GlobWatch.batchFor` (emitting only `add`)
 *    fails "an edit is one del+add batch" with 1 arrival instead of 2, and
 *    fails the oracle diff.
 *  - dropping the `current === previous` early-out (re-emitting an unchanged
 *    file) fails "identical bytes are not a change" with a fourth tick.
 *  - widening `path.matchesGlob` to accept everything fails "a path outside the
 *    glob is not a row" with a README.md row.
 */

import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { Observable, Subject, VirtualTimeScheduler } from "rxjs";

import { watchRootOf } from "../serve/2_binds.ts";
import type { IWatchSource } from "../runtime/types.ts";
import { logOfTicks, oracleLog, postProgram, request, scheduleFromTicks, startServed, tickEvents } from "./serveHelpers.ts";
import { readFileSync } from "node:fs";

const WATCH_RAIL_DL6 = fileURLToPath(new URL("../../dl/fixtures/served-watch-rail.dl6", import.meta.url));
const COALESCE_MS = 50;

/** A driveable `IWatchSource`: `notify(...)` plays the paths one underlying
 *  event burst would have produced, `settle()` closes the coalesce window. */
class ScriptedWatchSource implements IWatchSource {
  private readonly paths = new Subject<string>();

  constructor(private readonly scheduler: VirtualTimeScheduler) {}

  watch(): Observable<string> {
    return this.paths.asObservable();
  }

  notify(...paths: readonly string[]): void {
    for (const path of paths) this.paths.next(path);
  }

  /** Advance past one coalesce window so `bufferTime` emits. */
  settle(): void {
    this.scheduler.maxFrames = this.scheduler.frame + COALESCE_MS * 2;
    this.scheduler.flush();
  }
}

async function idbRows(port: number, rel: string): Promise<readonly (readonly (string | number)[])[]> {
  const reply = await request(port, `/idb/${rel}`, "GET");
  return (JSON.parse(reply.body) as { rows: readonly (readonly (string | number)[])[] }).rows;
}

function settled(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 40));
}

test("watch bind: real file bytes become (glob, path, digest) rows, one batch per coalesce window", async () => {
  const source = readFileSync(WATCH_RAIL_DL6, "utf8");
  const root = mkdtempSync(join(tmpdir(), "tsv2-watch-"));
  const scheduler = new VirtualTimeScheduler();
  const watchSource = new ScriptedWatchSource(scheduler);
  const served = await startServed(17541, scheduler, ":memory:", {
    watchRoot: root,
    watchCoalesceMs: COALESCE_MS,
    watchSource,
  });
  try {
    const loaded = await postProgram(served.port, source);
    assert.equal(loaded.statusCode, 200, loaded.body);
    const plans = JSON.parse(loaded.body) as {
      readonly binds: readonly { readonly name: string; readonly literals: readonly (string | number)[] }[];
      readonly arrivalTargets: readonly string[];
    };
    // The GLOB came from the program's own rule, exactly as the period does.
    assert.deepEqual(plans.binds, [{ name: "watch", literals: ["**/*.ts"] }]);

    // ── one window, two files, ONE tick ──────────────────────────────────
    mkdirSync(join(root, "src"), { recursive: true });
    writeFileSync(join(root, "src/one.ts"), "export const one = 1;\n");
    writeFileSync(join(root, "src/two.ts"), "export const two = 2;\n");
    writeFileSync(join(root, "README.md"), "not typescript\n");
    watchSource.notify(join(root, "src/one.ts"), join(root, "src/two.ts"), join(root, "README.md"));
    watchSource.settle();
    await settled();

    let ticks = tickEvents(served.events);
    assert.equal(ticks.length, 1, `two files in one window must be ONE tick, got ${ticks.length}`);
    const afterFirst = await idbRows(served.port, "seen");
    assert.equal(afterFirst.length, 2, "a path outside the glob is not a row");
    assert.deepEqual(
      afterFirst.map((row) => row[0]).sort(),
      ["src/one.ts", "src/two.ts"],
      "rows carry ROOT-RELATIVE paths",
    );

    // ── an edit is one del+add batch, in one tick ────────────────────────
    writeFileSync(join(root, "src/one.ts"), "export const one = 99;\n");
    watchSource.notify(join(root, "src/one.ts"));
    watchSource.settle();
    await settled();

    ticks = tickEvents(served.events);
    assert.equal(ticks.length, 2, "one edit is one tick");
    const editDelta = ticks[1]?.deltas.rels.find((delta) => delta.rel === "watch");
    assert.equal(editDelta?.add.length, 1, "an edit is one del+add batch");
    assert.equal(editDelta?.del.length, 1, "an edit is one del+add batch");
    assert.equal((await idbRows(served.port, "seen")).length, 2, "the edited path still has exactly one row");

    // ── identical bytes are not a change ─────────────────────────────────
    writeFileSync(join(root, "src/one.ts"), "export const one = 99;\n");
    watchSource.notify(join(root, "src/one.ts"), join(root, "src/two.ts"));
    watchSource.settle();
    await settled();
    assert.equal(tickEvents(served.events).length, 2, "identical bytes are not a change: no tick is owed");

    // ── a deletion is a bare del ─────────────────────────────────────────
    rmSync(join(root, "src/two.ts"));
    watchSource.notify(join(root, "src/two.ts"));
    watchSource.settle();
    await settled();

    ticks = tickEvents(served.events);
    assert.equal(ticks.length, 3, "one deletion is one tick");
    const removeDelta = ticks[2]?.deltas.rels.find((delta) => delta.rel === "watch");
    assert.equal(removeDelta?.add.length, 0);
    assert.equal(removeDelta?.del.length, 1);
    assert.deepEqual((await idbRows(served.port, "seen")).map((row) => row[0]), ["src/one.ts"]);

    // ── TOTAL grading: replay every world push through the oracle ────────
    const replayed = scheduleFromTicks(ticks, plans.arrivalTargets);
    assert.deepEqual(
      replayed.map((batch) => batch.map((arrival) => `${arrival.sign}:${arrival.rel}`)),
      [
        ["add:watch", "add:watch"],
        ["del:watch", "add:watch"],
        ["del:watch"],
      ],
    );
    assert.equal(logOfTicks(ticks), oracleLog(source, replayed));
  } finally {
    await served.stop();
    rmSync(root, { recursive: true, force: true });
  }
});

test("watch bind: the watched subtree is the glob's literal prefix, not the whole root", () => {
  // The kernel watch is opened on exactly this directory, so a glob rooted deep
  // in a repo never asks the OS to notify on the repo (the node_modules-scale
  // hazard named in SLOT-P2-ENUMERATE-SCOPE).
  assert.equal(watchRootOf("/repo", "v6/tsv2/**/*.ts"), "/repo/v6/tsv2");
  assert.equal(watchRootOf("/repo", "src/*.ts"), "/repo/src");
  assert.equal(watchRootOf("/repo", "**/*.ts"), "/repo");
  assert.equal(watchRootOf("/repo", "docs/guide.md"), "/repo/docs");
});
