import assert from "node:assert/strict";
import { test } from "node:test";

import { firstValueFrom } from "rxjs";

import { stageOrderedFrontiers } from "../runtime/1_incremental.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import type { IIncrementalRelationPlan } from "../runtime/types.ts";

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
