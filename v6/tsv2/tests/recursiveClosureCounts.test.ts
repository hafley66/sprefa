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
 * The receipt is statements per tick against closure DEPTH, pinned EXACTLY.
 * The in-place assert walk (IDredPlan) pays 4 statements per round -- clear
 * idle wave, hop, commit the wave into the head, append it to __new_<rel> --
 * and every one touches only wavefront rows, so the pinned shape is affine:
 * STATEMENTS_FLAT + 4 * (depth + 1). The defect class this rail exists for
 * pays per round AND re-reads the whole frontier each round; any change to the
 * per-round shape moves these exact numbers and fails the pin.
 *
 * FAIL-PRE-FIX RECEIPT (naive loop, restored from e3997cec's parent and rerun):
 * [39, 59, 91], slope 4 per round with each round re-reading everything. The
 * one-pass CTE era pinned [36, 36, 36]; the expand fill into __support_next
 * pinned [48, 63, 87]; in-place head maintenance pins [48, 68, 100] (four
 * statements per round, and a flat term four smaller because the tail no
 * longer updates, diffs and re-inserts the whole head).
 *
 * The SECOND test is the retraction slope, and it is the reason DRed exists:
 * rows touched on a delete tick must follow the CONE, never |head|. Bulk chain
 * depth 8/16/32 puts 42/142/534 rows in the head while the cone stays 3.
 * FAIL-PRE-FIX RECEIPT (the shipped full-recompute path at 14c03007, this file
 * checked out over it and rerun 2026-08-06): [105, 153, 249] -- the reseed
 * walks the WHOLE closure, so its round count is the bulk chain's depth. In
 * place it is [111, 111, 111].
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
function chain_arrivals(depth: number): readonly IArrivalRow[] {
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
const STATEMENTS_FLAT = 34;
const STATEMENTS_PER_ROUND = 4;
const statements_at_depth = (depth: number): number =>
  STATEMENTS_FLAT + STATEMENTS_PER_ROUND * (depth + 1);

/** Two DISJOINT chains: `small_*` is the one a delete tick cuts, `bulk_*` is
 *  everything the cone must not touch. */
function two_chain_arrivals(bulk_depth: number): readonly IArrivalRow[] {
  const rows: IArrivalRow[] = [];
  for (let index = 0; index < CONE_CHAIN_DEPTH; index += 1) {
    rows.push({
      rel: "resolved_call_edge",
      sign: "add",
      row: [`small_${index}`, "fn", `small_${index + 1}`, "fn"],
    });
  }
  for (let index = 0; index < bulk_depth; index += 1) {
    rows.push({
      rel: "resolved_call_edge",
      sign: "add",
      row: [`bulk_${index}`, "fn", `bulk_${index + 1}`, "fn"],
    });
  }
  return rows;
}

const CONE_CHAIN_DEPTH = 3;
const STATEMENTS_PER_DELETE_TICK = 113;

async function run_one_tick(depth: number) {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, program.ddl));
  stmt_counter.reset();
  await firstValueFrom(program.tick(seam, chain_arrivals(depth)));
  const statement_count = stmt_counter.get();
  const final = await firstValueFrom(
    seam.runner.execute(seam.db, program.final_select.flow_reach!),
  );
  seam.db.close();
  return { statement_count, reach_rows: final.rows.length };
}

test("in-tick closure statements are flat in the recursion depth", async () => {
  const shallow = await run_one_tick(3);
  const middle = await run_one_tick(8);
  const deep = await run_one_tick(16);

  assert.deepEqual(
    [shallow.statement_count, middle.statement_count, deep.statement_count],
    [statements_at_depth(3), statements_at_depth(8), statements_at_depth(16)],
    `closure pays exactly ${STATEMENTS_PER_ROUND} wavefront statements per round: depth 3 ran ${shallow.statement_count} statements, depth 8 ran ${middle.statement_count}, depth 16 ran ${deep.statement_count}`,
  );
  // Non-vacuity: the transitive closure of a path of n edges is n(n+1)/2 rows,
  // so a flat count over these numbers is flat over real derivation.
  assert.deepEqual(
    [shallow.reach_rows, middle.reach_rows, deep.reach_rows],
    [6, 36, 136],
    "every depth must reach full closure inside the one tick",
  );
});

async function run_delete_tick(bulk_depth: number) {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, program.ddl));
  await firstValueFrom(program.tick(seam, two_chain_arrivals(bulk_depth)));
  const before = await firstValueFrom(
    seam.runner.execute(seam.db, program.final_select.flow_reach!),
  );
  stmt_counter.reset();
  await firstValueFrom(
    program.tick(seam, [
      { rel: "resolved_call_edge", sign: "del", row: ["small_0", "fn", "small_1", "fn"] },
    ]),
  );
  const statement_count = stmt_counter.get();
  const after = await firstValueFrom(
    seam.runner.execute(seam.db, program.final_select.flow_reach!),
  );
  seam.db.close();
  return { statement_count, head_before: before.rows.length, head_after: after.rows.length };
}

test("a delete tick pays the cone, not the head", async () => {
  const small = await run_delete_tick(8);
  const middle = await run_delete_tick(16);
  const large = await run_delete_tick(32);

  assert.deepEqual(
    [small.statement_count, middle.statement_count, large.statement_count],
    [
      STATEMENTS_PER_DELETE_TICK,
      STATEMENTS_PER_DELETE_TICK,
      STATEMENTS_PER_DELETE_TICK,
    ],
    `the cone is ${CONE_CHAIN_DEPTH} rows at every head size: bulk 8 ran ${small.statement_count} statements, bulk 16 ran ${middle.statement_count}, bulk 32 ran ${large.statement_count}`,
  );
  // Non-vacuity on both axes: the head really does grow, and the delete really
  // does retract exactly the cut node's reach.
  assert.deepEqual(
    [small.head_before, middle.head_before, large.head_before],
    [42, 142, 534],
    "the bulk chain must actually make the head grow",
  );
  assert.deepEqual(
    [
      small.head_before - small.head_after,
      middle.head_before - middle.head_after,
      large.head_before - large.head_after,
    ],
    [CONE_CHAIN_DEPTH, CONE_CHAIN_DEPTH, CONE_CHAIN_DEPTH],
    "cutting the small chain's first edge retracts exactly its reach",
  );
});
