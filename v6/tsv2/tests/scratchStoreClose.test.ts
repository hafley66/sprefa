/**
 * scratchStoreClose.test.ts — ScratchStore.close releases the native SQLite
 * handle, counted at the OS rather than inferred from a flag.
 *
 * THE COUNTER. `/dev/fd` (a `/proc/self/fd` alias on Linux) lists this
 * process's open file descriptors, so one file-backed libsql connection is one
 * countable entry. `:memory:` opens no descriptor, which is why the handle
 * assertions run on `file:` urls and the corpus-sized soak only reports.
 *
 * WHY THE SEAMS STAY REACHABLE. An unreachable client's handle is reclaimed by
 * the napi finalizer whether or not anyone closed it, so a test that drops its
 * references measures the collector, not the seam. Every seam here is held in
 * an array for the whole test, which is the shape a long-lived process has.
 *
 * WHY A FORCED COLLECTION. `Client.close()` is sqlite3_close_v2 underneath: it
 * marks the connection closed at once and hands the descriptor back only after
 * the statement wrappers the driver prepared are finalized, one collection and
 * one tick later. Measured on this machine, 32 file seams held reachable:
 *   closed:     baseline 13, open 51, after collect+tick 13  (all 32 back)
 *   not closed: baseline 13, open 51, after collect+tick 51  (none back)
 * `holds_every_handle` below is that second row as a live control, so the first
 * assertion cannot pass vacuously.
 *
 * FAIL-FIRST RECEIPT. Delete the `ScratchStore.close(seam)` body in
 * runtime/scratchStore.ts and `releases_every_handle` fails at +64 descriptors,
 * which is what a replay process carried at 335 fixtures and a shard child at
 * 168.
 */

import assert from "node:assert/strict";
import { mkdtempSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { setFlagsFromString } from "node:v8";
import { runInNewContext } from "node:vm";
import { firstValueFrom } from "rxjs";

import { ScratchStore } from "../runtime/scratchStore.ts";
import type { ISqlSeam } from "../runtime/types.ts";

const SEAM_COUNT = 64;
const CONTROL_COUNT = 32;
const SOAK_ROUNDS = 335;
/** Descriptors the measurement itself churns (the temp directory, the runner's
 *  own reads). Measured at 6 over baseline on a clean pass; a missed close is
 *  +64 and a missed control is +32, so the two are nowhere near each other. */
const SLACK = 8;
const DDL: readonly string[] = ['CREATE TABLE "probe" ("__id" INTEGER PRIMARY KEY, "n" INTEGER)'];

/** A collection this test can ask for, without putting --expose-gc on the
 *  whole battery's command line. */
const collect: () => void = (() => {
  setFlagsFromString("--expose-gc");
  const exposed = runInNewContext("gc") as () => void;
  setFlagsFromString("--no-expose-gc");
  return exposed;
})();

function open_descriptors(): number {
  for (const path of ["/dev/fd", "/proc/self/fd"]) {
    try {
      return readdirSync(path).length;
    } catch {
      continue;
    }
  }
  throw new Error("no descriptor directory on this platform");
}

/** Collect, then yield the loop: the napi finalizer that hands the descriptor
 *  back runs on a later tick, never inside gc(). */
async function reclaim(): Promise<void> {
  collect();
  await new Promise((resolve) => setTimeout(resolve, 100));
}

async function open_seams(directory: string, count: number, prefix: string): Promise<ISqlSeam[]> {
  const seams: ISqlSeam[] = [];
  for (let index = 0; index < count; index += 1) {
    const seam = ScratchStore.open(`file:${join(directory, `${prefix}${index}.sqlite`)}`);
    await firstValueFrom(ScratchStore.boot(seam, DDL));
    seams.push(seam);
  }
  return seams;
}

test("releases_every_handle: 64 reachable file seams return the descriptor count to baseline", async () => {
  const directory = mkdtempSync(join(tmpdir(), "scratch-close-"));
  const seams: ISqlSeam[] = [];
  try {
    await reclaim();
    const baseline = open_descriptors();
    seams.push(...await open_seams(directory, SEAM_COUNT, "probe"));
    const while_open = open_descriptors();
    assert.ok(
      while_open - baseline >= SEAM_COUNT / 2,
      `the counter must see the handles it is asked to prove released: baseline=${baseline} open=${while_open}`,
    );

    for (const seam of seams) ScratchStore.close(seam);
    await reclaim();
    const after_close = open_descriptors();
    assert.ok(
      after_close - baseline <= SLACK,
      `descriptors after ${SEAM_COUNT} close calls: baseline=${baseline} open=${while_open} closed=${after_close}`,
    );
    assert.equal(seams.length, SEAM_COUNT, "every seam stayed reachable across the measurement");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("holds_every_handle: the same seams left unclosed keep their descriptors through a collection", async () => {
  const directory = mkdtempSync(join(tmpdir(), "scratch-hold-"));
  const seams: ISqlSeam[] = [];
  try {
    await reclaim();
    const baseline = open_descriptors();
    seams.push(...await open_seams(directory, CONTROL_COUNT, "held"));
    await reclaim();
    const still_open = open_descriptors();
    assert.ok(
      still_open - baseline >= CONTROL_COUNT / 2,
      `a reachable unclosed seam must keep its handle, or releases_every_handle proves nothing: ` +
        `baseline=${baseline} held=${still_open}`,
    );
  } finally {
    for (const seam of seams) ScratchStore.close(seam);
    rmSync(directory, { recursive: true, force: true });
  }
});

test("close is idempotent, so a finalize firing on the complete and the error leg is safe", () => {
  const seam = ScratchStore.open(":memory:");
  ScratchStore.close(seam);
  ScratchStore.close(seam);
});

test(`soak: ${SOAK_ROUNDS} open/boot/close rounds, the corpus one replay process carries`, async () => {
  await reclaim();
  const baseline = open_descriptors();
  const rss_before = process.memoryUsage().rss;
  for (let index = 0; index < SOAK_ROUNDS; index += 1) {
    const seam = ScratchStore.open(":memory:");
    await firstValueFrom(ScratchStore.boot(seam, DDL));
    ScratchStore.close(seam);
  }
  await reclaim();
  const after = open_descriptors();
  const rss_after = process.memoryUsage().rss;
  process.stdout.write(
    `SCRATCH_SOAK rounds=${SOAK_ROUNDS} fd_baseline=${baseline} fd_after=${after} ` +
      `rss_before_mb=${(rss_before / 1048576).toFixed(1)} rss_after_mb=${(rss_after / 1048576).toFixed(1)}\n`,
  );
  assert.ok(after - baseline <= SLACK, `descriptors must not grow across ${SOAK_ROUNDS} rounds: ${baseline} -> ${after}`);
});
