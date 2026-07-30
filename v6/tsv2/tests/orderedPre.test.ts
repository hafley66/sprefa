/**
 * orderedPre.test.ts — the ordered-occurrence (`pre`) execution family.
 *
 * The two staging tests below grade ROW CONTENT of `stageOrderedFrontiers`.
 * The two count tests at the bottom grade the SHAPE, which is what this file
 * did not do and what review finding 5 named: the standing law is that a
 * formerly-quadratic or plan-sensitive path gets a COUNT or EXPLAIN assertion
 * and never end-state equality alone, and the one execution family whose shape
 * IS per-row was the one family with neither. Seven files in this package carry
 * EXPLAIN assertions and three carry statement counts; every one of them sits
 * on the incremental family. The law was held exactly where someone had already
 * looked.
 *
 * WHAT THE COUNTS PIN, and why the numbers are the bad ones. The ordered family
 * is `13 + 2n` statements per tick against the incremental family's flat 31,
 * measured here. The growth is not a defect of this lowering that a smaller
 * edit removes: `pre(counter(Name, Total))` reads the value BEFORE this
 * occurrence in the ordered stream, so occurrence k+1's read depends on
 * occurrence k's write. That is a genuine sequential fold, and collapsing it
 * needs a running aggregate expressed in SQL over the arrival order -- a new
 * execution shape, which is exactly what ARCH row `pre_occurrence_loop` already
 * owns. The emitter that would have to change is `v6/prolog/compile/emit_ts.pl`
 * (`:1333`, `:1360-1372`), prolog, and outside this lane.
 *
 * So the curve is PINNED, not fixed. The assertion states the exact slope, so
 * any movement -- a regression to something worse, or the flattening that
 * closes `pre_occurrence_loop` -- shows up as a failure that has to be read and
 * re-stated rather than as silence.
 *
 * DISCRIMINATING RED for a pin, since there is no defect here to fail first
 * against. The count assertion was written claiming the FLAT shape the
 * incremental family has, and run (verbatim, reverted):
 *
 *   ✖ the ordered/pre family costs 13 + 2n statements per tick, against the
 *     incremental family's flat 31
 *     AssertionError [ERR_ASSERTION]: ordered/pre at 1,5,25,100 arrivals
 *     + actual - expected
 *       [
 *     +   15, +   23, +   63, +   213
 *     -   31, -   31, -   31, -   31
 *       ]
 *
 * That is the curve, measured by the assertion itself rather than asserted from
 * the review's table. The second test's red is its own: run before the seed
 * rows were added, `__pre_counter` held 0 and the copy assertion said
 * "the pre table holds a full copy of the relation, not a delta / 0 !== 3".
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import { firstValueFrom } from "rxjs";
import { stmt_counter } from "sprefa-store-engine/src/engine/counter.ts";

import { program as orderedProgram } from "../gen_emitted/batched_increments_both_count.ts";
import { program as incrementalProgram } from "../gen_emitted/comparison_filters_rows.ts";
import { stageOrderedFrontiers } from "../runtime/1_incremental.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import type { IArrivalBatch, IIncrementalRelationPlan, IServedProgram } from "../runtime/types.ts";

/** One tick of one emitted program on a fresh `:memory:` db, counting the SQL
 *  statements it executed. The counter is reset after boot so the DDL is not in
 *  the number. */
async function statementsForOneTick(program: IServedProgram, arrivals: IArrivalBatch): Promise<number> {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, program.ddl));
  stmt_counter.reset();
  await firstValueFrom(program.tick(seam, arrivals));
  const executed = stmt_counter.get();
  seam.db.close();
  return executed;
}

function orderedArrivals(count: number): IArrivalBatch {
  return Array.from({ length: count }, (_value, index) => ({
    rel: "increment",
    sign: "add" as const,
    row: [`name_${index}`, "x"],
  }));
}

function incrementalArrivals(count: number): IArrivalBatch {
  return Array.from({ length: count }, (_value, index) => ({
    rel: "callee_set_size",
    sign: "add" as const,
    row: [`left_${index}`, index],
  }));
}

test("the ordered/pre family costs 13 + 2n statements per tick, against the incremental family's flat 31", async () => {
  const sizes = [1, 5, 25, 100];
  const ordered = [];
  const incremental = [];
  for (const size of sizes) {
    ordered.push(await statementsForOneTick(orderedProgram as unknown as IServedProgram, orderedArrivals(size)));
    incremental.push(
      await statementsForOneTick(incrementalProgram as unknown as IServedProgram, incrementalArrivals(size)),
    );
  }

  // The exact curve, stated so that ANY movement in it is a failure someone has
  // to read. Flattening this is ARCH row `pre_occurrence_loop` and lives in the
  // prolog emitter; when it lands, this line is what it edits.
  assert.deepEqual(ordered, sizes.map((size) => 13 + 2 * size), `ordered/pre at ${sizes.join(",")} arrivals`);

  // The comparison that makes the number mean something: the same seam, the
  // same counter, a program on the incremental family, flat.
  assert.deepEqual(incremental, sizes.map(() => 31), `incremental at ${sizes.join(",")} arrivals`);

  // And the claim in one line, independent of the constants above: the ordered
  // family's cost is a function of arrival count, the incremental family's is
  // not.
  assert.ok(
    ordered[3]! - ordered[0]! === 2 * (sizes[3]! - sizes[0]!) && incremental[3]! === incremental[0]!,
    `ordered ${ordered.join(",")} / incremental ${incremental.join(",")}`,
  );
});

