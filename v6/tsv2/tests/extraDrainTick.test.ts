/**
 * extraDrainTick.test.ts — the ARCH `extra_drain_tick` defect pinned
 * (flagship fixture finding, tick-phase-alignment inheritance).
 *
 * A schedule ENDING on a retraction tick whose refCount reconcile
 * (`recomputeLevelsAfterEdges`, supportSql branch) deletes and then
 * re-derives level rows mints one drain tick the oracle lacks, with empty
 * deltas: the reconcile's re-INSERT events were staged into
 * `__next_frontier_*` phase 1, `promoteFrontiers` reads any next-frontier
 * row as carry, and the drained tick derives nothing because the rows it
 * carries were never new (net zero at the boundary, so the oracle logged
 * no occurrence either).
 *
 * The aggregate branch of the same reconcile was already fixed
 * (`afterEdges=false`, its comment block holds the measured receipt); the
 * refCount branch kept the P3 staging shape because "no corpus fixture
 * distinguishes the two". The flagship callgraph fixture distinguishes
 * them: truncate its schedule to end on the `del call(b.rs, main)` tick.
 *
 * FAIL-FIRST RECEIPT (this file run before the fix, staging still
 * `[{nextFrontierTableName, phase: 1}]`):
 *   truncated 4-tick schedule -> 5 tick lines, line 5 = {"tick":5,"deltas":{}}
 * After the fix (reconcile stages no frontier copies, matching the
 * aggregate branch): 4 lines, and the full-schedule run stays
 * byte-identical to the checked-in oracle log (the guard that the fix
 * did not eat a LEGIT carry).
 */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { concatMap, firstValueFrom, toArray } from "rxjs";

import { BootRunner } from "../runtime/2_boot.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import { TickFold } from "../runtime/tickLoop.ts";
import type { IArrivalBatch, IBootStatement, IGenProgram } from "../runtime/types.ts";
import { program } from "../gen_emitted/callgraph_unused_inverts_with_the_call_set.ts";

const HERE = dirname(fileURLToPath(import.meta.url));
const COMPILE_OUT = join(HERE, "..", "..", "prolog", "compile", "out");
const FIXTURE = "callgraph_unused_inverts_with_the_call_set";

type EmittedProgram = IGenProgram & { readonly boot: readonly IBootStatement[] };

function readSchedule(): readonly IArrivalBatch[] {
  const text = readFileSync(join(COMPILE_OUT, `${FIXTURE}.schedule.json`), "utf8");
  return JSON.parse(text) as readonly IArrivalBatch[];
}

function runTicks(schedule: readonly IArrivalBatch[]): Promise<readonly string[]> {
  const emitted = program as EmittedProgram;
  const seam = ScratchStore.open(":memory:");
  return firstValueFrom(
    ScratchStore.boot(seam, emitted.ddl).pipe(
      concatMap(() => BootRunner.run(seam, emitted.boot)),
      concatMap(() => TickFold.run(emitted, seam, schedule).pipe(toArray())),
    ),
  );
}

test("a schedule ending on a retraction tick mints no extra empty drain tick", async () => {
  const truncated = readSchedule().slice(0, 4);
  const lines = await runTicks(truncated);
  assert.equal(
    lines.length,
    truncated.length,
    `retraction-final schedule must end on its own tick, got an extra line: ${lines[lines.length - 1]}`,
  );
});

test("the full schedule still matches the checked-in oracle log line for line", async () => {
  const oracle = readFileSync(join(COMPILE_OUT, `${FIXTURE}.oracle.jsonl`), "utf8")
    .split("\n")
    .filter((line) => line.length > 0);
  const lines = await runTicks(readSchedule());
  assert.deepEqual([...lines], oracle);
});
