import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

import { firstValueFrom, toArray } from "rxjs";

import { BootRunner } from "../runtime/2_boot.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import { TickLogEmitter } from "../runtime/ticklog.ts";
import { TickFold } from "../runtime/tickLoop.ts";
import type {
  IArrivalBatch,
  IRowValue,
  IServedProgram,
} from "../runtime/types.ts";

function value_json(value: unknown): string {
  if (typeof value === "bigint") return value.toString();
  if (typeof value === "number" && Number.isInteger(value)) return `${value}`;
  return TickLogEmitter.value_text(String(value) as IRowValue);
}

function final_state_line(
  rows_by_rel: Readonly<Record<string, readonly (readonly unknown[])[]>>,
): string {
  const parts: string[] = [];
  for (const rel of Object.keys(rows_by_rel).sort()) {
    const rows = rows_by_rel[rel] ?? [];
    if (rows.length === 0) continue;
    const row_texts = rows
      .map((row) => `[${row.map(value_json).join(",")}]`)
      .sort();
    parts.push(`${JSON.stringify(rel)}:[${row_texts.join(",")}]`);
  }
  return `{"final":{${parts.join(",")}}}`;
}

async function read_final(
  program: IServedProgram,
  seam: ReturnType<typeof ScratchStore.open>,
): Promise<string> {
  const entries = await Promise.all(
    Object.keys(program.final_select).map(async (rel) => {
      const result = await firstValueFrom(
        seam.runner.execute(seam.db, program.final_select[rel]!),
      );
      const columns = program.rel_columns[rel] ?? [];
      return {
        rel,
        rows: result.rows.map((row) => columns.map((column) => row[column])),
      };
    }),
  );
  return final_state_line(
    Object.fromEntries(entries.map((entry) => [entry.rel, entry.rows])),
  );
}

async function main(): Promise<void> {
  const [module_file, schedule_file] = process.argv.slice(2);
  if (module_file === undefined || schedule_file === undefined) {
    process.stderr.write(
      "usage: node --experimental-transform-types 4_ghcacher-tick-golden.ts <module.ts> <schedule.json>\n",
    );
    process.exitCode = 2;
    return;
  }

  const loaded = (await import(
    pathToFileURL(resolve(module_file)).href
  )) as { readonly program: IServedProgram };
  const schedule = JSON.parse(
    readFileSync(schedule_file, "utf8"),
  ) as readonly IArrivalBatch[];
  const seam = ScratchStore.open(":memory:");
  try {
    await firstValueFrom(ScratchStore.boot(seam, loaded.program.ddl));
    await firstValueFrom(BootRunner.run(seam, loaded.program.boot));
    const tick_lines = await firstValueFrom(
      TickFold.run(loaded.program, seam, schedule).pipe(toArray()),
    );
    for (const line of tick_lines) process.stdout.write(`${line}\n`);
    process.stdout.write(`${await read_final(loaded.program, seam)}\n`);
  } finally {
    seam.db.close();
  }
}

void main().catch((failure: unknown) => {
  process.stderr.write(
    `${failure instanceof Error ? failure.stack : String(failure)}\n`,
  );
  process.exitCode = 1;
});
