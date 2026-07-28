/**
 * v1 evalProgramSql scale worker. Each tick inserts the new EDB rows, then
 * reruns the v1 Datalog evaluator over a fresh in-memory database state.
 */

import { firstValueFrom } from "rxjs";
import { createClient } from "@libsql/client";

import type { Program } from "../lower/ast.ts";
import { evalProgramSql } from "../lower/lowerSql.ts";
import type { RelTable, RelTables } from "../lower/types.ts";

const [shapeArg, rowsArg, recordPath] = process.argv.slice(2);
if (shapeArg === undefined || rowsArg === undefined || recordPath === undefined) {
  throw new Error("usage: v1_scale_bench.ts <shape> <rows> <record.jsonl>");
}
const recordFile = recordPath;

const shape = shapeArg;
const rows = Number(rowsArg);
const ticks = rows / 100;
const totalTicks = shape === "s3" ? ticks * 2 : ticks;

const programModule = await import(new URL("../gen/v1_scale_generated.ts", import.meta.url).href);
const program = programModule.program as Program;
const db = createClient({ url: ":memory:" });
const tables = new Map<string, RelTable>();

function quoteIdent(identifier: string): string {
  return `"${identifier.replaceAll('"', '""')}"`;
}

function tableName(rel: string): string {
  return `v1_${rel}`;
}

async function createTables(): Promise<void> {
  for (const decl of program.rels) {
    const columns = decl.columns.map(quoteIdent).join(", ");
    const table = tableName(decl.name);
    tables.set(decl.name, { table, columns: decl.columns });
    await db.execute(
      `CREATE TABLE ${quoteIdent(table)} (${decl.columns.map((column) => `${quoteIdent(column)} TEXT NOT NULL`).join(", ")}, PRIMARY KEY (${columns})) WITHOUT ROWID`,
    );
  }
}

async function insert(rel: string, values: readonly (readonly string[])[]): Promise<void> {
  if (values.length === 0) return;
  const table = quoteIdent(tableName(rel));
  const columns = program.rels.find((decl) => decl.name === rel)?.columns;
  if (columns === undefined) throw new Error(`unknown rel ${rel}`);
  const placeholders = values.map(() => `(${columns.map(() => "?").join(",")})`).join(",");
  const args = values.flatMap((value) => [...value]);
  await db.execute({ sql: `INSERT OR IGNORE INTO ${table} VALUES ${placeholders}`, args });
}

async function seedStatic(): Promise<void> {
  if (shape !== "s2") return;
  await insert(
    "b_link",
    Array.from({ length: 100 }, (_, key) => [`k${key}`, `m${key}`]),
  );
  await insert(
    "a_link",
    Array.from({ length: 100 }, (_, key) => [`m${key}`, `o${key}`]),
  );
}

function arrivals(tick: number): { rel: string; values: string[][] } {
  const start = tick * 100;
  if (shape === "s2") {
    return {
      rel: "c",
      values: Array.from({ length: 100 }, (_, offset) => [`k${offset}`, `p${start + offset}`]),
    };
  }
  if (shape === "s3") {
    const rel = tick < ticks ? "left" : "right";
    const valueStart = rel === "left" ? tick : tick - ticks;
    return {
      rel,
      values: Array.from({ length: 100 }, (_, offset) => [`${rel === "left" ? "l" : "r"}${valueStart * 100 + offset}`]),
    };
  }
  throw new Error(`unsupported v1 shape ${shape}`);
}

async function count(rel: string): Promise<number> {
  const result = await db.execute(`SELECT count(*) AS n FROM ${quoteIdent(tableName(rel))}`);
  return Number(result.rows[0]?.n ?? 0);
}

async function tick(tickIndex: number): Promise<void> {
  const batch = arrivals(tickIndex);
  await insert(batch.rel, batch.values);
  await firstValueFrom(evalProgramSql(db, program, tables as RelTables));
}

async function run(): Promise<void> {
  await createTables();
  await seedStatic();
  for (let i = 0; i < totalTicks; i += 1) await tick(i);

  const measuredTickDurations: number[] = [];
  const started = process.hrtime.bigint();
  for (let i = 0; i < totalTicks; i += 1) {
    const tickStarted = process.hrtime.bigint();
    await tick(i);
    measuredTickDurations.push(Number(process.hrtime.bigint() - tickStarted) / 1_000_000);
  }
  const totalWallMs = Number(process.hrtime.bigint() - started) / 1_000_000;
  const sorted = [...measuredTickDurations].sort((a, b) => a - b);
  const p95Index = Math.max(0, Math.ceil(sorted.length * 0.95) - 1);
  const finalTableSizes: Record<string, number> = {};
  for (const decl of program.rels) finalTableSizes[decl.name] = await count(decl.name);
  const result = {
    engine: "v1-gen",
    shape,
    rows,
    status: "OK",
    total_wall_ms: totalWallMs,
    ticks: totalTicks,
    arrivals: totalTicks * 100,
    mean_tick_ms: totalWallMs / totalTicks,
    p95_tick_ms: sorted[p95Index],
    max_tick_ms: sorted.at(-1),
    final_table_sizes: finalTableSizes,
    ms_per_1k_arrivals: totalWallMs / (totalTicks * 100) * 1000,
    worker_rss_mb: process.memoryUsage().rss / 1_048_576,
  };
  const fs = await import("node:fs/promises");
  await fs.appendFile(recordFile, `${JSON.stringify(result)}\n`);
  console.log(`CSV,v1-gen,${rows},${totalTicks * 100},${Object.values(finalTableSizes).reduce((sum, value) => sum + value, 0)},${totalWallMs.toFixed(6)},${(totalWallMs / totalTicks).toFixed(6)}`);
  db.close();
}

run().catch(async (failure: unknown) => {
  const message = failure instanceof Error ? `${failure.name}: ${failure.message}` : String(failure);
  console.error(message);
  process.exitCode = 1;
});
