/**
 * sweep.ts — Phase C driver (plans/2026-07-27-tsv2-compile-target-header.md,
 * PHASE C CONTRACT): runs every prolog-compiled fixture named in
 * v6/prolog/compile/out/manifest.json (written by v6/prolog/compile/sweep.pl)
 * against the phase-A runtime, replaying each fixture's OWN schedule
 * (out/<name>.schedule.json, a JSON rendering of the fixture's Schedule term)
 * and diffing the resulting tick log byte-for-byte against
 * v6/prolog/compile/out/<name>.oracle.jsonl (written by
 * v6/prolog/compile/oracle_dump.pl, run over the SAME schedule via
 * conformance/ticklog.pl -- never edited by this arc).
 *
 * Differs from scripts/run-emitted.ts in scope, not seam: run-emitted.ts is
 * the phase A/B two-fixture reconciliation runner with a hand-registered
 * lookup table; this walks the WHOLE compiled set the manifest names, one
 * fixture at a time (concatMap, one seam per fixture, never shared), so
 * neither script needed to change for the other to exist.
 *
 * Prerequisite: v6/tsv2/scripts/sweep.sh has already copied every compiled
 * fixture's emitted module into gen_emitted/<name>.ts -- dynamic import
 * resolves relative to THIS package, so a module still sitting only in
 * compile/out/ cannot be imported directly (its "../runtime/..." relative
 * imports would resolve against the wrong directory).
 *
 * Writes v6/prolog/compile/out/run-results.json (one record per compiled
 * fixture: bucket + a short diff/error detail) and prints a summary line.
 *
 * Usage: node --experimental-transform-types scripts/sweep.ts
 */

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { catchError, concatMap, forkJoin, from, map, of, toArray, type Observable } from "rxjs";

import { BootRunner } from "../runtime/2_boot.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import { TickLogEmitter } from "../runtime/ticklog.ts";
import { TickFold } from "../runtime/tickLoop.ts";
import { rowValueFromSql } from "../runtime/rows.ts";
import type { IArrivalBatch, IBootStatement, IRowColumnType, IRowValue, ISqlSeam, IGenProgram } from "../runtime/types.ts";

const HERE = dirname(fileURLToPath(import.meta.url));
const COMPILE_OUT = join(HERE, "..", "..", "prolog", "compile", "out");

interface IManifestEntry {
  readonly name: string;
  readonly file: string;
  readonly bucket: "compiled" | "unsupported" | "crash";
  readonly reason: string;
}

// The two extra fields emit_ts.pl adds beyond IGenProgram's five pinned
// names. `IBootStatement` and the loop that runs it are no longer per-script
// copies: both this file and run-emitted.ts had their own, and both bound
// `params` RAW, which is fail-first check (b) -- see runtime/2_boot.ts.
type EmittedProgram = IGenProgram & {
  readonly boot: readonly IBootStatement[];
  readonly finalSelect: Record<string, string>;
};

type RunBucket = "identical" | "wrong" | "run_error" | "no_oracle_log";

/** Final-state leg (EXPRESSION + AGGREGATE LIFT arc). Reported ALONGSIDE the
 *  tick-log bucket, never folded into it: the tick-log diff stays the gate
 *  every earlier arc was graded on, and this adds the grade an EMPTY-schedule
 *  fixture has no other way to earn (both sides print zero tick lines, which
 *  the tick-log diff calls IDENTICAL on no evidence -- SCOREBOARD.md Finding
 *  2's vacuous-pass class). `no_oracle_final` means the oracle run threw. */
type FinalBucket = "final_identical" | "final_wrong" | "no_oracle_final";

interface IFixtureRunResult {
  readonly name: string;
  readonly bucket: RunBucket;
  readonly detail: string;
  readonly finalBucket: FinalBucket;
  readonly finalDetail: string;
}

