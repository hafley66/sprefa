/**
 * run_case.ts — one-program twin of scripts/sweep.ts for the break-hunt lab.
 *
 * Copied into v6/tsv2/gen_emitted/ before it runs, so its relative imports
 * and the emitted module's own "../runtime/..." specifiers resolve against the
 * same package. Prints the emitted module's tick log plus the final-state
 * line, in the byte format conformance/ticklog.pl prints, so the two doors
 * diff directly.
 *
 * Usage (from v6/tsv2): node --experimental-transform-types
 *   gen_emitted/run_case.ts <module_name> <schedule.json>
 */

import { readFileSync } from "node:fs";

import { concatMap, forkJoin, map, of, toArray, type Observable } from "rxjs";

import { ScratchStore } from "../runtime/scratchStore.ts";
import { TickFold } from "../runtime/tickLoop.ts";
import { TickLogEmitter } from "../runtime/ticklog.ts";
import { BootRunner } from "../runtime/2_boot.ts";
import { row_value_from_sql } from "../runtime/rows.ts";
import type {
  IArrivalBatch,
  IBootStatement,
  IGenProgram,
  IRowColumnType,
  IRowValue,
  ISqlSeam,
} from "../runtime/types.ts";

type EmittedProgram = IGenProgram & {
  readonly boot: readonly IBootStatement[];
  readonly final_select: Record<string, string>;
};

function final_value_json(value: unknown, type?: IRowColumnType): string {
  if (typeof value === "bigint") return value.toString();
  if (typeof value === "number" || typeof value === "boolean") {
    return TickLogEmitter.value_text(value as IRowValue, type);
  }
  return TickLogEmitter.value_text(String(value), type);
}

function final_state_line(
  rows_by_rel: Record<string, readonly (readonly unknown[])[]>,
  rel_column_types?: Readonly<Record<string, readonly IRowColumnType[]>>,
): string {
  const rel_names = Object.keys(rows_by_rel).sort();
  const parts: string[] = [];
  for (const rel of rel_names) {
    const rows = rows_by_rel[rel]!;
    if (rows.length === 0) continue;
    const types = rel_column_types?.[rel];
    const row_texts = rows
      .map((row) => `[${row.map((value, index) => final_value_json(value, types?.[index])).join(",")}]`)
      .sort();
    parts.push(`${JSON.stringify(rel)}:[${row_texts.join(",")}]`);
  }
  return `{"final":{${parts.join(",")}}}`;
}

function read_final_state(seam: ISqlSeam, program: EmittedProgram): Observable<string> {
  const rel_names = Object.keys(program.final_select);
  if (rel_names.length === 0) return of(final_state_line({}));
  return forkJoin(
    rel_names.map((rel) =>
      seam.runner.execute(seam.db, program.final_select[rel]!).pipe(
        map((result) => ({
          rel,
          rows: result.rows.map((row) =>
            (program.rel_columns[rel] ?? []).map((column, index) =>
              row_value_from_sql(program.rel_column_types?.[rel]?.[index], row[column]),
            ),
          ),
        })),
      ),
    ),
  ).pipe(
    map((entries) => {
      const rows_by_rel: Record<string, readonly (readonly unknown[])[]> = {};
      for (const entry of entries) rows_by_rel[entry.rel] = entry.rows;
      return final_state_line(rows_by_rel, program.rel_column_types);
    }),
  );
}

function main(): void {
  const module_name = process.argv[2];
  const schedule_path = process.argv[3];
  if (module_name === undefined || schedule_path === undefined) {
    process.stderr.write("usage: run_case.ts <module_name> <schedule.json>\n");
    process.exitCode = 2;
    return;
  }
  const schedule = JSON.parse(readFileSync(schedule_path, "utf8")) as readonly IArrivalBatch[];
  void import(["..", "gen_emitted", `${module_name}.ts`].join("/"))
    .then((loaded: { program: EmittedProgram }) => loaded.program)
    .then((program) => {
      const seam = ScratchStore.open(":memory:");
      return new Promise<void>((resolve, reject) => {
        ScratchStore.boot(seam, program.ddl)
          .pipe(
            concatMap(() => BootRunner.run(seam, program.boot)),
            concatMap(() => TickFold.run(program, seam, schedule).pipe(toArray())),
            concatMap((lines) => read_final_state(seam, program).pipe(map((final_line) => ({ lines, final_line })))),
          )
          .subscribe({
            next: ({ lines, final_line }) => {
              for (const line of lines) process.stdout.write(`${line}\n`);
              process.stdout.write(`${final_line}\n`);
            },
            error: (failure: unknown) => reject(failure),
            complete: () => resolve(),
          });
      });
    })
    .catch((failure: unknown) => {
      process.stderr.write(`EMITTED_THROW ${failure instanceof Error ? failure.message : String(failure)}\n`);
      process.exitCode = 1;
    });
}

main();
