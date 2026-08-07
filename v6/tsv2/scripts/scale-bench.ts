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

function parse_args(): { shape: Shape; rows: number; record_path: string; log_path: string | undefined } {
  const [, , shape_arg, rows_arg, record_path, log_path] = process.argv;
  if (shape_arg !== "s1" && shape_arg !== "s2" && shape_arg !== "s3") throw new Error("scale-bench: shape must be s1, s2, or s3");
  const rows = Number(rows_arg);
  if (!Number.isInteger(rows) || rows < BATCH_SIZE || rows % BATCH_SIZE !== 0) throw new Error("scale-bench: rows must be a positive multiple of 100");
  if (record_path === undefined) throw new Error("scale-bench: missing record path");
  return { shape: shape_arg, rows, record_path, log_path };
}

function batch<T>(rows: readonly T[], start: number): readonly T[] {
  return rows.slice(start, start + BATCH_SIZE);
}

function schedule_for(shape: Shape, rows: number): readonly IArrivalBatch[] {
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

function row_count(seam: ISqlSeam, rel: string) {
  return seam.runner.execute(seam.db, `SELECT count(*) FROM ${rel}`);
}

async function final_table_sizes(seam: ISqlSeam): Promise<Readonly<Record<string, number>>> {
  const relations = Object.keys(program.rel_columns);
  const counts = await lastValueFrom(
    forkJoin(Object.fromEntries(relations.map((rel) => [rel, row_count(seam, rel)]))),
  );
  return Object.fromEntries(
    relations.map((rel) => [rel, Number(counts[rel]?.rows[0]?.[0] ?? 0)]),
  );
}

async function main(): Promise<void> {
  const { shape, rows, record_path, log_path } = parse_args();
  const schedule = schedule_for(shape, rows);
  const seam = ScratchStore.open(":memory:");
  await lastValueFrom(ScratchStore.boot(seam, program.ddl));
  await lastValueFrom(
    concat(
      ...program.boot.map((statement) =>
        seam.runner.execute(seam.db, { sql: statement.sql, args: [...statement.params] }),
      ),
    ).pipe(toArray()),
  );

  const tick_durations: number[] = [];
  let host_peak_bytes = process.memoryUsage().heapUsed;
  let previous = process.hrtime.bigint();
  const started = previous;
  const lines = await lastValueFrom(
    TickFold.run(program, seam, schedule).pipe(
      tap(() => {
        const now = process.hrtime.bigint();
        tick_durations.push(Number(now - previous) / 1_000_000);
        host_peak_bytes = Math.max(host_peak_bytes, process.memoryUsage().heapUsed);
        previous = now;
      }),
      toArray(),
    ),
  );
  const finished = process.hrtime.bigint();
  const total_wall_ms = Number(finished - started) / 1_000_000;
  const ordered = [...tick_durations].sort((a, b) => a - b);
  const mean_tick_ms = tick_durations.reduce((sum, value) => sum + value, 0) / tick_durations.length;
  const p95_tick_ms = ordered[Math.max(0, Math.ceil(ordered.length * 0.95) - 1)] ?? 0;
  const max_tick_ms = ordered.at(-1) ?? 0;
  const arrivals = schedule.reduce((sum, tick) => sum + tick.length, 0);
  const table_sizes = await final_table_sizes(seam);
  const result = {
    engine: "tsv2-gen",
    shape,
    rows,
    status: "OK",
    ticks: schedule.length,
    arrivals,
    total_wall_ms: total_wall_ms,
    mean_tick_ms: mean_tick_ms,
    p95_tick_ms: p95_tick_ms,
    max_tick_ms: max_tick_ms,
    final_table_sizes: table_sizes,
    ms_per_1k_arrivals: total_wall_ms / (arrivals / 1000),
    worker_rss_mb: process.memoryUsage().rss / 1_048_576,
    host_peak_mb: host_peak_bytes / 1_048_576,
  };
  if (log_path !== undefined) writeFileSync(log_path, `${lines.join("\n")}\n`);
  if (record_path !== "/dev/null") appendFileSync(record_path, `${JSON.stringify(result)}\n`);
  const final_rows = Object.values(table_sizes).reduce((sum, value) => sum + value, 0);
  process.stdout.write(`CSV,tsv2-gen,${rows},${arrivals},${final_rows},${total_wall_ms},${mean_tick_ms}\n`);
}

void main().catch((error: unknown) => {
  process.stderr.write(`${error instanceof Error ? error.stack : String(error)}\n`);
  process.exitCode = 1;
});
