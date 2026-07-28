import { appendFileSync } from "node:fs";

import { firstValueFrom } from "rxjs";
import { stmt_counter } from "sprefa-store-engine/src/engine/counter.ts";

import { incrementalPlan, program } from "../gen/scale_generated.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import type {
  IArrivalBatch,
  IIncrementalLevelStatement,
  IIncrementalRelationPlan,
  ISqlSeam,
} from "../runtime/types.ts";

const BATCH_SIZE = 100;

type Shape = "s1" | "s2" | "s3";

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
  const left = Array.from({ length: rows }, (_, value) => ({
    rel: "left",
    sign: "add" as const,
    row: [`l${value}`] as const,
  }));
  const right = Array.from({ length: rows }, (_, value) => ({
    rel: "right",
    sign: "add" as const,
    row: [`r${value}`] as const,
  }));
  return [
    ...Array.from({ length: rows / BATCH_SIZE }, (_, tick) =>
      left.slice(tick * BATCH_SIZE, (tick + 1) * BATCH_SIZE)),
    ...Array.from({ length: rows / BATCH_SIZE }, (_, tick) =>
      right.slice(tick * BATCH_SIZE, (tick + 1) * BATCH_SIZE)),
  ];
}

async function runBoot(seam: ISqlSeam): Promise<void> {
  await firstValueFrom(ScratchStore.boot(seam, program.ddl));
  for (const statement of program.boot) {
    await firstValueFrom(
      seam.runner.execute(seam.db, { sql: statement.sql, args: [...statement.params] }),
    );
  }
}

async function explainLevel(
  seam: ISqlSeam,
  statement: IIncrementalLevelStatement,
): Promise<{ readonly headRel: string; readonly details: readonly string[] }> {
  const result = await firstValueFrom(
    seam.runner.execute(seam.db, `EXPLAIN QUERY PLAN ${statement.insertSql}`),
  );
  return {
    headRel: statement.headRel,
    details: result.rows.map((row) => String(row.detail)),
  };
}

async function explainBoundary(
  seam: ISqlSeam,
  relation: IIncrementalRelationPlan,
): Promise<{ readonly rel: string; readonly details: readonly string[] }> {
  const result = await firstValueFrom(
    seam.runner.execute(seam.db, `EXPLAIN QUERY PLAN ${relation.boundarySql}`),
  );
  return {
    rel: relation.rel,
    details: result.rows.map((row) => String(row.detail)),
  };
}

async function main(): Promise<void> {
  const [, , shapeArg, rowsArg, recordPath] = process.argv;
  if (shapeArg !== "s1" && shapeArg !== "s2" && shapeArg !== "s3") {
    throw new Error("p1-receipts: shape must be s1, s2, or s3");
  }
  const rows = Number(rowsArg);
  if (!Number.isInteger(rows) || rows < BATCH_SIZE || rows % BATCH_SIZE !== 0) {
    throw new Error("p1-receipts: rows must be a positive multiple of 100");
  }
  if (!incrementalPlan.safe) throw new Error(`p1-receipts: ${shapeArg} lowered as unsafe`);

  const explainSeam = ScratchStore.open(":memory:");
  await runBoot(explainSeam);
  const plans = await Promise.all(
    incrementalPlan.levels.map((statement) => explainLevel(explainSeam, statement)),
  );
  const boundaryPlans = await Promise.all(
    incrementalPlan.relations.map((relation) => explainBoundary(explainSeam, relation)),
  );
  explainSeam.db.close();

  const seam = ScratchStore.open(":memory:");
  await runBoot(seam);
  const statementCounts: number[] = [];
  for (const arrivals of scheduleFor(shapeArg, rows)) {
    stmt_counter.reset();
    await firstValueFrom(program.tick(seam, arrivals));
    statementCounts.push(stmt_counter.get());
  }
  const receipt = {
    shape: shapeArg,
    rows,
    tickCount: statementCounts.length,
    statementCounts: [...new Set(statementCounts)].sort((left, right) => left - right),
    plans,
    boundaryPlans,
  };
  const line = JSON.stringify(receipt);
  if (recordPath !== undefined) appendFileSync(recordPath, `${line}\n`);
  process.stdout.write(`${line}\n`);
}

void main().catch((failure: unknown) => {
  process.stderr.write(`${failure instanceof Error ? failure.stack : String(failure)}\n`);
  process.exitCode = 1;
});
