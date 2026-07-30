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

function valueJson(value: unknown): string {
  if (typeof value === "bigint") return value.toString();
  if (typeof value === "number" && Number.isInteger(value)) return `${value}`;
  return TickLogEmitter.valueText(String(value) as IRowValue);
}

function finalStateLine(
  rowsByRel: Readonly<Record<string, readonly (readonly unknown[])[]>>,
): string {
  const parts: string[] = [];
  for (const rel of Object.keys(rowsByRel).sort()) {
    const rows = rowsByRel[rel] ?? [];
    if (rows.length === 0) continue;
    const rowTexts = rows
      .map((row) => `[${row.map(valueJson).join(",")}]`)
      .sort();
    parts.push(`${JSON.stringify(rel)}:[${rowTexts.join(",")}]`);
  }
  return `{"final":{${parts.join(",")}}}`;
}

async function readFinal(
  program: IServedProgram,
  seam: ReturnType<typeof ScratchStore.open>,
): Promise<string> {
  const entries = await Promise.all(
    Object.keys(program.finalSelect).map(async (rel) => {
      const result = await firstValueFrom(
        seam.runner.execute(seam.db, program.finalSelect[rel]!),
      );
      const columns = program.relColumns[rel] ?? [];
      return {
        rel,
        rows: result.rows.map((row) => columns.map((column) => row[column])),
      };
    }),
  );
  return finalStateLine(
    Object.fromEntries(entries.map((entry) => [entry.rel, entry.rows])),
  );
}

async function main(): Promise<void> {
  const [moduleFile, scheduleFile] = process.argv.slice(2);
  if (moduleFile === undefined || scheduleFile === undefined) {
    process.stderr.write(
      "usage: node --experimental-transform-types 4_ghcacher-tick-golden.ts <module.ts> <schedule.json>\n",
    );
    process.exitCode = 2;
    return;
  }

  const loaded = (await import(
    pathToFileURL(resolve(moduleFile)).href
  )) as { readonly program: IServedProgram };
  const schedule = JSON.parse(
    readFileSync(scheduleFile, "utf8"),
  ) as readonly IArrivalBatch[];
  const seam = ScratchStore.open(":memory:");
  try {
    await firstValueFrom(ScratchStore.boot(seam, loaded.program.ddl));
    await firstValueFrom(BootRunner.run(seam, loaded.program.boot));
    const tickLines = await firstValueFrom(
      TickFold.run(loaded.program, seam, schedule).pipe(toArray()),
    );
    for (const line of tickLines) process.stdout.write(`${line}\n`);
    process.stdout.write(`${await readFinal(loaded.program, seam)}\n`);
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
