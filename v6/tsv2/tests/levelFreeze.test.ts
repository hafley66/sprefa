/**
 * levelFreeze.test.ts — the COUNT/PLAN receipt for the mid-tick level freeze
 * (TICK PHASE ALIGNMENT arc; repo law "formerly-quadratic paths get COUNT
 * tests ... never end-state equality alone").
 *
 * engine.pl:tick/7 freezes `MidLevel = level_closure(store AFTER arrivals)`
 * and hands it to process_occurrences as the level plane an edge body reads.
 * The emitted runtime used to grow that plane only, so a level row an arrival
 * retracted this tick was still in its table when a NON-TRIGGER atom joined
 * it. `IncrementalRuntime.recomputeLevelsBeforeEdges` runs the retracting half
 * at the same point.
 *
 * FAIL-FIRST RECEIPT, taken before the runtime change with the compiler's
 * `edge_body_joins_arrival_fed_level` unsupported construct switched off, on
 * check_eventing.pl:clock_rel_join_storms, BOTH emitter modes, tick 3:
 *   actual  "diag_seen":{"add":[["a_rs",3,..],["a_rs",5,..],["a_rs",7,..]]}
 *   oracle  "diag_seen":{"add":[["a_rs",5,..]]}
 * and tick 7 likewise 3 rows vs 0. After the change: tick log AND final state
 * byte-identical in both modes (the sweep is the standing gate).
 *
 * The correctness half is graded there, byte for byte. What that grading
 * CANNOT see is the cost: a pass that reconciled every level statement on
 * every tick would produce the identical tick log while turning every drain
 * tick into a full refCount reseed of every level rel. So the three
 * assertions here are counts of executed statements against the REAL emitted
 * plan (read out of gen_emitted/, never hand-written in this file), one per
 * narrowing the method documents:
 *
 *   1  DRAIN TICK (empty arrival batch) executes ZERO statements. The level
 *      tables at tick start are the closure the previous tick's post-edge pass
 *      left; with no arrival nothing has moved them.
 *   2  ARRIVAL TICK WITH NO RETRACTION executes exactly ONE statement, the
 *      shared retraction guard. Nothing left, so nothing can have stopped
 *      being derivable (this program has no negated level body:
 *      reconcileEveryTick is false).
 *   3  ARRIVAL TICK WITH A STAGED RETRACTION executes the guard plus this
 *      program's one plain level statement's five supportSql statements.
 *
 * Plus the PLAN of the one statement this arc adds to the per-tick path: the
 * guard must SEARCH each delta table by its `_sign` index, never scan it. The
 * delta tables are per-tick scratch, but "small right now" is not a plan.
 *
 * No new SQL TEXT is introduced by this arc: the reconcile runs the same
 * `supportSql` array the post-edge pass runs, emitter-owned and unchanged, so
 * the EXPLAIN receipts already in aggregateScope.test.ts / edgeGuard.test.ts
 * still cover it.
 *
 * SABOTAGE RECEIPTS (each edit made, this file run, then reverted; messages
 * quoted are what the run printed):
 *   a. Delete `recomputeLevelsBeforeEdges`'s `if (arrivals.length === 0)
 *      return of(undefined);` -> count 1 RED, 1 of 4 failing: "a drain tick
 *      must not touch the level plane: 1 statements".
 *   b. Replace the guard branch with an unconditional `reconcile()` -> 3 of 4
 *      RED, the first being "an arrival tick with no retraction is the guard
 *      alone: 5 statements".
 *   c. Strip the six `CREATE INDEX "__delta_*_sign"` lines from the fixture's
 *      emitted DDL -> the plan assertion RED, the other three still green:
 *      "the retraction guard must SEARCH by _sign, got: SCAN CONSTANT ROW |
 *      SCALAR SUBQUERY 1 | SCAN __delta_diag_history | ... | SCAN
 *      __delta_tick_rel".
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import { firstValueFrom, type Observable } from "rxjs";
import type { ISqlRunner } from "sprefa-store-engine/src/engine/types.ts";

import { IncrementalRuntime } from "../runtime/1_incremental.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import type { IArrivalBatch, ISqlSeam, SqlStatement } from "../runtime/types.ts";
import { incremental_plan, program } from "../gen_emitted/clock_rel_join_storms.ts";

/** Wraps the store's own `SqlRunner`, counting every statement that crosses
 *  the seam. Nothing is intercepted or rewritten; the count is the receipt. */
