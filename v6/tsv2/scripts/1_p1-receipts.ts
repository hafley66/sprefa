import { appendFileSync } from "node:fs";

import { firstValueFrom } from "rxjs";
import { stmt_counter } from "sprefa-store-engine/src/engine/counter.ts";

import { incremental_plan, program } from "../gen/scale_generated.ts";
import {
  incremental_plan as switch_incremental_plan,
  program as switch_program,
} from "../gen_emitted/switch_as_keyed_replace.ts";
import {
  incremental_plan as retraction_incremental_plan,
  program as retraction_program,
} from "../gen_emitted/retraction_only_tick_retracts_level_view.ts";
import {
  incremental_plan as shared_incremental_plan,
  program as shared_program,
} from "../gen_emitted/shared_demand_refcount.ts";
import {
  incremental_plan as negative_incremental_plan,
  program as negative_program,
} from "../gen_emitted/merge_policy.ts";
import {
  incremental_plan as edge_carry_incremental_plan,
  program as edge_carry_program,
} from "../gen_emitted/edge_chain_hops_tick_per_stage.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import type {
  IArrivalBatch,
  IBootStatement,
  IGenProgram,
  IIncrementalEdgeStatement,
  IIncrementalLevelStatement,
  IIncrementalProgramPlan,
  IIncrementalRelationPlan,
  ISqlSeam,
} from "../runtime/types.ts";

const BATCH_SIZE = 100;

type Shape =
  | "s1"
  | "s2"
  | "s3"
  | "p2-switch"
  | "p3-retraction"
  | "p3-shared"
  | "p3-negative"
  | "p2-edge-carry";

