/**
 * Q2: tick cost, per-relation transient tables (arm A) vs one shared pair (arm B).
 *
 * 200 ticks per run, 5 runs per cell, medians reported. One arrival per touched
 * relation. Per tick, per touched relation: insert the durable row, insert the
 * frontier row, run the frontier-to-durable join, then clear the tick's
 * frontier. Arm A clears one table per touched relation; arm B clears the tick
 * with one statement, and the statements/tick column carries that difference.
 */

import { markdownTable, median, openMemory, round } from "./common.ts";
import {
  type Arm,
  bootDdl,
  durableInsertSql,
  frontierDeleteSql,
  frontierInsertSql,
  frontierReadSql,
  touched,
} from "./schema.ts";

const TICKS = 200;
const RUNS = 5;
const RELATION_COUNTS = [16, 64, 256, 1024];

interface ICell {
  readonly arm: Arm;
  readonly relations: number;
  readonly k: number;
  readonly kLabel: string;
  readonly msPerTick: number;
  readonly statementsPerTick: number;
  readonly runMs: readonly number[];
  readonly insertMs: number;
  readonly readMs: number;
  readonly deleteMs: number;
}

async function runOnce(arm: Arm, relations: number, k: number): Promise<{ ms: number; statements: number; phases: [number, number, number] }> {
  const db = openMemory();
  for (const statement of bootDdl(arm, relations)) await db.execute(statement);

  let statements = 0;
  let insertMs = 0;
  let readMs = 0;
  let deleteMs = 0;
  let rowKey = 0;
  const start = performance.now();
  for (let tick = 1; tick <= TICKS; tick += 1) {
    const indices = touched(relations, k, tick);

    let phase = performance.now();
    for (const index of indices) {
      rowKey += 1;
      const inserted = await db.execute({ sql: durableInsertSql(index), args: [rowKey, rowKey * 2, rowKey * 3] });
      const rowId = Number(inserted.lastInsertRowid);
      await db.execute({
        sql: frontierInsertSql(arm, index),
        args: arm === "A" ? [tick, rowKey, rowId] : [index, rowId, tick],
      });
      statements += 2;
    }
    insertMs += performance.now() - phase;

    phase = performance.now();
    for (const index of indices) {
      await db.execute({ sql: frontierReadSql(arm, index), args: arm === "A" ? [tick] : [index, tick] });
      statements += 1;
    }
    readMs += performance.now() - phase;

    phase = performance.now();
    if (arm === "A") {
      for (const index of indices) {
        await db.execute({ sql: frontierDeleteSql(arm, index), args: [tick] });
        statements += 1;
      }
    } else {
      await db.execute({ sql: frontierDeleteSql(arm, 0), args: [tick] });
      statements += 1;
    }
    deleteMs += performance.now() - phase;
  }
  return { ms: performance.now() - start, statements, phases: [insertMs, readMs, deleteMs] };
}

const cells: ICell[] = [];
for (const relations of RELATION_COUNTS) {
  const ks: readonly (readonly [string, number])[] = [
    ["1", 1],
    ["N/8", Math.max(1, relations >> 3)],
    ["N", relations],
  ];
  for (const [kLabel, k] of ks) {
    for (const arm of ["A", "B", "B'"] as const) {
      const runMs: number[] = [];
      const insert: number[] = [];
      const read: number[] = [];
      const remove: number[] = [];
      let statements = 0;
      for (let run = 0; run < RUNS; run += 1) {
        const result = await runOnce(arm, relations, k);
        runMs.push(result.ms);
        insert.push(result.phases[0]);
        read.push(result.phases[1]);
        remove.push(result.phases[2]);
        statements = result.statements;
      }
      cells.push({
        arm,
        relations,
        k,
        kLabel,
        msPerTick: median(runMs) / TICKS,
        statementsPerTick: statements / TICKS,
        runMs,
        insertMs: median(insert) / TICKS,
        readMs: median(read) / TICKS,
        deleteMs: median(remove) / TICKS,
      });
      process.stderr.write(`cell arm=${arm} N=${relations} k=${kLabel} median=${round(median(runMs), 1)}ms\n`);
    }
  }
}

const rows: string[][] = [];
for (const relations of RELATION_COUNTS) {
  for (const kLabel of ["1", "N/8", "N"]) {
    const a = cells.find((cell) => cell.arm === "A" && cell.relations === relations && cell.kLabel === kLabel) as ICell;
    const b = cells.find((cell) => cell.arm === "B" && cell.relations === relations && cell.kLabel === kLabel) as ICell;
    const bPrime = cells.find((cell) => cell.arm === "B'" && cell.relations === relations && cell.kLabel === kLabel) as ICell;
    rows.push([
      String(relations),
      `${kLabel} (${a.k})`,
      round(a.msPerTick, 3).toFixed(3),
      round(b.msPerTick, 3).toFixed(3),
      round(bPrime.msPerTick, 3).toFixed(3),
      round(b.msPerTick / a.msPerTick, 3).toFixed(3),
      round(bPrime.msPerTick / a.msPerTick, 3).toFixed(3),
      String(a.statementsPerTick),
      String(b.statementsPerTick),
      round(Math.max(...a.runMs) / 1000, 2).toFixed(2),
      round(Math.max(...b.runMs) / 1000, 2).toFixed(2),
      round(Math.max(...bPrime.runMs) / 1000, 2).toFixed(2),
    ]);
  }
}

console.log("### Q2a. ms/tick, median of 5 runs of 200 ticks\n");
console.log(
  markdownTable(
    ["N", "k", "arm A ms/tick", "arm B ms/tick", "arm B' ms/tick", "B/A", "B'/A", "arm A stmts/tick", "arm B stmts/tick", "arm A worst run s", "arm B worst run s", "arm B' worst run s"],
    rows,
  ),
);

console.log("\n### Q2b. phase split of the same medians, ms/tick\n");
console.log(
  markdownTable(
    ["N", "k", "arm", "insert", "read", "delete"],
    cells.map((cell) => [
      String(cell.relations),
      cell.kLabel,
      cell.arm,
      round(cell.insertMs, 3).toFixed(3),
      round(cell.readMs, 3).toFixed(3),
      round(cell.deleteMs, 3).toFixed(3),
    ]),
  ),
);

console.log(`\n<!-- json ${JSON.stringify(cells)} -->`);
