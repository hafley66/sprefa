import assert from "node:assert/strict";
import { test } from "node:test";

import { firstValueFrom } from "rxjs";

import { IncrementalRuntime } from "../runtime/1_incremental.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import type { IArrivalBatch, IIncrementalRelationPlan } from "../runtime/types.ts";

const PLAN: IIncrementalRelationPlan = {
  rel: "probe_in",
  kind: "set",
  tableName: "probe_in",
  deltaTableName: "__delta_probe_in",
  frontierTableName: "__frontier_probe_in",
  nextFrontierTableName: "__next_frontier_probe_in",
  columns: ["value"],
  columnTypes: ["text"],
  keyIndices: [],
  arrivalAddSql: `INSERT INTO "probe_in" ("value") SELECT json_extract(value, '$[0]') FROM json_each(?) RETURNING "value"`,
  arrivalDelSql: `DELETE FROM "probe_in" WHERE "value" IN (SELECT json_extract(value, '$[0]') FROM json_each(?)) RETURNING "value"`,
  boundarySql: `SELECT "value", "_sign" AS "__sign", count(*) AS "__count" FROM "__delta_probe_in" GROUP BY "value", "_sign"`,
};

const DDL = [
  `CREATE TABLE "probe_in" ("value" TEXT NOT NULL)`,
  `CREATE TEMP TABLE "__delta_probe_in" ("_sign" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, "value" TEXT NOT NULL)`,
  `CREATE TEMP TABLE "__frontier_probe_in" ("_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, "value" TEXT NOT NULL)`,
  `CREATE TEMP TABLE "__next_frontier_probe_in" ("_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, "value" TEXT NOT NULL)`,
] as const;

test("arrival delta carries the text value returned by SQLite", async () => {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, DDL));

  const arrivals: IArrivalBatch = [{ rel: "probe_in", sign: "add", row: [4] }];
  await firstValueFrom(IncrementalRuntime.applyArrivals(seam, arrivals, [PLAN]));

  const result = await firstValueFrom(
    seam.runner.execute(seam.db, `SELECT "_sign", "_sequence", "value" FROM "__delta_probe_in"`),
  );
  assert.deepEqual(result.rows, [{ _sign: 1, _sequence: 0, value: "4" }]);
});
