/** Q6: retraction cost. Arm A signs into `__delta_<rel>` (lower.pl:6331-6333); `__frontier_<rel>` (lower.pl:6347-6349) has `_phase`, no sign column.
 *  N=256, k=32, 200 ticks, 5 runs. Shape, phase order, and statement counts are in REPORT.md Q6. */

import { markdownTable, median, openMemory, round } from "./common.ts";
import { durableDdl, touched } from "./schema.ts";

const RELATIONS = 256;
const K = 32;
const TICKS = 200;
const RUNS = 5;

type RetractionArm = "A" | "B" | "B'";

/** Arm A transient DDL for this cell: the phase frontier, the signed delta, the support table. */
function armATransientDdl(index: number): readonly string[] {
  return [
    `CREATE TEMP TABLE "__frontier_rel_${index}" ("_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, "row_id" INTEGER NOT NULL)`,
    `CREATE INDEX "__frontier_rel_${index}_phase" ON "__frontier_rel_${index}" ("_phase")`,
    `CREATE TEMP TABLE "__delta_rel_${index}" ("_sign" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, "row_id" INTEGER NOT NULL)`,
    `CREATE INDEX "__delta_rel_${index}_sign" ON "__delta_rel_${index}" ("_sign")`,
    `CREATE INDEX "__delta_rel_${index}_group" ON "__delta_rel_${index}" ("_sequence", "row_id")`,
    `CREATE TEMP TABLE "__support_next_rel_${index}" ("row_id" INTEGER NOT NULL, "rule_id" INTEGER NOT NULL, "__refcount" INTEGER NOT NULL, PRIMARY KEY ("row_id", "rule_id")) WITHOUT ROWID`,
  ];
}

function armBFrontierDdl(arm: RetractionArm): readonly string[] {
  const key =
    arm === "B"
      ? `"relation_id", "row_id", "tick", "sign"`
      : `"relation_id", "tick", "row_id", "sign"`;
  return [
    `CREATE TEMP TABLE "frontier" ("relation_id" INTEGER NOT NULL, "row_id" INTEGER NOT NULL, "tick" INTEGER NOT NULL, "sign" INTEGER NOT NULL CHECK ("sign" IN (-1, 1)), PRIMARY KEY (${key}))`,
    `CREATE TEMP TABLE "support_count" ("relation_id" INTEGER NOT NULL, "row_id" INTEGER NOT NULL, "rule_id" INTEGER NOT NULL, "count" INTEGER NOT NULL, PRIMARY KEY ("relation_id", "row_id", "rule_id"))`,
  ];
}

function bootDdl(arm: RetractionArm, relations: number): readonly string[] {
  const statements: string[] = [];
  for (let index = 0; index < relations; index += 1) statements.push(durableDdl(index));
  if (arm === "A") {
    for (let index = 0; index < relations; index += 1) statements.push(...armATransientDdl(index));
  } else {
    statements.push(...armBFrontierDdl(arm));
  }
  return statements;
}

interface IPhases {
  arrival: number;
  retraction: number;
  read: number;
  clear: number;
}

