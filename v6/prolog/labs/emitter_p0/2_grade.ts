import { execFileSync } from "node:child_process";
import {
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { firstValueFrom, toArray } from "rxjs";
import { stmt_counter } from "../../../sprefa-store/js/src/engine/counter.ts";
import { ScratchStore } from "../../../tsv2/runtime/scratchStore.ts";
import { TickFold } from "../../../tsv2/runtime/tickLoop.ts";

import { variant as semiNaiveInline } from "./generated/0_semi_naive_inline.ts";
import { variant as semiNaiveHelper } from "./generated/1_semi_naive_helper.ts";
import { variant as countIvmInline } from "./generated/2_count_ivm_inline.ts";
import { variant as countIvmHelper } from "./generated/3_count_ivm_helper.ts";
import { variant as distinctInline } from "./generated/4_distinct_inline.ts";
import { variant as distinctHelper } from "./generated/5_distinct_helper.ts";
import { variant as boundaryInline } from "./generated/6_boundary_inline.ts";
import { variant as boundaryHelper } from "./generated/7_boundary_helper.ts";
import {
  LAB_SCHEDULES,
  type IExplainReceipt,
  type ILabVariant,
  type IReceiptProgram,
} from "./1_runtime.ts";

interface IIdentityReceipt {
  readonly fixture: string;
  readonly ticks: number;
  readonly oracleBytes: number;
  readonly actualBytes: number;
  readonly identical: true;
}

interface IPlanReceipt {
  readonly fixture: string;
  readonly label: string;
  readonly deltaTable: string;
  readonly details: readonly string[];
  readonly deltaUsesSearch: true;
  readonly deltaUsesScan: false;
}

interface IVariantReceipt {
  readonly family: string;
  readonly spelling: string;
  readonly generatedFile: string;
  readonly generatedLinecount: number;
  readonly identity: readonly IIdentityReceipt[];
  readonly statementsPerTick: Readonly<Record<string, readonly number[]>>;
  readonly plans: readonly IPlanReceipt[];
}

const LAB_DIRECTORY = dirname(fileURLToPath(import.meta.url));
const WORKTREE_ROOT = resolve(LAB_DIRECTORY, "../../../..");
const TICKLOG_FILE = join(
  WORKTREE_ROOT,
  "v6/prolog/conformance/ticklog.pl",
);

const VARIANTS: readonly {
  readonly value: ILabVariant;
  readonly generatedFile: string;
}[] = [
  { value: semiNaiveInline, generatedFile: "generated/0_semi_naive_inline.ts" },
  { value: semiNaiveHelper, generatedFile: "generated/1_semi_naive_helper.ts" },
  { value: countIvmInline, generatedFile: "generated/2_count_ivm_inline.ts" },
  { value: countIvmHelper, generatedFile: "generated/3_count_ivm_helper.ts" },
  { value: distinctInline, generatedFile: "generated/4_distinct_inline.ts" },
  { value: distinctHelper, generatedFile: "generated/5_distinct_helper.ts" },
  { value: boundaryInline, generatedFile: "generated/6_boundary_inline.ts" },
  { value: boundaryHelper, generatedFile: "generated/7_boundary_helper.ts" },
];

function sourceLinecount(relativePath: string): number {
  const source = readFileSync(join(LAB_DIRECTORY, relativePath), "utf8");
  const withoutFinalNewline = source.endsWith("\n") ? source.slice(0, -1) : source;
  return withoutFinalNewline.length === 0
    ? 0
    : withoutFinalNewline.split("\n").length;
}

function oracleLog(fixtureName: string): string {
  return execFileSync(
    "swipl",
    ["-q", "-l", TICKLOG_FILE, "-g", `emit(${fixtureName})`, "-g", "halt"],
    { cwd: WORKTREE_ROOT, encoding: "utf8" },
  );
}

async function explainReceipts(
  program: IReceiptProgram,
  seam: ReturnType<typeof ScratchStore.open>,
): Promise<IPlanReceipt[]> {
  const receipts: IPlanReceipt[] = [];
  for (const explainReceipt of program.explainReceipts) {
    receipts.push(await explainReceiptForProgram(program, seam, explainReceipt));
  }
  return receipts;
}

async function explainReceiptForProgram(
  program: IReceiptProgram,
  seam: ReturnType<typeof ScratchStore.open>,
  explainReceipt: IExplainReceipt,
): Promise<IPlanReceipt> {
  const result = await seam.db.execute({
    sql: `EXPLAIN QUERY PLAN ${explainReceipt.sql}`,
    args: [...explainReceipt.args],
  });
  const details = result.rows.map((resultRow) => String(resultRow.detail));
  const deltaPlanName = explainReceipt.deltaPlanName ?? explainReceipt.deltaTable;
  const escapedPlanName = deltaPlanName.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const searchPattern = new RegExp(`^SEARCH ${escapedPlanName}\\b`);
  const scanPattern = new RegExp(`^SCAN ${escapedPlanName}\\b`);
  const deltaUsesSearch = details.some((detail) => searchPattern.test(detail));
  const deltaUsesScan = details.some((detail) => scanPattern.test(detail));
  if (!deltaUsesSearch || deltaUsesScan) {
    throw new Error(
      `${program.fixtureName}/${explainReceipt.label}: expected SEARCH and no SCAN for ${explainReceipt.deltaTable}; ${JSON.stringify(details)}`,
    );
  }
  return {
    fixture: program.fixtureName,
    label: explainReceipt.label,
    deltaTable: explainReceipt.deltaTable,
    details,
    deltaUsesSearch: true,
    deltaUsesScan: false,
  };
}

async function gradeProgram(
  program: IReceiptProgram,
  databasePath: string,
): Promise<{
  readonly identity: IIdentityReceipt;
  readonly plans: readonly IPlanReceipt[];
}> {
  const schedule = LAB_SCHEDULES[program.fixtureName];
  if (schedule === undefined) {
    throw new Error(`missing schedule for ${program.fixtureName}`);
  }
  const seam = ScratchStore.open(`file:${databasePath}`);
  try {
    await firstValueFrom(ScratchStore.boot(seam, program.ddl));
    const plans = await explainReceipts(program, seam);
    stmt_counter.reset();
    const actualLines = await firstValueFrom(
      TickFold.run(program, seam, schedule).pipe(toArray()),
    );
    const actual = actualLines.length === 0 ? "" : `${actualLines.join("\n")}\n`;
    const oracle = oracleLog(program.fixtureName);
    if (actual !== oracle) {
      throw new Error(
        `${program.fixtureName}: tick-log mismatch\nACTUAL\n${actual}\nORACLE\n${oracle}`,
      );
    }
    return {
      identity: {
        fixture: program.fixtureName,
        ticks: actualLines.length,
        oracleBytes: Buffer.byteLength(oracle),
        actualBytes: Buffer.byteLength(actual),
        identical: true,
      },
      plans,
    };
  } finally {
    seam.db.close();
  }
}

async function gradeVariant(
  variant: ILabVariant,
  generatedFile: string,
  scratchDirectory: string,
): Promise<IVariantReceipt> {
  const identity: IIdentityReceipt[] = [];
  const plans: IPlanReceipt[] = [];
  const statementsPerTick: Record<string, readonly number[]> = {};
  for (const program of variant.programs) {
    const databasePath = join(
      scratchDirectory,
      `${variant.family}-${variant.spelling}-${program.fixtureName}.sqlite3`,
    );
    const programReceipt = await gradeProgram(program, databasePath);
    identity.push(programReceipt.identity);
    plans.push(...programReceipt.plans);
    statementsPerTick[program.fixtureName] = [...program.statementCounts];
  }
  return {
    family: variant.family,
    spelling: variant.spelling,
    generatedFile,
    generatedLinecount: sourceLinecount(generatedFile),
    identity,
    statementsPerTick,
    plans,
  };
}

async function main(): Promise<void> {
  const scratchDirectory = mkdtempSync(join(tmpdir(), "sprefa-emitter-p0-"));
  try {
    const receipts: IVariantReceipt[] = [];
    for (const variant of VARIANTS) {
      receipts.push(
        await gradeVariant(
          variant.value,
          variant.generatedFile,
          scratchDirectory,
        ),
      );
    }
    const receiptFile = join(LAB_DIRECTORY, "receipts.json");
    writeFileSync(receiptFile, `${JSON.stringify(receipts, null, 2)}\n`);
    process.stdout.write(`${JSON.stringify(receipts, null, 2)}\n`);
  } finally {
    rmSync(scratchDirectory, { recursive: true, force: true });
  }
}

void main().catch((failure: unknown) => {
  process.stderr.write(
    `${failure instanceof Error ? failure.stack : String(failure)}\n`,
  );
  process.exitCode = 1;
});