function counting_seam(seam: ISqlSeam): { seam: ISqlSeam; statements: string[] } {
  const statements: string[] = [];
  const record = (statement: string | SqlStatement): void => {
    statements.push(typeof statement === "string" ? statement : statement.sql);
  };
  const runner: ISqlRunner = {
    ...seam.runner,
    execute(db, statement) {
      record(statement);
      return seam.runner.execute(db, statement);
    },
    batch(db, batched) {
      for (const statement of batched) record(statement);
      return seam.runner.batch(db, batched);
    },
    executeMultiple(db, sql) {
      for (const statement of sql.split(";\n")) record(statement);
      return seam.runner.executeMultiple(db, sql);
    },
  };
  return { seam: { db: seam.db, runner }, statements };
}

function booted_seam(): Promise<ISqlSeam> {
  const seam = ScratchStore.open(":memory:");
  return firstValueFrom(ScratchStore.boot(seam, program.ddl)).then(() => seam);
}

function freeze(seam: ISqlSeam, arrivals: IArrivalBatch): Observable<void> {
  return IncrementalRuntime.recompute_levels_before_edges(
    seam,
    incremental_plan.levels,
    incremental_plan.relations,
    incremental_plan.reconcile_every_tick,
    arrivals,
  );
}

const ONE_ARRIVAL: IArrivalBatch = [{ rel: "file_line", sign: "add", row: ["a_rs", 3, "eprintln_ban"] }];

test("count: a drain tick pays nothing for the mid-tick level freeze", async () => {
  const base = await booted_seam();
  const { seam, statements } = counting_seam(base);
  await firstValueFrom(freeze(seam, []));
  assert.equal(statements.length, 0, `a drain tick must not touch the level plane: ${statements.length} statements`);
});

test("count: an arrival tick with no retraction is the retraction guard alone", async () => {
  assert.equal(
    incremental_plan.reconcile_every_tick,
    false,
    "this fixture is the guarded case; a negated level body would make it unconditional",
  );
  const base = await booted_seam();
  const { seam, statements } = counting_seam(base);
  await firstValueFrom(freeze(seam, ONE_ARRIVAL));
  assert.equal(
    statements.length,
    1,
    `an arrival tick with no retraction is the guard alone: ${statements.length} statements`,
  );
  assert.match(statements[0]!, /has_retraction/);
});

test("count: a staged retraction reconciles exactly the plain level statements", async () => {
  const plain_levels = incremental_plan.levels.filter((statement) => statement.aggregate_sql === null);
  assert.equal(plain_levels.length, 1, "fixture shape assumed by the expected count below");
  const base = await booted_seam();
  await firstValueFrom(
    base.runner.execute(base.db, {
      sql: `INSERT INTO "__delta_clock_rel_join_storms_file_line" ("_sign", "_sequence", "path", "line", "code") VALUES (-1, 0, ?, ?, ?)`,
      args: ["a_rs", 3n, "eprintln_ban"],
    }),
  );
  const { seam, statements } = counting_seam(base);
  await firstValueFrom(freeze(seam, ONE_ARRIVAL));
  // 1 guard + 10 of the 11 supportSql statements per plain level statement; the
  // next-frontier copy is skipped because this pass asks for the frontier only.
  assert.equal(
    statements.length,
    1 + 10 * plain_levels.length,
    `guard + one refCount reconcile per plain level statement: ${statements.length} statements`,
  );
  assert.equal(
    statements.slice(1).join("\n"),
    plain_levels
      .flatMap((statement) => (statement.support_sql ?? []).filter((_, index) => index !== 9))
      .join("\n"),
    "the reconcile must run the emitter's own supportSql, byte for byte, not runtime-built SQL",
  );
});

test("plan: the retraction guard SEARCHes each delta table by its _sign index", async () => {
  const seam = await booted_seam();
  const counted = counting_seam(seam);
  await firstValueFrom(freeze(counted.seam, ONE_ARRIVAL));
  const guard_sql = counted.statements[0]!;
  const explained = await firstValueFrom(
    seam.runner.execute(seam.db, `EXPLAIN QUERY PLAN ${guard_sql}`),
  );
  const plan = explained.rows.map((row) => String(row.detail)).join(" | ");
  assert.ok(
    !/\b_scan __delta_/.test(plan),
    `the retraction guard must SEARCH by _sign, got: ${plan}`,
  );
  assert.ok(
    /USING COVERING INDEX __delta_\w+_sign|USING INDEX __delta_\w+_sign/.test(plan),
    `the retraction guard must use the _sign index, got: ${plan}`,
  );
});