test("the ordered/pre snapshot copies the whole relation every tick, arrivals or not", async () => {
  // `snapshotOrderedPre` is emitted as an unconditional
  // `DELETE FROM __pre_x; INSERT INTO __pre_x SELECT * FROM x` before the
  // occurrence loop. Its cost is a function of the RELATION, not of the batch,
  // so a statement count cannot see it and an end-state test cannot either.
  // Pinned two ways: the copy runs on an EMPTY batch, and its read is a SCAN.
  const seam = ScratchStore.open(":memory:");
  const program = orderedProgram as unknown as IServedProgram;
  await firstValueFrom(ScratchStore.boot(seam, program.ddl));
  // `counter` is a rule head, not an arrival target, and its own rule needs a
  // `pre` row to exist before it derives anything. Seeded directly, because
  // what is being measured is the copy, not how the rows got there.
  await firstValueFrom(
    seam.runner.execute(seam.db, `INSERT INTO "counter" ("name", "next") VALUES ('a', 1), ('b', 2), ('c', 3)`),
  );

  stmt_counter.reset();
  await firstValueFrom(program.tick(seam, []));
  assert.equal(stmt_counter.get(), 13, "the constant term runs in full on a tick with no arrivals");

  const counterRows = await firstValueFrom(seam.runner.execute(seam.db, 'SELECT count(*) AS n FROM "__pre_counter"'));
  assert.equal(counterRows.rows[0]!.n, 3, "the pre table holds a full copy of the relation, not a delta");

  const plan = await firstValueFrom(
    seam.runner.execute(seam.db, 'EXPLAIN QUERY PLAN INSERT INTO "__pre_counter" ("name", "next") SELECT "name", "next" FROM "counter"'),
  );
  const detail = plan.rows.map((row) => String(row.detail)).join(" | ");
  assert.match(detail, /SCAN counter/, `the snapshot reads the whole relation: ${detail}`);
  seam.db.close();
});

test("ordered frontier staging carries only the supplied boundary additions", async () => {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(
    ScratchStore.boot(seam, [
      'CREATE TEMP TABLE "__frontier_counter" ("_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, "name" TEXT NOT NULL, "next" INTEGER NOT NULL)',
      'CREATE TEMP TABLE "__next_frontier_counter" ("_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, "name" TEXT NOT NULL, "next" INTEGER NOT NULL)',
    ]),
  );
  await firstValueFrom(
    seam.runner.executeMultiple(
      seam.db,
      'INSERT INTO "__frontier_counter" VALUES (0, 0, \'stale\', 1);\nINSERT INTO "__next_frontier_counter" VALUES (0, 0, \'also_stale\', 2)',
    ),
  );
  const relation: IIncrementalRelationPlan = {
    rel: "counter",
    kind: "set",
    tableName: "counter",
    deltaTableName: "__delta_counter",
    frontierTableName: "__frontier_counter",
    nextFrontierTableName: "__next_frontier_counter",
    columns: ["name", "next"],
    keyIndices: [0],
    arrivalAddSql: null,
    arrivalDelSql: null,
    boundarySql: "",
  };

  const carryPending = await firstValueFrom(
    stageOrderedFrontiers(
      seam,
      [relation],
      [{ rel: "counter", add: [["clicks", 2]], del: [] }],
    ),
  );
  const current = await firstValueFrom(
    seam.runner.execute(
      seam.db,
      'SELECT "_phase", "_sequence", "name", "next" FROM "__frontier_counter"',
    ),
  );
  const next = await firstValueFrom(
    seam.runner.execute(seam.db, 'SELECT * FROM "__next_frontier_counter"'),
  );

  assert.deepEqual(
    { carryPending, current: current.rows, next: next.rows },
    {
      carryPending: true,
      current: [{ _phase: 0, _sequence: 0, name: "clicks", next: 2 }],
      next: [],
    },
  );
});

test("ordered frontier staging retains sequence across relation groups", async () => {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(
    ScratchStore.boot(seam, [
      'CREATE TEMP TABLE "__frontier_left" ("_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, "value" TEXT NOT NULL)',
      'CREATE TEMP TABLE "__next_frontier_left" ("_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, "value" TEXT NOT NULL)',
      'CREATE TEMP TABLE "__frontier_right" ("_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, "value" TEXT NOT NULL)',
      'CREATE TEMP TABLE "__next_frontier_right" ("_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, "value" TEXT NOT NULL)',
    ]),
  );
  const relation = (rel: string): IIncrementalRelationPlan => ({
    rel,
    kind: "log",
    tableName: rel,
    deltaTableName: `__delta_${rel}`,
    frontierTableName: `__frontier_${rel}`,
    nextFrontierTableName: `__next_frontier_${rel}`,
    columns: ["value"],
    keyIndices: [],
    arrivalAddSql: null,
    arrivalDelSql: null,
    boundarySql: "",
  });

  await firstValueFrom(
    stageOrderedFrontiers(
      seam,
      [relation("left"), relation("right")],
      [
        { rel: "left", add: [["a"]], del: [] },
        { rel: "right", add: [["b"]], del: [] },
        { rel: "left", add: [["c"]], del: [] },
      ],
    ),
  );
  const left = await firstValueFrom(
    seam.runner.execute(
      seam.db,
      'SELECT "_sequence", "value" FROM "__frontier_left" ORDER BY "_sequence"',
    ),
  );
  const right = await firstValueFrom(
    seam.runner.execute(
      seam.db,
      'SELECT "_sequence", "value" FROM "__frontier_right" ORDER BY "_sequence"',
    ),
  );

  assert.deepEqual(
    { left: left.rows, right: right.rows },
    {
      left: [
        { _sequence: 0, value: "a" },
        { _sequence: 2, value: "c" },
      ],
      right: [{ _sequence: 1, value: "b" }],
    },
  );
});
