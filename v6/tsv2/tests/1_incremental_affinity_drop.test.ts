import assert from "node:assert/strict";
import { test } from "node:test";

import { firstValueFrom } from "rxjs";

import { IncrementalRuntime } from "../runtime/1_incremental.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import type {
  IArrivalBatch,
  IIncrementalEdgeStatement,
  IIncrementalRelationPlan,
} from "../runtime/types.ts";

const PLAN: IIncrementalRelationPlan = {
  rel: "probe_in",
  kind: "set",
  table_name: "probe_in",
  delta_table_name: "__delta_probe_in",
  frontier_table_name: "__frontier_probe_in",
  next_frontier_table_name: "__next_frontier_probe_in",
  columns: ["value"],
  column_types: ["text"],
  key_indices: [],
  arrival_add_sql: `INSERT INTO "probe_in" ("value") SELECT json_extract(value, '$[0]') FROM json_each(?) RETURNING "value"`,
  arrival_del_sql: `DELETE FROM "probe_in" WHERE "value" IN (SELECT json_extract(value, '$[0]') FROM json_each(?)) RETURNING "value"`,
  boundary_sql: `SELECT "value", "_sign" AS "__sign", count(*) AS "__count" FROM "__delta_probe_in" GROUP BY "value", "_sign"`,
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
  await firstValueFrom(IncrementalRuntime.apply_arrivals(seam, arrivals, [PLAN]));

  const result = await firstValueFrom(
    seam.runner.execute(seam.db, `SELECT "_sign", "_sequence", "value" FROM "__delta_probe_in"`),
  );
  assert.deepEqual(result.rows, [{ _sign: 1, _sequence: 0, value: "4" }]);
});

const ZERO_PLAN: IIncrementalRelationPlan = {
  rel: "flag",
  kind: "set",
  table_name: "flag",
  delta_table_name: "__delta_flag",
  frontier_table_name: "__frontier_flag",
  next_frontier_table_name: "__next_frontier_flag",
  columns: [],
  column_types: [],
  key_indices: [],
  arrival_add_sql: `INSERT OR IGNORE INTO "flag" ("__unit") SELECT 1 FROM json_each(?) RETURNING "__unit"`,
  arrival_del_sql: `DELETE FROM "flag" WHERE "__unit" = 1 AND EXISTS (SELECT 1 FROM json_each(?)) RETURNING "__unit"`,
  boundary_sql: `SELECT "__unit", "_sign" AS "__sign", count(*) AS "__count" FROM "__delta_flag" GROUP BY "__unit", "_sign"`,
};

const ZERO_DDL = [
  `CREATE TABLE "flag" ("__id" INTEGER PRIMARY KEY, "__unit" INTEGER NOT NULL DEFAULT 1 CHECK ("__unit" = 1), UNIQUE ("__unit"))`,
  `CREATE TEMP TABLE "__delta_flag" ("_sign" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, "__unit" INTEGER NOT NULL DEFAULT 1 CHECK ("__unit" = 1))`,
  `CREATE TEMP TABLE "__frontier_flag" ("_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, "__unit" INTEGER NOT NULL DEFAULT 1 CHECK ("__unit" = 1))`,
  `CREATE TEMP TABLE "__next_frontier_flag" ("_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, "__unit" INTEGER NOT NULL DEFAULT 1 CHECK ("__unit" = 1))`,
] as const;

test("zero-arity set arrivals add, deduplicate, and delete one unit tuple", async () => {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, ZERO_DDL));

  await firstValueFrom(IncrementalRuntime.apply_arrivals(
    seam,
    [
      { rel: "flag", sign: "add", row: [] },
      { rel: "flag", sign: "add", row: [] },
      { rel: "flag", sign: "del", row: [] },
    ],
    [ZERO_PLAN],
  ));

  const base = await firstValueFrom(seam.runner.execute(seam.db, `SELECT * FROM "flag"`));
  const delta = await firstValueFrom(
    seam.runner.execute(
      seam.db,
      `SELECT "_sign", "_sequence", "__unit" FROM "__delta_flag" ORDER BY "_sign" DESC`,
    ),
  );
  assert.deepEqual({ base: base.rows, delta: delta.rows }, {
    base: [],
    delta: [
      { _sign: 1, _sequence: 0, __unit: 1 },
      { _sign: -1, _sequence: 2, __unit: 1 },
    ],
  });
});

const ZERO_EDGE: IIncrementalEdgeStatement = {
  head_rel: "flag",
  rule_id: "zero-edge:flag/0#1",
  head_kind: "set",
  head_table_name: "flag",
  head_delta_table_name: "__delta_flag",
  head_columns: [],
  key_indices: [],
  project_sql: `SELECT 1 AS "__unit"`,
};

test("zero-arity set edge writes and stages one deduplicated unit tuple", async () => {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, ZERO_DDL));

  await firstValueFrom(IncrementalRuntime.apply_edges(seam, [ZERO_EDGE], [ZERO_PLAN]));
  await firstValueFrom(IncrementalRuntime.apply_edges(seam, [ZERO_EDGE], [ZERO_PLAN]));

  const base = await firstValueFrom(seam.runner.execute(seam.db, `SELECT * FROM "flag"`));
  const delta = await firstValueFrom(
    seam.runner.execute(seam.db, `SELECT "_sign", "_sequence", "__unit" FROM "__delta_flag"`),
  );
  const next = await firstValueFrom(
    seam.runner.execute(seam.db, `SELECT "_phase", "_sequence", "__unit" FROM "__next_frontier_flag"`),
  );
  assert.deepEqual({ base: base.rows, delta: delta.rows, next: next.rows }, {
    base: [{ __id: 1, __unit: 1 }],
    delta: [{ _sign: 1, _sequence: 1, __unit: 1 }],
    next: [{ _phase: 0, _sequence: 1, __unit: 1 }],
  });
});