async function runOnce(arm: RetractionArm): Promise<{ ms: number; statements: number; retractions: number; phases: IPhases }> {
  const db = openMemory();
  for (const statement of bootDdl(arm, RELATIONS)) await db.execute(statement);

  // Pre-seeding is outside the timed window: tick 1 needs a prior row to retract.
  const lastRowId: number[] = [];
  let rowKey = 0;
  for (let index = 0; index < RELATIONS; index += 1) {
    rowKey += 1;
    const seeded = await db.execute({
      sql: `INSERT INTO "rel_${index}" ("row_key", "value_a", "value_b") VALUES (?, ?, ?)`,
      args: [rowKey, rowKey * 2, rowKey * 3],
    });
    const rowId = Number(seeded.lastInsertRowid);
    lastRowId[index] = rowId;
    if (arm === "A") {
      await db.execute({
        sql: `INSERT INTO "__support_next_rel_${index}" ("row_id", "rule_id", "__refcount") VALUES (?, 0, 1)`,
        args: [rowId],
      });
    } else {
      await db.execute({
        sql: `INSERT INTO "support_count" ("relation_id", "row_id", "rule_id", "count") VALUES (?, ?, 0, 1)`,
        args: [index, rowId],
      });
    }
  }

  const phases: IPhases = { arrival: 0, retraction: 0, read: 0, clear: 0 };
  let statements = 0;
  let retractions = 0;
  const start = performance.now();
  for (let tick = 1; tick <= TICKS; tick += 1) {
    const indices = touched(RELATIONS, K, tick);
    const arrivedRowId: number[] = [];

    let phase = performance.now();
    for (const index of indices) {
      rowKey += 1;
      const inserted = await db.execute({
        sql: `INSERT INTO "rel_${index}" ("row_key", "value_a", "value_b") VALUES (?, ?, ?)`,
        args: [rowKey, rowKey * 2, rowKey * 3],
      });
      const rowId = Number(inserted.lastInsertRowid);
      arrivedRowId.push(rowId);
      if (arm === "A") {
        await db.execute({
          sql: `INSERT INTO "__frontier_rel_${index}" ("_phase", "_sequence", "row_id") VALUES (?, ?, ?)`,
          args: [tick, rowKey, rowId],
        });
        await db.execute({
          sql: `INSERT INTO "__support_next_rel_${index}" ("row_id", "rule_id", "__refcount") VALUES (?, 0, 1)`,
          args: [rowId],
        });
      } else {
        await db.execute({
          sql: `INSERT INTO "frontier" ("relation_id", "row_id", "tick", "sign") VALUES (?, ?, ?, 1)`,
          args: [index, rowId, tick],
        });
        await db.execute({
          sql: `INSERT INTO "support_count" ("relation_id", "row_id", "rule_id", "count") VALUES (?, ?, 0, 1)`,
          args: [index, rowId],
        });
      }
      statements += 3;
    }
    phases.arrival += performance.now() - phase;

    phase = performance.now();
    for (let slot = 0; slot < indices.length; slot += 1) {
      const index = indices[slot];
      const victim = lastRowId[index];
      await db.execute({ sql: `DELETE FROM "rel_${index}" WHERE "__id" = ?`, args: [victim] });
      if (arm === "A") {
        await db.execute({
          sql: `INSERT INTO "__delta_rel_${index}" ("_sign", "_sequence", "row_id") VALUES (-1, ?, ?)`,
          args: [tick, victim],
        });
        await db.execute({
          sql: `UPDATE "__support_next_rel_${index}" SET "__refcount" = "__refcount" - 1 WHERE "row_id" = ? AND "rule_id" = 0`,
          args: [victim],
        });
      } else {
        await db.execute({
          sql: `INSERT INTO "frontier" ("relation_id", "row_id", "tick", "sign") VALUES (?, ?, ?, -1)`,
          args: [index, victim, tick],
        });
        await db.execute({
          sql: `UPDATE "support_count" SET "count" = "count" - 1 WHERE "relation_id" = ? AND "row_id" = ? AND "rule_id" = 0`,
          args: [index, victim],
        });
      }
      statements += 3;
      retractions += 1;
      lastRowId[index] = arrivedRowId[slot];
    }
    phases.retraction += performance.now() - phase;

    phase = performance.now();
    for (const index of indices) {
      const projection = `typed."__id", typed."row_key", typed."value_a", typed."value_b"`;
      if (arm === "A") {
        await db.execute({
          sql: `SELECT ${projection} FROM "__frontier_rel_${index}" f JOIN "rel_${index}" typed ON typed."__id" = f."row_id" WHERE f."_phase" = ?`,
          args: [tick],
        });
      } else {
        await db.execute({
          sql: `SELECT ${projection} FROM "frontier" f JOIN "rel_${index}" typed ON typed."__id" = f."row_id" WHERE f."relation_id" = ? AND f."tick" = ?`,
          args: [index, tick],
        });
      }
      statements += 1;
    }
    phases.read += performance.now() - phase;

    phase = performance.now();
    if (arm === "A") {
      for (const index of indices) {
        await db.execute({ sql: `DELETE FROM "__frontier_rel_${index}" WHERE "_phase" = ?`, args: [tick] });
        await db.execute({ sql: `DELETE FROM "__delta_rel_${index}" WHERE "_sequence" = ?`, args: [tick] });
        statements += 2;
      }
    } else {
      await db.execute({ sql: `DELETE FROM "frontier" WHERE "tick" = ?`, args: [tick] });
      statements += 1;
    }
    phases.clear += performance.now() - phase;
  }
  return { ms: performance.now() - start, statements, retractions, phases };
}

