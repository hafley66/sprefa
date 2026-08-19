/**
 * Q3: boot cost. Time every CREATE for N relations in both arms, then read
 * page_count from both the main and the temp database (arm A's transient
 * tables are TEMP, so `PRAGMA page_count` alone reports none of them).
 * Median of 5.
 */

import { markdownTable, median, openMemory, round } from "./common.ts";
import { type Arm, bootDdl } from "./schema.ts";

const RUNS = 5;
const RELATION_COUNTS = [16, 64, 256, 1024];

async function bootOnce(arm: Arm, relations: number) {
  const ddl = bootDdl(arm, relations);
  const db = openMemory();
  const start = performance.now();
  for (const statement of ddl) await db.execute(statement);
  const ms = performance.now() - start;
  const mainPages = Number((await db.execute("PRAGMA main.page_count")).rows[0][0]);
  const tempPages = Number((await db.execute("PRAGMA temp.page_count")).rows[0][0]);
  const objects = await db.execute(
    "SELECT count(*) FROM (SELECT name FROM sqlite_master UNION ALL SELECT name FROM sqlite_temp_master)",
  );
  return { ms, mainPages, tempPages, objects: Number(objects.rows[0][0]), statements: ddl.length, bytes: ddl.reduce((total, s) => total + Buffer.byteLength(s, "utf8"), 0) };
}

const rows: string[][] = [];
for (const relations of RELATION_COUNTS) {
  for (const arm of ["A", "B"] as const) {
    const samples: Awaited<ReturnType<typeof bootOnce>>[] = [];
    for (let run = 0; run < RUNS; run += 1) samples.push(await bootOnce(arm, relations));
    const last = samples[samples.length - 1];
    rows.push([
      String(relations),
      arm,
      String(last.statements),
      String(last.bytes),
      String(last.objects),
      round(median(samples.map((sample) => sample.ms)), 2).toFixed(2),
      String(last.mainPages),
      String(last.tempPages),
    ]);
  }
}

console.log("### Q3. Boot cost, median of 5\n");
console.log(
  markdownTable(
    ["N", "arm", "CREATE statements", "DDL bytes", "sqlite_master + temp objects", "boot ms", "main page_count", "temp page_count"],
    rows,
  ),
);