function readManifest(): readonly IManifestEntry[] {
  const text = readFileSync(join(COMPILE_OUT, "manifest.json"), "utf8");
  return JSON.parse(text) as readonly IManifestEntry[];
}

function readSchedule(name: string): readonly IArrivalBatch[] {
  const text = readFileSync(join(COMPILE_OUT, `${name}.schedule.json`), "utf8");
  return JSON.parse(text) as readonly IArrivalBatch[];
}

function readOracleLines(name: string): readonly string[] | null {
  try {
    const text = readFileSync(join(COMPILE_OUT, `${name}.oracle.jsonl`), "utf8");
    return text.split("\n").filter((line) => line.length > 0);
  } catch {
    return null;
  }
}

function readOracleFinalLine(name: string): string | null {
  try {
    const text = readFileSync(join(COMPILE_OUT, `${name}.oracle.final.jsonl`), "utf8");
    return text.split("\n").filter((line) => line.length > 0)[0] ?? null;
  } catch {
    return null;
  }
}

/** The oracle side encodes a value with ticklog.pl's `value_json/2`: an
 *  integer as a JSON number, a json value as canonical JSON text, everything
 *  else as a JSON string. The emitted side reads an INTEGER-affinity column
 *  back as a JS number and a TEXT-affinity column as a JS string (rows.ts's
 *  own note), so the SAME rule applied here is what makes a TEXT-collapsed
 *  integer ("12" stored in a TEXT column) show up as a diff instead of
 *  passing silently.
 *
 *  This now delegates to `TickLogEmitter.valueText`, the per-tick leg's own
 *  encoder, rather than restating the rule. The two legs HAD drifted: the
 *  json_ticklog_encoding regrade taught the per-tick leg to canonicalize
 *  object/array text and left this copy behind, which no corpus value
 *  exercised until a struct column arrived whose stored value IS canonical
 *  JSON text -- the tick log printed the object and the final state printed
 *  the same bytes wrapped in quotes. */
function finalValueJson(value: unknown, type?: IRowColumnType): string {
  if (typeof value === "bigint") return value.toString();
  if (typeof value === "number" || typeof value === "boolean") {
    return TickLogEmitter.valueText(value as IRowValue, type);
  }
  return TickLogEmitter.valueText(String(value), type);
}

function finalStateLine(
  rowsByRel: Record<string, readonly (readonly unknown[])[]>,
  relColumnTypes?: Readonly<Record<string, readonly IRowColumnType[]>>,
): string {
  const relNames = Object.keys(rowsByRel).sort();
  const parts: string[] = [];
  for (const rel of relNames) {
    const rows = rowsByRel[rel]!;
    if (rows.length === 0) continue;
    const types = relColumnTypes?.[rel];
    const rowTexts = rows
      .map((row) => `[${row.map((value, index) => finalValueJson(value, types?.[index])).join(",")}]`)
      .sort();
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
      return finalStateLine(rowsByRel, program.relColumnTypes);
    }),
  );
}

function gradeFinalState(name: string, actualLine: string): { bucket: FinalBucket; detail: string } {
  const oracleLine = readOracleFinalLine(name);
  if (oracleLine === null) return { bucket: "no_oracle_final", detail: "oracle run threw; no final state to diff" };
  if (oracleLine === actualLine) return { bucket: "final_identical", detail: "" };
  return { bucket: "final_wrong", detail: `actual=${actualLine.slice(0, 400)} oracle=${oracleLine.slice(0, 400)}` };
}

function loadEmitted(name: string): Promise<EmittedProgram> {
  const specifier = ["..", "gen_emitted", `${name}.ts`].join("/");
  return import(specifier).then((loaded: { program: EmittedProgram }) => loaded.program);
}

