import { appendFileSync } from "node:fs";

import { firstValueFrom } from "rxjs";
import { stmt_counter } from "sprefa-store-engine/src/engine/counter.ts";

import { incrementalPlan, program } from "../gen/scale_generated.ts";
import {
  incrementalPlan as switchIncrementalPlan,
  program as switchProgram,
} from "../gen_emitted/switch_as_keyed_replace.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import type {
  IArrivalBatch,
  IGenProgram,
  IIncrementalEdgeStatement,
  IIncrementalLevelStatement,
  IIncrementalProgramPlan,
  IIncrementalRelationPlan,
  ISqlSeam,
} from "../runtime/types.ts";

const BATCH_SIZE = 100;

type Shape = "s1" | "s2" | "s3" | "p2-switch";

function scheduleFor(shape: Shape, rows: number): readonly IArrivalBatch[] {
  if (shape === "p2-switch") {
    return [
      [{ rel: "route_change", sign: "add", row: ["session_one", "settings"] }],
      [{ rel: "route_change", sign: "add", row: ["session_one", "profile"] }],
    ];
  }
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

type ProgramWithBoot = IGenProgram & {
  readonly boot: readonly {
    readonly sql: string;
    readonly params: readonly (string | number)[];
  }[];
};

async function runBoot(seam: ISqlSeam, selectedProgram: ProgramWithBoot): Promise<void> {
  await firstValueFrom(ScratchStore.boot(seam, selectedProgram.ddl));
  for (const statement of selectedProgram.boot) {
    await firstValueFrom(
      seam.runner.execute(seam.db, { sql: statement.sql, args: [...statement.params] }),
    );
  }
}

async function explainEdge(
  seam: ISqlSeam,
  statement: IIncrementalEdgeStatement,
): Promise<{ readonly headRel: string; readonly details: readonly string[] }> {
  const result = await firstValueFrom(
    seam.runner.execute(seam.db, `EXPLAIN QUERY PLAN ${statement.projectSql}`),
  );
  return {
    headRel: statement.headRel,
    details: result.rows.map((row) => String(row.detail)),
  };
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
  if (
    shapeArg !== "s1" &&
    shapeArg !== "s2" &&
    shapeArg !== "s3" &&
    shapeArg !== "p2-switch"
  ) {
    throw new Error("p1-receipts: shape must be s1, s2, s3, or p2-switch");
  }
  const rows = Number(rowsArg);
  if (
    !Number.isInteger(rows) ||
    rows < BATCH_SIZE ||
    rows % BATCH_SIZE !== 0
  ) {
    throw new Error("p1-receipts: rows must be a positive multiple of 100");
  }
  const selectedPlan: IIncrementalProgramPlan =
    shapeArg === "p2-switch" ? switchIncrementalPlan : incrementalPlan;
  const selectedProgram: ProgramWithBoot =
    shapeArg === "p2-switch" ? switchProgram : program;
  if (!selectedPlan.safe) throw new Error(`p1-receipts: ${shapeArg} lowered as unsafe`);

  const explainSeam = ScratchStore.open(":memory:");
  await runBoot(explainSeam, selectedProgram);
  const plans = await Promise.all(
    selectedPlan.levels.map((statement) => explainLevel(explainSeam, statement)),
  );
  const edgePlans = await Promise.all(
    selectedPlan.edges.map((statement) => explainEdge(explainSeam, statement)),
  );
  const boundaryPlans = await Promise.all(
    selectedPlan.relations.map((relation) => explainBoundary(explainSeam, relation)),
  );
  const frontierDetails = [...plans, ...edgePlans]
    .flatMap((plan) => plan.details)
    .filter((detail) => detail.includes("__frontier_"));
  if (
    frontierDetails.length === 0 ||
    frontierDetails.some((detail) => detail.startsWith("SCAN ")) ||
    !frontierDetails.some((detail) => detail.startsWith("SEARCH "))
  ) {
    throw new Error(`p1-receipts: frontier plan is not indexed: ${frontierDetails.join(" | ")}`);
  }
  explainSeam.db.close();

  const seam = ScratchStore.open(":memory:");
  await runBoot(seam, selectedProgram);
  const statementCounts: number[] = [];
  const drainStatementCounts: number[] = [];
  let lastCarryPending = false;
  let drainCount = 0;
  const drain = async (): Promise<void> => {
    while (lastCarryPending) {
      if (drainCount >= 100) throw new Error("p1-receipts: drain overflow");
      stmt_counter.reset();
      const deltas = await firstValueFrom(selectedProgram.tick(seam, []));
      drainStatementCounts.push(stmt_counter.get());
      lastCarryPending = deltas.carryPending;
      drainCount += 1;
    }
  };
  for (const arrivals of scheduleFor(shapeArg, rows)) {
    stmt_counter.reset();
    const deltas = await firstValueFrom(selectedProgram.tick(seam, arrivals));
    statementCounts.push(stmt_counter.get());
    lastCarryPending = deltas.carryPending;
    if (shapeArg === "p2-switch") await drain();
  }
  await drain();
  const receipt = {
    shape: shapeArg,
    rows,
    tickCount: statementCounts.length,
    statementCounts: [...new Set(statementCounts)].sort((left, right) => left - right),
    drainTickCount: drainStatementCounts.length,
    drainStatementCounts: [...new Set(drainStatementCounts)].sort(
      (left, right) => left - right,
    ),
    plans,
    edgePlans,
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
