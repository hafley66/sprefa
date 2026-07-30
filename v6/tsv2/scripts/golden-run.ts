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
 * SPREFA_TSV2_EMITTER_MODE is read by the emitted module itself at load time
 * (`naive` = the snapshot referee, anything else = incremental), so the mode is
 * selected by the environment of this process and nothing here passes it on.
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
import { rowValueFromSql } from "../runtime/rows.ts";
import type { IArrivalBatch, IBootStatement, IRowValue, ISqlSeam, IGenProgram } from "../runtime/types.ts";

type EmittedProgram = IGenProgram & {
  readonly boot: readonly IBootStatement[];
  readonly finalSelect: Record<string, string>;
};

function finalValueJson(value: unknown): string {
  if (typeof value === "bigint") return value.toString();
  if (typeof value === "number" || typeof value === "boolean") {
    return TickLogEmitter.valueText(value as IRowValue);
  }
  return TickLogEmitter.valueText(String(value));
}

function finalStateLine(rowsByRel: Record<string, readonly (readonly unknown[])[]>): string {
  const parts: string[] = [];
  for (const rel of Object.keys(rowsByRel).sort()) {
    const rows = rowsByRel[rel]!;
    if (rows.length === 0) continue;
    const rowTexts = rows.map((row) => `[${row.map(finalValueJson).join(",")}]`).sort();
    parts.push(`${JSON.stringify(rel)}:[${rowTexts.join(",")}]`);
  }
  return `{"final":{${parts.join(",")}}}`;
}

function readFinalState(seam: ISqlSeam, program: EmittedProgram): Observable<string> {
  const relNames = Object.keys(program.finalSelect);
  if (relNames.length === 0) return of(finalStateLine({}));
  return forkJoin(
    relNames.map((rel) =>
      seam.runner.execute(seam.db, program.finalSelect[rel]!).pipe(
        map((result) => ({
          rel,
          rows: result.rows.map((row) =>
            (program.relColumns[rel] ?? []).map((column, index) =>
              rowValueFromSql(program.relColumnTypes?.[rel]?.[index], row[column]),
            ),
          ),
        })),
      ),
    ),
  ).pipe(
    map((entries) => {
      const rowsByRel: Record<string, readonly (readonly unknown[])[]> = {};
      for (const entry of entries) rowsByRel[entry.rel] = entry.rows;
      return finalStateLine(rowsByRel);
    }),
  );
}

function main(): void {
  const [moduleName, schedulePath, ...flags] = process.argv.slice(2);
  if (moduleName === undefined || schedulePath === undefined) {
    process.stderr.write("usage: golden-run.ts <module> <schedule.json> [--final]\n");
    process.exitCode = 2;
    return;
  }
  const wantFinal = flags.includes("--final");
  const schedule = JSON.parse(readFileSync(schedulePath, "utf8")) as readonly IArrivalBatch[];

  void import(["..", "gen_emitted", `${moduleName}.ts`].join("/")).then((loaded: { program: EmittedProgram }) => {
    const program = loaded.program;
    const seam = ScratchStore.open(":memory:");
    ScratchStore.boot(seam, program.ddl)
      .pipe(
        concatMap(() => BootRunner.run(seam, program.boot)),
        concatMap(() => TickFold.run(program, seam, schedule).pipe(toArray())),
        concatMap((lines) =>
          wantFinal ? readFinalState(seam, program).pipe(map((line) => [...lines, line])) : of(lines),
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
