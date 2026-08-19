/**
 * Q4: one writer, 1024 relations' frontier rows per tick, table accumulating.
 *
 * The question is whether ONE shared btree under PRIMARY KEY
 * (relation_id, row_id, tick, sign) costs more per row than 1024 small
 * per-relation btrees. Two measures, because driver dispatch and btree work
 * are separable:
 *   a. dispatch-matched, one row per statement in both arms;
 *   b. natural, arm B chunked 100 rows per statement (the store's CHUNK_ROWS,
 *      sprefa-store/js/src/engine/lib.ts), arm A unable to chunk across
 *      relations because each relation has its own table.
 * Frontier rows are never deleted here: the tree is meant to grow.
 */

import { markdownTable, median, openMemory, round } from "./common.ts";
import { ARM_B_TRANSIENT_DDL, armATransientDdl } from "./schema.ts";

const RELATIONS = 1024;
const TICKS = 300;
const RUNS = 5;
const CHUNK = 100;

async function armA(): Promise<number> {
  const db = openMemory();
  for (let index = 0; index < RELATIONS; index += 1) for (const statement of armATransientDdl(index)) await db.execute(statement);
  const start = performance.now();
  for (let tick = 1; tick <= TICKS; tick += 1) {
    for (let index = 0; index < RELATIONS; index += 1) {
      await db.execute({ sql: `INSERT INTO "__frontier_rel_${index}" ("_phase", "_sequence", "row_id") VALUES (?, ?, ?)`, args: [tick, index, tick] });
    }
  }
  return performance.now() - start;
}

async function armB(chunk: number): Promise<number> {
  const db = openMemory();
  for (const statement of ARM_B_TRANSIENT_DDL) await db.execute(statement);
  const start = performance.now();
  for (let tick = 1; tick <= TICKS; tick += 1) {
    for (let base = 0; base < RELATIONS; base += chunk) {
      const values: string[] = [];
      for (let offset = 0; offset < chunk && base + offset < RELATIONS; offset += 1) values.push(`(${base + offset}, ${tick}, ${tick}, 1)`);
      await db.execute(`INSERT INTO "frontier" ("relation_id", "row_id", "tick", "sign") VALUES ${values.join(", ")}`);
    }
  }
  return performance.now() - start;
}

async function armAChunked(): Promise<number> {
  // Arm A's only chunking is within one relation, and one tick gives it one
  // row per relation, so its statement count is fixed at RELATIONS per tick.
  return armA();
}

const rows = RELATIONS * TICKS;
const measures: readonly (readonly [string, () => Promise<number>, number])[] = [
  ["A, 1024 per-relation tables, 1 row/statement", armA, RELATIONS * TICKS],
  ["B, one shared table, 1 row/statement", () => armB(1), RELATIONS * TICKS],
  ["A, 1024 per-relation tables, chunked (cannot chunk across relations)", armAChunked, RELATIONS * TICKS],
  ["B, one shared table, 100 rows/statement", () => armB(CHUNK), Math.ceil(RELATIONS / CHUNK) * TICKS],
];

const table: string[][] = [];
for (const [label, run, statements] of measures) {
  const samples: number[] = [];
  for (let attempt = 0; attempt < RUNS; attempt += 1) samples.push(await run());
  const ms = median(samples);
  table.push([
    label,
    String(rows),
    String(statements),
    round(ms, 1).toFixed(1),
    Math.round(rows / (ms / 1000)).toLocaleString("en-US"),
    round(Math.max(...samples) / 1000, 2).toFixed(2),
  ]);
  process.stderr.write(`${label}: ${round(ms, 1)}ms\n`);
}

console.log(`### Q4. One writer, ${RELATIONS} relations x ${TICKS} ticks = ${rows.toLocaleString("en-US")} frontier rows, never deleted, median of ${RUNS}\n`);
console.log(markdownTable(["arm", "rows", "statements", "median ms", "rows/s", "worst run s"], table));
