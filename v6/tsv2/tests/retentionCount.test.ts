import assert from "node:assert/strict";
import { test } from "node:test";

import { firstValueFrom } from "rxjs";
import { stmt_counter } from "sprefa-store-engine/src/engine/counter.ts";

import {
  incrementalPlan,
  program,
} from "../gen_emitted/retention_count_prunes_oldest.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";

async function runRetentionTick(rowCount: number) {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, program.ddl));
  stmt_counter.reset();
  const deltas = await firstValueFrom(
    program.tick(
      seam,
      Array.from({ length: rowCount }, (_value, index) => ({
        rel: "event",
        sign: "add" as const,
        row: [`row_${index}`],
      })),
    ),
  );
  const statementCount = stmt_counter.get();
  const finalResult = await firstValueFrom(
    seam.runner.execute(seam.db, program.finalSelect.event!),
  );
  const finalRows = finalResult.rows.map((row) => String(row.col1)).sort();
  seam.db.close();
  return {
    statementCount,
    finalRows,
    eventDelta: deltas.rels.find((delta) => delta.rel === "event"),
  };
}

test("keep(count) lowers to one set-based retention statement", () => {
  assert.deepEqual(
    incrementalPlan.retention,
    [
      {
        rel: "event",
        count: 2,
        deleteSql:
          'DELETE FROM "event" WHERE rowid NOT IN (SELECT rowid FROM "event" ORDER BY rowid DESC LIMIT 2) RETURNING "col1"',
      },
    ],
  );
});

test("keep(count) statement count is flat and the oldest rows are pruned", async () => {
  const threeRows = await runRetentionTick(3);
  const hundredRows = await runRetentionTick(100);
  assert.deepEqual(
    {
      statementCounts: [threeRows.statementCount, hundredRows.statementCount],
      threeFinal: threeRows.finalRows,
      hundredFinal: hundredRows.finalRows,
      threeDelta: threeRows.eventDelta,
      hundredDeltaCount: hundredRows.eventDelta?.add.length,
    },
    {
      statementCounts: [12, 12],
      threeFinal: ["row_1", "row_2"],
      hundredFinal: ["row_98", "row_99"],
      threeDelta: { rel: "event", add: [["row_1"], ["row_2"]], del: [] },
      hundredDeltaCount: 2,
    },
  );
});
