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
import { graphFixture } from "../../sprefa-store/bench/engines/1_postgres_reach.mjs";
import { stmt_counter } from "sprefa-store-engine/src/engine/counter.ts";
import assert from "node:assert/strict";

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
  if (process.argv[2] === "reach") return reach_cell();
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

async function reach_cell(): Promise<void> {
  const layers = Number(process.argv[3]);
  const width = Number(process.argv[4]);
  const graph = graphFixture(layers, width, Number(process.env.BENCH_BACK_STRIDE ?? 0));
  const { program: emitted } = await import("../gen/bench_root_reach.ts");
  const seam = ScratchStore.open(":memory:");
  const arrivals: IArrivalBatch = [
    ...graph.edges.map((row: number[]) => ({ rel: "edge", sign: "add" as const, row })),
    { rel: "root", sign: "add", row: [0] }, { rel: "root", sign: "add", row: [1] },
  ];
  const count = async () => Number((await lastValueFrom(seam.runner.execute(seam.db,
    `SELECT count(*) AS n FROM (${emitted.final_select.alive})`))).rows[0]?.n);
  const rows = async (rel: "alive" | "edge") => (await lastValueFrom(seam.runner.execute(seam.db,
    `${emitted.final_select[rel]} ORDER BY ${rel === "edge" ? "parent, child" : "node"}`))).rows
    .map(row => rel === "edge" ? [Number(row.parent), Number(row.child)] : Number(row.node));
  try {
    const setup_started = process.hrtime.bigint();
    await lastValueFrom(ScratchStore.boot(seam, emitted.ddl));
    for (const statement of emitted.boot) await lastValueFrom(seam.runner.execute(seam.db, { sql: statement.sql, args: [...statement.params] }));
    const initial = await lastValueFrom(emitted.tick(seam, arrivals));
    const before_count = await count();
    const setup_ms = Number(process.hrtime.bigint() - setup_started) / 1e6;
    assert.equal(initial.carry_pending, false, "root reach must settle inside its tick");
    assert.equal(before_count, graph.before.length);
    assert.deepEqual(await rows("alive"), graph.before);
    assert.deepEqual(await rows("edge"), graph.edges);
    process.stderr.write(`INPUT|tsv2-runtime|${graph.input_hash}\n`);
    stmt_counter.reset();
    const started = process.hrtime.bigint();
    const retracted = await lastValueFrom(emitted.tick(seam, [{ rel: "root", sign: "del", row: [0] }]));
    const after_count = await count();
    const retract_ms = Number(process.hrtime.bigint() - started) / 1e6;
    const statements = stmt_counter.get();
    assert.equal(retracted.carry_pending, false);
    assert.equal(after_count, graph.after.length);
    assert.deepEqual(await rows("alive"), graph.after);
    process.stderr.write("STATUS|tsv2-runtime|ok|compile_dl6 emitted program.tick through IncrementalRuntime; emitted recursive DRed plan; exact input edges and before/after sets match shared BFS outside clocks; count inside clocks|Node process RSS sampled after validation; in-memory libSQL store|DL_MEMCAP_MB limits Node old-space only; SQLite C heap and total RSS unenforced\n");
    process.stderr.write(`CSV,tsv2-runtime,${graph.nodes},${graph.edges.length},${before_count-after_count},${setup_ms.toFixed(3)},${retract_ms.toFixed(3)},${statements},${(process.memoryUsage().rss/1048576).toFixed(1)},N/A,N/A,0\n`);
  } finally { seam.db.close(); }
}

void main().catch((error: unknown) => {
  process.stderr.write(`${error instanceof Error ? error.stack : String(error)}\n`);
  process.exitCode = 1;
});