function gradeAgainstOracle(
  name: string,
  actualLines: readonly string[],
  finalGrade: { bucket: FinalBucket; detail: string },
): IFixtureRunResult {
  const oracle = readOracleLines(name);
  const withFinal = (bucket: RunBucket, detail: string): IFixtureRunResult => ({
    name,
    bucket,
    detail,
    finalBucket: finalGrade.bucket,
    finalDetail: finalGrade.detail,
  });
  if (oracle === null) {
    return withFinal("no_oracle_log", "oracle run threw (see oracle_dump.pl ORACLE_THROW output); nothing to diff");
  }
  const identical = actualLines.length === oracle.length && actualLines.every((line, index) => line === oracle[index]);
  if (identical) return withFinal("identical", "");
  const firstDiffIndex = actualLines.findIndex((line, index) => line !== oracle[index]);
  const excerptIndex = firstDiffIndex === -1 ? Math.min(actualLines.length, oracle.length) : firstDiffIndex;
  const actualExcerpt = actualLines[excerptIndex] ?? "<missing tick>";
  const oracleExcerpt = oracle[excerptIndex] ?? "<missing tick>";
  return withFinal("wrong", `first diff at line ${excerptIndex + 1}: actual=${actualExcerpt} oracle=${oracleExcerpt}`);
}

function runFixture(name: string): Observable<IFixtureRunResult> {
  return from(loadEmitted(name)).pipe(
    concatMap((program) => {
      const schedule = readSchedule(name);
      const seam = ScratchStore.open(":memory:");
      return ScratchStore.boot(seam, program.ddl).pipe(
        concatMap(() => BootRunner.run(seam, program.boot)),
        concatMap(() => TickFold.run(program, seam, schedule).pipe(toArray())),
        concatMap((lines) => readFinalState(seam, program).pipe(map((finalLine) => ({ lines, finalLine })))),
      );
    }),
    map(({ lines, finalLine }) => gradeAgainstOracle(name, lines, gradeFinalState(name, finalLine))),
    catchError((error: unknown) =>
      of<IFixtureRunResult>({
        name,
        bucket: "run_error",
        detail: error instanceof Error ? error.message : String(error),
        finalBucket: "final_wrong",
        finalDetail: "run threw before the final state could be read",
      }),
    ),
  );
}

function summaryLine(results: readonly IFixtureRunResult[]): string {
  const countOf = (bucket: RunBucket): number => results.filter((result) => result.bucket === bucket).length;
  return `RUN total=${results.length} identical=${countOf("identical")} wrong=${countOf("wrong")} run_error=${countOf("run_error")} no_oracle_log=${countOf("no_oracle_log")}`;
}

function finalSummaryLine(results: readonly IFixtureRunResult[]): string {
  const countOf = (bucket: FinalBucket): number => results.filter((result) => result.finalBucket === bucket).length;
  return `FINAL total=${results.length} final_identical=${countOf("final_identical")} final_wrong=${countOf("final_wrong")} no_oracle_final=${countOf("no_oracle_final")}`;
}

function main(): void {
  const manifest = readManifest();
  const compiledNames = manifest.filter((entry) => entry.bucket === "compiled").map((entry) => entry.name);
  from(compiledNames)
    .pipe(concatMap((name) => runFixture(name)), toArray())
    .subscribe({
      next: (results) => {
        writeFileSync(join(COMPILE_OUT, "run-results.json"), `${JSON.stringify(results, null, 2)}\n`);
        process.stdout.write(`${summaryLine(results)}\n`);
        for (const result of results) {
          if (result.bucket !== "identical") process.stdout.write(`  ${result.bucket.toUpperCase()} ${result.name} ${result.detail}\n`);
        }
        process.stdout.write(`${finalSummaryLine(results)}\n`);
        for (const result of results) {
          if (result.finalBucket !== "final_identical") {
            process.stdout.write(`  ${result.finalBucket.toUpperCase()} ${result.name} ${result.finalDetail}\n`);
          }
        }
      },
      error: (failure) => {
        process.stderr.write(`${failure instanceof Error ? failure.stack : String(failure)}\n`);
        process.exit(1);
      },
    });
}

main();
