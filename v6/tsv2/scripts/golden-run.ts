/**
 * golden-run.ts — run ONE prolog-emitted module against ONE schedule file and
 * print the two graded legs: the tick log (one line per tick) and, with
 * `--final`, the final-state line.
 *
 * scripts/sweep.ts does this for the whole fixture corpus, driven by
 * out/manifest.json; scripts/run-emitted.ts does it for four names in a
 * hardcoded table. Neither can run a hand-written `.dl6` program against a
 * caller-supplied schedule, which is exactly what grading golden-flex at three
 * cardinalities needs. The grading LOGIC is not re-implemented here: the final
 * state encoder is the same shape sweep.ts writes, and `TickFold.run` is the
 * same runner, so a divergence between this and the sweep is a divergence in
 * the module, not in two copies of a comparison.
 *
 * Usage:
 *   node --experimental-transform-types scripts/golden-run.ts <module> <schedule.json> [--final]
 *
 * <module> is a file name under gen_emitted/ WITHOUT the .ts suffix; dynamic
 * import resolves relative to this package, which is why sweep.sh copies
 * compiled modules there rather than importing out/ directly.
 */

import { readFileSync } from "node:fs";

import { concatMap, forkJoin, map, of, toArray, type Observable } from "rxjs";

import { BootRunner } from "../runtime/2_boot.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import { TickLogEmitter } from "../runtime/ticklog.ts";
import { TickFold } from "../runtime/tickLoop.ts";
import { row_value_from_sql } from "../runtime/rows.ts";
import { decode_json_arrivals } from "../runtime/boundary.ts";
import type { IBootStatement, IRowColumnType, IRowValue, ISqlSeam, IGenProgram } from "../runtime/types.ts";

type EmittedProgram = IGenProgram & {
  readonly boot: readonly IBootStatement[];
  readonly final_select: Record<string, string>;
};

function final_value_json(value: unknown, type?: IRowColumnType): string {
  if (type === "bytes" && (value instanceof Uint8Array || value instanceof ArrayBuffer)) {
    return TickLogEmitter.value_text(value instanceof ArrayBuffer ? new Uint8Array(value) : value, type);
  }
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
  const parts: string[] = [];
  for (const rel of Object.keys(rows_by_rel).sort()) {
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
  const [module_name, schedule_path, ...flags] = process.argv.slice(2);
  if (module_name === undefined || schedule_path === undefined) {
    process.stderr.write("usage: golden-run.ts <module> <schedule.json> [--final]\n");
    process.exitCode = 2;
    return;
  }
  const want_final = flags.includes("--final");
  const schedule_json: unknown = JSON.parse(readFileSync(schedule_path, "utf8"));

  void import(["..", "gen_emitted", `${module_name}.ts`].join("/")).then((loaded: { program: EmittedProgram }) => {
    const program = loaded.program;
    const schedule = decode_json_arrivals(schedule_json, program.rel_column_types ?? {});
    const seam = ScratchStore.open(":memory:");
    ScratchStore.boot(seam, program.ddl)
      .pipe(
        concatMap(() => BootRunner.run(seam, program.boot)),
        concatMap(() => TickFold.run(program, seam, schedule).pipe(toArray())),
        concatMap((lines) =>
          want_final ? read_final_state(seam, program).pipe(map((line) => [...lines, line])) : of(lines),
        ),
      )
      .subscribe({
        next: (lines) => process.stdout.write(lines.map((line) => `${line}\n`).join("")),
        error: (failure: unknown) => {
          process.stderr.write(`${failure instanceof Error ? (failure.stack ?? failure.message) : String(failure)}\n`);
          process.exit(1);
        },
      });
  });
}

main();