function schedule_for(shape: Shape, rows: number): readonly IArrivalBatch[] {
  if (shape === "p2-edge-carry") {
    return [
      Array.from({ length: rows }, (_, index) => ({
        rel: "source_ev",
        sign: "add" as const,
        row: [`item_${index}`] as const,
      })),
    ];
  }
  if (shape === "p2-switch") {
    return [
      [{ rel: "route_change", sign: "add", row: ["session_one", "settings"] }],
      [{ rel: "route_change", sign: "add", row: ["session_one", "profile"] }],
    ];
  }
  if (shape === "p3-retraction") {
    return [
      [
        { rel: "source_row", sign: "add", row: ["alpha"] },
        { rel: "source_row", sign: "add", row: ["beta"] },
      ],
      [
        { rel: "source_row", sign: "del", row: ["alpha"] },
        { rel: "source_row", sign: "del", row: ["beta"] },
      ],
    ];
  }
  if (shape === "p3-shared") {
    return [
      [
        { rel: "open_feed", sign: "add", row: ["session_one", "alpha"] },
        { rel: "open_feed", sign: "add", row: ["session_two", "alpha"] },
      ],
      [{ rel: "open_feed", sign: "del", row: ["session_one", "alpha"] }],
      [{ rel: "open_feed", sign: "del", row: ["session_two", "alpha"] }],
    ];
  }
  if (shape === "p3-negative") {
    return [
      [
        { rel: "open_request", sign: "add", row: ["session_one", "tab_a"] },
        { rel: "open_request", sign: "add", row: ["session_one", "tab_b"] },
      ],
      [{ rel: "close_request", sign: "add", row: ["session_one", "tab_a"] }],
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
  readonly boot: readonly IBootStatement[];
};

async function run_boot(seam: ISqlSeam, selected_program: ProgramWithBoot): Promise<void> {
  await firstValueFrom(ScratchStore.boot(seam, selected_program.ddl));
  for (const statement of selected_program.boot) {
    await firstValueFrom(
      seam.runner.execute(seam.db, { sql: statement.sql, args: [...statement.params] }),
    );
  }
}

async function explain_edge(
  seam: ISqlSeam,
  statement: IIncrementalEdgeStatement,
): Promise<{ readonly head_rel: string; readonly details: readonly string[] }> {
  const result = await firstValueFrom(
    seam.runner.execute(seam.db, `EXPLAIN QUERY PLAN ${statement.project_sql}`),
  );
  return {
    head_rel: statement.head_rel,
    details: result.rows.map((row) => String(row.detail)),
  };
}

async function explain_level(
  seam: ISqlSeam,
  statement: IIncrementalLevelStatement,
): Promise<{ readonly head_rel: string; readonly details: readonly string[] }> {
  const result = await firstValueFrom(
    seam.runner.execute(seam.db, `EXPLAIN QUERY PLAN ${statement.insert_sql}`),
  );
  return {
    head_rel: statement.head_rel,
    details: result.rows.map((row) => String(row.detail)),
  };
}

async function explain_ref_count(
  seam: ISqlSeam,
  statement: IIncrementalLevelStatement,
): Promise<{
  readonly head_rel: string;
  readonly statements: readonly {
    readonly index: number;
    readonly details: readonly string[];
  }[];
}> {
  // supportSql is null exactly on an AGGREGATE level statement, which is
  // maintained by the group-scoped plan instead (IAggregateLevelPlan). This
  // receipts script only walks the scale_generated program, which has no
  // aggregate heads, so an empty list here is unreachable in practice and the
  // narrowing is the honest way to say so rather than a `!`.
  const statements = await Promise.all(
    (statement.support_sql ?? []).map(async (sql, index) => {
      const result = await firstValueFrom(
        seam.runner.execute(seam.db, `EXPLAIN QUERY PLAN ${sql}`),
      );
      return { index, details: result.rows.map((row) => String(row.detail)) };
    }),
  );
  return { head_rel: statement.head_rel, statements };
}

async function explain_boundary(
  seam: ISqlSeam,
  relation: IIncrementalRelationPlan,
): Promise<{ readonly rel: string; readonly details: readonly string[] }> {
  const result = await firstValueFrom(
    seam.runner.execute(seam.db, `EXPLAIN QUERY PLAN ${relation.boundary_sql}`),
  );
  return {
    rel: relation.rel,
    details: result.rows.map((row) => String(row.detail)),
  };
}

async function main(): Promise<void> {
  const [, , shape_arg, rows_arg, record_path] = process.argv;
  if (
    shape_arg !== "s1" &&
    shape_arg !== "s2" &&
    shape_arg !== "s3" &&
    shape_arg !== "p2-switch" &&
    shape_arg !== "p3-retraction" &&
    shape_arg !== "p3-shared" &&
    shape_arg !== "p3-negative" &&
    shape_arg !== "p2-edge-carry"
  ) {
    throw new Error("p1-receipts: unknown receipt shape");
  }
  const rows = Number(rows_arg);
  if (
    !Number.isInteger(rows) ||
    rows < BATCH_SIZE ||
    rows % BATCH_SIZE !== 0
  ) {
    throw new Error("p1-receipts: rows must be a positive multiple of 100");
  }
  const selected = shape_arg === "p2-switch"
    ? { plan: switch_incremental_plan, program: switch_program }
    : shape_arg === "p2-edge-carry"
    ? { plan: edge_carry_incremental_plan, program: edge_carry_program }
    : shape_arg === "p3-retraction"
    ? { plan: retraction_incremental_plan, program: retraction_program }
    : shape_arg === "p3-shared"
    ? { plan: shared_incremental_plan, program: shared_program }
    : shape_arg === "p3-negative"
    ? { plan: negative_incremental_plan, program: negative_program }
    : { plan: incremental_plan, program };
  const selected_plan: IIncrementalProgramPlan = selected.plan;
  const selected_program: ProgramWithBoot = selected.program;

  const explain_seam = ScratchStore.open(":memory:");
  await run_boot(explain_seam, selected_program);
  const plans = await Promise.all(
    selected_plan.levels.map((statement) => explain_level(explain_seam, statement)),
  );
  const ref_count_plans = await Promise.all(
    selected_plan.levels.map((statement) => explain_ref_count(explain_seam, statement)),
  );
  const edge_plans = await Promise.all(
    selected_plan.edges.map((statement) => explain_edge(explain_seam, statement)),
  );
  const boundary_plans = await Promise.all(
    selected_plan.relations.map((relation) => explain_boundary(explain_seam, relation)),
  );
  const frontier_details = [...plans, ...edge_plans]
    .flatMap((plan) => plan.details)
    .filter((detail) => detail.includes("__frontier_"));
  if (
    frontier_details.length === 0 ||
    frontier_details.some((detail) => detail.startsWith("SCAN ")) ||
    !frontier_details.some((detail) => detail.startsWith("SEARCH "))
  ) {
    throw new Error(`p1-receipts: frontier plan is not indexed: ${frontier_details.join(" | ")}`);
  }
  explain_seam.db.close();

  const seam = ScratchStore.open(":memory:");
  await run_boot(seam, selected_program);
  const statement_counts: number[] = [];
  const drain_statement_counts: number[] = [];
  let last_carry_pending = false;
  let drain_count = 0;
  const drain = async (): Promise<void> => {
    while (last_carry_pending) {
      if (drain_count >= 100) throw new Error("p1-receipts: drain overflow");
      stmt_counter.reset();
      const deltas = await firstValueFrom(selected_program.tick(seam, []));
      drain_statement_counts.push(stmt_counter.get());
      last_carry_pending = deltas.carry_pending;
      drain_count += 1;
    }
  };
  for (const arrivals of schedule_for(shape_arg, rows)) {
    stmt_counter.reset();
    const deltas = await firstValueFrom(selected_program.tick(seam, arrivals));
    statement_counts.push(stmt_counter.get());
    last_carry_pending = deltas.carry_pending;
    if (shape_arg === "p2-switch" || shape_arg === "p2-edge-carry") await drain();
  }
  await drain();
  const receipt = {
    shape: shape_arg,
    rows,
    tick_count: statement_counts.length,
    statement_counts: [...new Set(statement_counts)].sort((left, right) => left - right),
    drain_tick_count: drain_statement_counts.length,
    drain_statement_counts: [...new Set(drain_statement_counts)].sort(
      (left, right) => left - right,
    ),
    plans,
    edge_plans,
    boundary_plans,
    ref_count_plans,
    reconcile_every_tick: selected_plan.reconcile_every_tick,
    retraction_guard: selected_plan.retraction_guard,
  };
  const line = JSON.stringify(receipt);
  if (record_path !== undefined) appendFileSync(record_path, `${line}\n`);
  process.stdout.write(`${line}\n`);
}

void main().catch((failure: unknown) => {
  process.stderr.write(`${failure instanceof Error ? failure.stack : String(failure)}\n`);
  process.exitCode = 1;
});
