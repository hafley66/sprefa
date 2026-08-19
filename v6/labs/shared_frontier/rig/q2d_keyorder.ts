/**
 * Q2d: the shared frontier's PRIMARY KEY column order.
 *
 * The plan writes PRIMARY KEY (relation_id, row_id, tick, sign). The read
 * filters relation_id and tick, and row_id sits between them, so the tick
 * predicate cannot be an index prefix. Arm B' is the same table with
 * (relation_id, tick, row_id, sign). Frontier rows accumulate over RETAINED
 * ticks so the per-relation slice is wider than one row.
 */

import { markdownTable, median, openMemory, round } from "./common.ts";

const RELATIONS = 256;
const RETAINED = 200;
const READS_PER_RUN = RELATIONS;
const RUNS = 5;

const VARIANTS: readonly (readonly [string, string])[] = [
  ["B  (relation_id, row_id, tick, sign)", `CREATE TEMP TABLE "frontier" ("relation_id" INTEGER NOT NULL, "row_id" INTEGER NOT NULL, "tick" INTEGER NOT NULL, "sign" INTEGER NOT NULL CHECK ("sign" IN (-1, 1)), PRIMARY KEY ("relation_id", "row_id", "tick", "sign"))`],
  ["B' (relation_id, tick, row_id, sign)", `CREATE TEMP TABLE "frontier" ("relation_id" INTEGER NOT NULL, "row_id" INTEGER NOT NULL, "tick" INTEGER NOT NULL, "sign" INTEGER NOT NULL CHECK ("sign" IN (-1, 1)), PRIMARY KEY ("relation_id", "tick", "row_id", "sign"))`],
];

const READ_SQL =
  `SELECT typed."__id", typed."row_key", typed."value_a", typed."value_b" FROM "frontier" f` +
  ` JOIN "rel_0" typed ON typed."__id" = f."row_id" WHERE f."relation_id" = ? AND f."tick" = ?`;

async function seed(ddl: string) {
  const db = openMemory();
  await db.execute(`CREATE TABLE "rel_0" ("__id" INTEGER PRIMARY KEY, "row_key" INTEGER NOT NULL, "value_a" INTEGER NOT NULL, "value_b" INTEGER NOT NULL, UNIQUE ("row_key"))`);
  for (let base = 0; base < RETAINED; base += 100) {
    const values: string[] = [];
    for (let offset = 0; offset < 100 && base + offset < RETAINED; offset += 1) {
      const key = base + offset + 1;
      values.push(`(${key}, ${key * 2}, ${key * 3})`);
    }
    await db.execute(`INSERT INTO "rel_0" ("row_key", "value_a", "value_b") VALUES ${values.join(", ")}`);
  }
  await db.execute(ddl);
  for (let tick = 1; tick <= RETAINED; tick += 1) {
    for (let base = 0; base < RELATIONS; base += 100) {
      const values: string[] = [];
      for (let offset = 0; offset < 100 && base + offset < RELATIONS; offset += 1) values.push(`(${base + offset}, ${tick}, ${tick}, 1)`);
      await db.execute(`INSERT INTO "frontier" ("relation_id", "row_id", "tick", "sign") VALUES ${values.join(", ")}`);
    }
  }
  return db;
}

const table: string[][] = [];
const plans: string[] = [];
for (const [label, ddl] of VARIANTS) {
  const db = await seed(ddl);
  const plan = await db.execute({ sql: `EXPLAIN QUERY PLAN ${READ_SQL}`, args: [0, 7] });
  plans.push(`#### ${label}\n\n\`\`\`\n${plan.rows.map((row) => String(row[3])).join("\n")}\n\`\`\`\n`);
  const samples: number[] = [];
  for (let run = 0; run < RUNS; run += 1) {
    const start = performance.now();
    for (let relation = 0; relation < READS_PER_RUN; relation += 1) await db.execute({ sql: READ_SQL, args: [relation, 7] });
    samples.push(performance.now() - start);
  }
  const ms = median(samples);
  table.push([label, String(RELATIONS * RETAINED), String(READS_PER_RUN), round(ms, 2).toFixed(2), round((ms * 1000) / READS_PER_RUN, 1).toFixed(1)]);
}

console.log(`### Q2d. Shared-frontier key order, ${RELATIONS} relations x ${RETAINED} retained ticks, median of ${RUNS}\n`);
console.log(markdownTable(["frontier PRIMARY KEY", "frontier rows", "reads per run", "median ms", "us per read"], table));
console.log(`\n${plans.join("\n")}`);
