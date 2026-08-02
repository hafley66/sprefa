/**
 * watchRealSource.test.ts — the watch stream is INFINITE, on the REAL backend.
 *
 * Every other watch receipt in `node --test` injects `ScriptedWatchSource`, so
 * the one class that names a watcher library (`NodeWatchSource`, node's own
 * `fsPromises.watch`) is exercised only by scripts/extraction-live.sh, which
 * edits three DIFFERENT files once each. Nothing pinned the property that a
 * SINGLE file stays audible: N successive digest-changing saves owe N arrival
 * batches, not one and then silence.
 *
 * Real here means real: real kernel notifications, real `asyncScheduler`
 * coalesce windows, real sha256 over real bytes. Waiting is therefore
 * poll-until-the-count-moves with a ceiling, never a fixed sleep.
 *
 * SABOTAGE RECEIPTS (both reverted):
 *  - `take(1)` on `NodeWatchSource.watch`'s returned observable (the exact
 *    reported failure shape: first change delivered, then deaf) fails
 *    "edit 2 owes its own batch" after the 15s ceiling with 1 live batch.
 *  - dropping `finalize(() => controller.abort())` leaves the test green and
 *    the handle open, which is why leak-soak and not this file is the receipt
 *    for teardown.
 */

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { readFileSync } from "node:fs";

import { sha256 } from "@noble/hashes/sha2.js";
import { bytesToHex } from "@noble/hashes/utils.js";

import type { IServeEvent, IWatchFired } from "../runtime/types.ts";
import { postProgram, request, startServed } from "./serveHelpers.ts";

const WATCH_RAIL_DL6 = fileURLToPath(new URL("../../dl/fixtures/served-watch-rail.dl6", import.meta.url));
const COALESCE_MS = 50;
/** Ceiling on ONE kernel notification plus its coalesce window and tick. Only
 *  a stalled or torn-down watch stream ever reaches it. */
const AUDIBLE_MS = 15_000;
const EDITS = 4;

function firings(events: readonly IServeEvent[]): readonly IWatchFired[] {
  return events.flatMap((event) => (event.kind === "watch" ? [event.fired] : []));
}

function waitUntil(condition: () => boolean, limitMs: number): Promise<boolean> {
  const deadline = Date.now() + limitMs;
  return new Promise((resolve) => {
    const poll = (): void => {
      if (condition()) return resolve(true);
      if (Date.now() > deadline) return resolve(false);
      setTimeout(poll, 25);
    };
    poll();
  });
}

async function seenRows(port: number): Promise<readonly (readonly (string | number)[])[]> {
  const reply = await request(port, "/idb/seen", "GET");
  return (JSON.parse(reply.body) as { rows: readonly (readonly (string | number)[])[] }).rows;
}

test("watch bind: successive saves to ONE file stay audible on the real fs backend", async () => {
  const source = readFileSync(WATCH_RAIL_DL6, "utf8");
  const root = mkdtempSync(join(tmpdir(), "tsv2-realwatch-"));
  mkdirSync(join(root, "src"), { recursive: true });
  writeFileSync(join(root, "src/one.ts"), "export const one = 0;\n");
  // Tracked, so BOOT enumerates the file and seeds `lastDigest`; every later
  // save is then a digest CHANGE (del+add) rather than a first sighting.
  spawnSync("git", ["init", "-q"], { cwd: root });
  spawnSync("git", ["add", "-A"], { cwd: root });

  // No `watchSource` override and no `scheduler` override: this is the
  // production pair, `NodeWatchSource` on `asyncScheduler`.
  const served = await startServed(0, undefined, ":memory:", { watchRoot: root, watchCoalesceMs: COALESCE_MS });
  try {
    const loaded = await postProgram(served.port, source);
    assert.equal(loaded.statusCode, 200, loaded.body);

    const booted = await waitUntil(() => firings(served.events).length >= 1, AUDIBLE_MS);
    assert.ok(booted, "boot reconciliation owes one batch for the tracked file");
    const bootFirings = firings(served.events).length;

    for (let edit = 1; edit <= EDITS; edit += 1) {
      const bytes = `export const one = ${edit};\n`;
      const before = firings(served.events).length;
      writeFileSync(join(root, "src/one.ts"), bytes);

      const heard = await waitUntil(() => firings(served.events).length > before, AUDIBLE_MS);
      assert.ok(heard, `edit ${edit} owes its own batch; the stream went deaf after ${before - bootFirings} live batches`);

      const fired = firings(served.events).at(-1);
      assert.deepEqual(
        { added: fired?.added, removed: fired?.removed },
        { added: 1, removed: 1 },
        `edit ${edit} is one del+add pair`,
      );
      const rows = await seenRows(served.port);
      assert.deepEqual(
        rows.map((row) => [row[0], row[1]]),
        [["src/one.ts", bytesToHex(sha256(new TextEncoder().encode(bytes)))]],
        `edit ${edit} leaves exactly one row, carrying the bytes just written`,
      );
    }

    assert.equal(
      firings(served.events).length - bootFirings,
      EDITS,
      "one batch per digest-changing save, no replays and no swallowed windows",
    );
  } finally {
    await served.stop();
    rmSync(root, { recursive: true, force: true });
  }
});
