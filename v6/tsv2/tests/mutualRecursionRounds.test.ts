/**
 * mutualRecursionRounds.test.ts — the COUNT receipt for the outer-round loop
 * (repo law: "formerly-quadratic paths get COUNT tests ... never end-state
 * equality alone").
 *
 * `path` and `reach` sit on one stratum cycle and neither reads ITSELF, so
 * neither gets an `expand_sql` wavefront: the least fixpoint only arrives by
 * re-running the whole group's statement pass. What end-state equality cannot
 * see is WHICH statements pay for it -- a loop that re-ran every level
 * statement, cycle or not, would answer identically while charging the acyclic
 * ones a round they do not need.
 *
 * Two axes, both pinned exactly:
 *   1. the cycle grows with rounds, and the rounds track the chain depth;
 *   2. an ACYCLIC program names zero recursion groups, which is what keeps its
 *      statement count identical to the one it paid before this loop existed
 *      (receipt: only one emitted module in the whole corpus changed text when
 *      the field landed, `out/mutual_recursion_matches_oracle.ts`).
 *
 * FAIL-PRE-FIX RECEIPT (this file run with runtime/1_incremental.ts checked
 * out at f11eb079 over it, emitted modules unchanged): the cycle test failed
 * with `path` [2, 4, 8] against the closure's [3, 10, 36], one hop per tick
 * and no settling; the acyclic test PASSED with the identical pin of 61, which
 * is the before/after receipt that no acyclic statement gained a round.
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import { firstValueFrom } from "rxjs";
import { stmt_counter } from "sprefa-store-engine/src/engine/counter.ts";

import {
  incremental_plan as acyclic_plan,
  program as acyclic_program,
} from "../gen_emitted/switch_as_keyed_replace.ts";
import { program as cycle_program } from "../gen_emitted/mutual_closure_needs_outer_rounds.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import type { IArrivalRow } from "../runtime/types.ts";

/** A path of `depth` edges: 0 -> 1 -> ... -> depth. Its transitive closure is
 *  depth*(depth+1)/2 rows and takes `depth` alternations of the two heads. */
function chain_arrivals(depth: number): readonly IArrivalRow[] {
  const rows: IArrivalRow[] = [];
  for (let index = 0; index < depth; index += 1) {
    rows.push({ rel: "edge", sign: "add", row: [index, index + 1] });
  }
  return rows;
}

async function run_cycle_tick(depth: number) {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, cycle_program.ddl));
  stmt_counter.reset();
  await firstValueFrom(cycle_program.tick(seam, chain_arrivals(depth)));
  const statement_count = stmt_counter.get();
  const path = await firstValueFrom(
    seam.runner.execute(seam.db, cycle_program.final_select.path!),
  );
  const reach = await firstValueFrom(
    seam.runner.execute(seam.db, cycle_program.final_select.reach!),
  );
  seam.db.close();
  return { statement_count, path_rows: path.rows.length, reach_rows: reach.rows.length };
}

test("a mutual cycle closes in-tick and pays rounds, not passes over the head", async () => {
  const shallow = await run_cycle_tick(2);
  const middle = await run_cycle_tick(4);
  const deep = await run_cycle_tick(8);

  // Non-vacuity first: the closure of a chain of n edges is n(n+1)/2 rows and
  // `reach` is that minus the n direct edges. One pass would give n and 0.
  assert.deepEqual(
    [shallow.path_rows, middle.path_rows, deep.path_rows],
    [3, 10, 36],
    "every depth must reach full closure inside the one tick",
  );
  assert.deepEqual(
    [shallow.reach_rows, middle.reach_rows, deep.reach_rows],
    [1, 6, 28],
    "the second head of the cycle must close too",
  );
  assert.deepEqual(
    [shallow.statement_count, middle.statement_count, deep.statement_count],
    [CYCLE_STATEMENTS_AT_DEPTH[0], CYCLE_STATEMENTS_AT_DEPTH[1], CYCLE_STATEMENTS_AT_DEPTH[2]],
    `the group pays ${CYCLE_STATEMENTS_PER_ROUND} statements per round: depth 2 ran ${shallow.statement_count}, depth 4 ran ${middle.statement_count}, depth 8 ran ${deep.statement_count}`,
  );
});

/** Pinned, not derived: an affine formula would still hold if the flat term
 *  absorbed a per-round cost. Measured at depths 2, 4, 8. */
const CYCLE_STATEMENTS_PER_ROUND = 20;
const CYCLE_STATEMENTS_AT_DEPTH = [83, 123, 203] as const;

/** The acyclic control, pinned on ONE tick of a program whose level rules
 *  form a DAG (`demanded` then `route_view`). */
const ACYCLIC_STATEMENTS_PER_TICK = 61;

test("an acyclic program names no recursion group and runs one pass", async () => {
  const grouped = acyclic_plan.levels.filter(
    (statement) => (statement.recursion_group ?? null) !== null,
  );
  assert.deepEqual(grouped, [], "no level head of a DAG program sits on a cycle");

  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, acyclic_program.ddl));
  await firstValueFrom(
    acyclic_program.tick(seam, [
      { rel: "route_row", sign: "add", row: ["settings", "body_settings"] },
    ]),
  );
  stmt_counter.reset();
  await firstValueFrom(
    acyclic_program.tick(seam, [
      { rel: "route_change", sign: "add", row: ["session_a", "settings"] },
    ]),
  );
  const statement_count = stmt_counter.get();
  seam.db.close();

  assert.equal(
    statement_count,
    ACYCLIC_STATEMENTS_PER_TICK,
    "a program off every cycle pays the same statements the single-pass loop paid",
  );
});
