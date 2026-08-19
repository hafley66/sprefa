/**
 * Q2c: EXPLAIN QUERY PLAN for the frontier-to-durable join in both arms.
 *
 * A SCAN on either side is a finding. Run on a booted database carrying
 * DURABLE_ROWS rows per relation and one frontier row per relation, both
 * before and after ANALYZE, so a plan chosen off degenerate statistics is
 * visible as such.
 */

import { openMemory } from "./common.ts";
import { type Arm, bootDdl, frontierInsertSql, frontierReadSql } from "./schema.ts";

const RELATIONS = 64;
const DURABLE_ROWS = 4000;
const CHUNK = 100;

async function seed(arm: Arm) {
  const db = openMemory();
  for (const statement of bootDdl(arm, RELATIONS)) await db.execute(statement);
  for (let index = 0; index < RELATIONS; index += 1) {
    for (let base = 0; base < DURABLE_ROWS; base += CHUNK) {
      const values: string[] = [];
      for (let offset = 0; offset < CHUNK; offset += 1) {
        const key = base + offset + 1;
        values.push(`(${key}, ${key * 2}, ${key * 3})`);
      }
      await db.execute(`INSERT INTO "rel_${index}" ("row_key", "value_a", "value_b") VALUES ${values.join(", ")}`);
    }
    await db.execute({ sql: frontierInsertSql(arm, index), args: arm === "A" ? [1, 1, 17] : [index, 17, 1] });
  }
  return db;
}

async function plan(db: Awaited<ReturnType<typeof seed>>, arm: Arm, label: string) {
  const sql = frontierReadSql(arm, 7);
  console.log(`#### arm ${arm}, ${label}\n`);
  console.log("```sql");
  console.log(sql);
  console.log("```\n");
  console.log("```");
  const rows = await db.execute({ sql: `EXPLAIN QUERY PLAN ${sql}`, args: arm === "A" ? [1] : [7, 1] });
  for (const row of rows.rows) console.log(String(row[3]));
  console.log("```\n");
}

console.log(`### Q2c. EXPLAIN QUERY PLAN, N=${RELATIONS}, ${DURABLE_ROWS} durable rows per relation, 1 frontier row per relation\n`);
for (const arm of ["A", "B", "B'"] as const) {
  const db = await seed(arm);
  await plan(db, arm, "no ANALYZE");
  await db.execute("ANALYZE");
  await plan(db, arm, "after ANALYZE");
}
