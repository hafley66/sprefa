/**
 * One tsv2 scale cell. The shell runner owns compilation, warmup policy,
 * timeout policy, and the shared bench CSV. This file only builds the
 * generated program's arrival schedule and measures the existing TickFold.
 */

import { appendFileSync, writeFileSync } from "node:fs";
import { concat, forkJoin, lastValueFrom, tap, toArray } from "rxjs";

import { program } from "../gen/scale_generated.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import { TickFold } from "../runtime/tickLoop.ts";
import type { IArrivalBatch, IRow, ISqlSeam } from "../runtime/types.ts";

const BATCH_SIZE = 100;

type Shape = "s1" | "s2" | "s3";

function parseArgs(): { shape: Shape; rows: number; recordPath: string; logPath: string | undefined } {
  const [, , shapeArg, rowsArg, recordPath, logPath] = process.argv;
  if (shapeArg !== "s1" && shapeArg !== "s2" && shapeArg !== "s3") throw new Error("scale-bench: shape must be s1, s2, or s3");
  const rows = Number(rowsArg);
  if (!Number.isInteger(rows) || rows < BATCH_SIZE || rows % BATCH_SIZE !== 0) throw new Error("scale-bench: rows must be a positive multiple of 100");
  if (recordPath === undefined) throw new Error("scale-bench: missing record path");
  return { shape: shapeArg, rows, recordPath, logPath };
}

function batch<T>(rows: readonly T[], start: number): readonly T[] {
  return rows.slice(start, start + BATCH_SIZE);
}

function scheduleFor(shape: Shape, rows: number): readonly IArrivalBatch[] {
  if (shape === "s1") {
    return Array.from({ length: rows / BATCH_SIZE }, (_, tick) =>
      Array.from({ length: BATCH_SIZE }, (_, key) => ({
        rel: "change",
        sign: "add" as const,
        row: [`k${key}`, `v${tick * 1000 + key}`] as const,
      })),
    );
  }

  if (shape === "s2") {
    return Array.from({ length: rows / BATCH_SIZE }, (_, tick) =>
      Array.from({ length: BATCH_SIZE }, (_, offset) => ({
        rel: "c",
        sign: "add" as const,
        row: [`k${offset}`, `p${tick * BATCH_SIZE + offset}`] as const,
      })),
    );
  }

  const left = Array.from({ length: rows }, (_, value) => ({ rel: "left", sign: "add" as const, row: [`l${value}`] as const }));
  const right = Array.from({ length: rows }, (_, value) => ({ rel: "right", sign: "add" as const, row: [`r${value}`] as const }));
  return [
    ...Array.from({ length: rows / BATCH_SIZE }, (_, index) => batch(left, index * BATCH_SIZE)),
    ...Array.from({ length: rows / BATCH_SIZE }, (_, index) => batch(right, index * BATCH_SIZE)),
  ];
}

function rowCount(seam: ISqlSeam, rel: string) {
  return seam.runner.execute(seam.db, `SELECT count(*) FROM ${rel}`);
}

async function finalTableSizes(seam: ISqlSeam): Promise<Readonly<Record<string, number>>> {
  const relations = Object.keys(program.relColumns);
  const counts = await lastValueFrom(
    forkJoin(Object.fromEntries(relations.map((rel) => [rel, rowCount(seam, rel)]))),
  );
  return Object.fromEntries(
    relations.map((rel) => [rel, Number(counts[rel]?.rows[0]?.[0] ?? 0)]),
  );
}

async function main(): Promise<void> {
  const { shape, rows, recordPath, logPath } = parseArgs();
  const schedule = scheduleFor(shape, rows);
  const seam = ScratchStore.open(":memory:");
  await lastValueFrom(ScratchStore.boot(seam, program.ddl));
  await lastValueFrom(
    concat(
      ...program.boot.map((statement) =>
        seam.runner.execute(seam.db, { sql: statement.sql, args: [...statement.params] }),
      ),
    ).pipe(toArray()),
  );

  const tickDurations: number[] = [];
  let hostPeakBytes = process.memoryUsage().heapUsed;
  let previous = process.hrtime.bigint();
  const started = previous;
  const lines = await lastValueFrom(
    TickFold.run(program, seam, schedule).pipe(
      tap(() => {
        const now = process.hrtime.bigint();
        tickDurations.push(Number(now - previous) / 1_000_000);
        hostPeakBytes = Math.max(hostPeakBytes, process.memoryUsage().heapUsed);
        previous = now;
      }),
      toArray(),
    ),
  );
  const finished = process.hrtime.bigint();
  const totalWallMs = Number(finished - started) / 1_000_000;
  const ordered = [...tickDurations].sort((a, b) => a - b);
  const meanTickMs = tickDurations.reduce((sum, value) => sum + value, 0) / tickDurations.length;
  const p95TickMs = ordered[Math.max(0, Math.ceil(ordered.length * 0.95) - 1)] ?? 0;
  const maxTickMs = ordered.at(-1) ?? 0;
  const arrivals = schedule.reduce((sum, tick) => sum + tick.length, 0);
  const tableSizes = await finalTableSizes(seam);
  const result = {
    engine: "tsv2-gen",
    shape,
    rows,
    status: "OK",
    ticks: schedule.length,
    arrivals,
    total_wall_ms: totalWallMs,
    mean_tick_ms: meanTickMs,
    p95_tick_ms: p95TickMs,
    max_tick_ms: maxTickMs,
    final_table_sizes: tableSizes,
    ms_per_1k_arrivals: totalWallMs / (arrivals / 1000),
    worker_rss_mb: process.memoryUsage().rss / 1_048_576,
    host_peak_mb: hostPeakBytes / 1_048_576,
  };
  if (logPath !== undefined) writeFileSync(logPath, `${lines.join("\n")}\n`);
  if (recordPath !== "/dev/null") appendFileSync(recordPath, `${JSON.stringify(result)}\n`);
  const finalRows = Object.values(tableSizes).reduce((sum, value) => sum + value, 0);
  process.stdout.write(`CSV,tsv2-gen,${rows},${arrivals},${finalRows},${totalWallMs},${meanTickMs}\n`);
}

void main().catch((error: unknown) => {
  process.stderr.write(`${error instanceof Error ? error.stack : String(error)}\n`);
  process.exitCode = 1;
});
