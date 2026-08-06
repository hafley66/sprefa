/**
 * recursiveClosureCounts.test.ts — the COUNT receipt for in-tick closure of a
 * recursive head (repo law: "formerly-quadratic paths get COUNT tests ...
 * never end-state equality alone").
 *
 * A recursive rule whose arrivals batch has to reach full closure inside one
 * tick. HOW it reaches closure is what end-state equality cannot see: a loop
 * that re-runs the emitted delta insert per round answers correctly and grades
 * IDENTICAL in the sweep while paying rounds x |head|, because the emitted
 * insert reads "__frontier_<rel>" under "_phase" >= 0 and nothing retires the
 * frontier between rounds.
 *
 * The receipt is statements per tick against closure DEPTH. A one-pass closure
 * runs a fixed statement count at every depth; a per-round loop runs one insert
 * plus its staging per round, so its count grows with the chain length.
 *
 * FAIL-PRE-FIX RECEIPT, taken by restoring the round loop from the parent
 * commit and re-running this file: "closure must not pay per round: depth 3 ran
 * 39 statements, depth 8 ran 59, depth 16 ran 91", actual [39, 59, 91] against
 * the pinned 33. The growth is 4 statements per extra round.
 *
 * The chain is fed as ONE arrival batch of consecutive same-rel rows, which the
 * arrival plane batches into one insert, so the count this file measures is the
 * rule plane alone (the coalesceCounts.test.ts arrivals/1 header records why
 * interleaving rels would measure run-length batching instead).
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import { firstValueFrom } from "rxjs";
import { stmt_counter } from "sprefa-store-engine/src/engine/counter.ts";

import { program } from "../gen_emitted/flagship_flow_reach_over_batched_resolved_edges.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import type { IArrivalRow } from "../runtime/types.ts";

/** A path of `depth` edges: node_0 -> node_1 -> ... -> node_depth, so the
 *  closure is depth*(depth+1)/2 rows and needs `depth` hops to settle. */
function chainArrivals(depth: number): readonly IArrivalRow[] {
  const rows: IArrivalRow[] = [];
  for (let index = 0; index < depth; index += 1) {
    rows.push({
      rel: "resolved_call_edge",
      sign: "add",
      row: [`node_${index}`, "fn", `node_${index + 1}`, "fn"],
    });
  }
  return rows;
}

/** Pinned rather than only compared: an equality assertion alone would still
 *  hold if every depth grew together. Measured at depths 3, 8 and 16. */
const STATEMENTS_PER_TICK = 34;

async function runOneTick(depth: number) {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, program.ddl));
  stmt_counter.reset();
  await firstValueFrom(program.tick(seam, chainArrivals(depth)));
  const statementCount = stmt_counter.get();
  const final = await firstValueFrom(
    seam.runner.execute(seam.db, program.finalSelect.flow_reach!),
  );
  seam.db.close();
  return { statementCount, reachRows: final.rows.length };
}

test("in-tick closure statements are flat in the recursion depth", async () => {
  const shallow = await runOneTick(3);
  const middle = await runOneTick(8);
  const deep = await runOneTick(16);

  assert.deepEqual(
    [shallow.statementCount, middle.statementCount, deep.statementCount],
    [STATEMENTS_PER_TICK, STATEMENTS_PER_TICK, STATEMENTS_PER_TICK],
    `closure must not pay per round: depth 3 ran ${shallow.statementCount} statements, depth 8 ran ${middle.statementCount}, depth 16 ran ${deep.statementCount}`,
  );
  // Non-vacuity: the transitive closure of a path of n edges is n(n+1)/2 rows,
  // so a flat count over these numbers is flat over real derivation.
  assert.deepEqual(
    [shallow.reachRows, middle.reachRows, deep.reachRows],
    [6, 36, 136],
    "every depth must reach full closure inside the one tick",
  );
});