const ARMS: readonly RetractionArm[] = ["A", "B", "B'"];
const results = new Map<RetractionArm, { msPerTick: number; statementsPerTick: number; worstS: number; retractions: number; phases: IPhases }>();

for (const arm of ARMS) {
  const runMs: number[] = [];
  const arrival: number[] = [];
  const retraction: number[] = [];
  const read: number[] = [];
  const clear: number[] = [];
  let statements = 0;
  let retractions = 0;
  for (let run = 0; run < RUNS; run += 1) {
    const result = await runOnce(arm);
    runMs.push(result.ms);
    arrival.push(result.phases.arrival);
    retraction.push(result.phases.retraction);
    read.push(result.phases.read);
    clear.push(result.phases.clear);
    statements = result.statements;
    retractions = result.retractions;
  }
  results.set(arm, {
    msPerTick: median(runMs) / TICKS,
    statementsPerTick: statements / TICKS,
    worstS: Math.max(...runMs) / 1000,
    retractions,
    phases: {
      arrival: median(arrival) / TICKS,
      retraction: median(retraction) / TICKS,
      read: median(read) / TICKS,
      clear: median(clear) / TICKS,
    },
  });
  process.stderr.write(`retraction arm=${arm} median=${round(median(runMs), 1)}ms\n`);
}

const armA = results.get("A") as NonNullable<ReturnType<typeof results.get>>;

console.log(
  `### Q6. Retraction, N=${RELATIONS}, k=${K}, ${TICKS} ticks, 1 arrival + 1 retraction per touched relation, median of ${RUNS}\n`,
);
console.log(
  markdownTable(
    ["arm", "ms/tick", "vs A", "stmts/tick", "retractions/run", "worst run s"],
    ARMS.map((arm) => {
      const cell = results.get(arm) as NonNullable<ReturnType<typeof results.get>>;
      return [
        arm === "A"
          ? "A, per-relation __frontier_ + __delta_ + __support_next_"
          : arm === "B"
            ? "B, shared frontier (relation_id, row_id, tick, sign)"
            : "B', shared frontier (relation_id, tick, row_id, sign)",
        round(cell.msPerTick, 3).toFixed(3),
        round(cell.msPerTick / armA.msPerTick, 3).toFixed(3),
        String(cell.statementsPerTick),
        String(cell.retractions),
        round(cell.worstS, 2).toFixed(2),
      ];
    }),
  ),
);

console.log("\n### Q6b. Phase split of the same medians, ms/tick\n");
console.log(
  markdownTable(
    ["arm", "arrival", "retraction", "read", "clear"],
    ARMS.map((arm) => {
      const cell = results.get(arm) as NonNullable<ReturnType<typeof results.get>>;
      return [
        arm,
        round(cell.phases.arrival, 3).toFixed(3),
        round(cell.phases.retraction, 3).toFixed(3),
        round(cell.phases.read, 3).toFixed(3),
        round(cell.phases.clear, 3).toFixed(3),
      ];
    }),
  ),
);

console.log(`\n<!-- json ${JSON.stringify([...results.entries()])} -->`);
