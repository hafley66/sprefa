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

import { catchError, concatMap, from, map, of, toArray, type Observable } from "rxjs";

import { ScratchStore } from "../runtime/scratchStore.ts";
import { TickFold } from "../runtime/tickLoop.ts";
import type { IArrivalBatch, ISqlSeam, IGenProgram } from "../runtime/types.ts";

const HERE = dirname(fileURLToPath(import.meta.url));
const COMPILE_OUT = join(HERE, "..", "..", "prolog", "compile", "out");

interface IManifestEntry {
  readonly name: string;
  readonly file: string;
  readonly bucket: "compiled" | "unsupported" | "crash";
  readonly reason: string;
}

// Local mirror of run-emitted.ts's own IEmittedBootStatement/EmittedProgram
// pair (that file does not export them) -- both scripts render the SAME
// emit_ts.pl output shape independently, matching the project's existing
// per-script local-type pattern (e.g. generated files' own IBootStatement).
interface IEmittedBootStatement {
  readonly sql: string;
  readonly params: readonly (string | number)[];
}

type EmittedProgram = IGenProgram & { readonly boot: readonly IEmittedBootStatement[] };

type RunBucket = "identical" | "wrong" | "run_error" | "no_oracle_log";

interface IFixtureRunResult {
  readonly name: string;
  readonly bucket: RunBucket;
  readonly detail: string;
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

function loadEmitted(name: string): Promise<EmittedProgram> {
  const specifier = ["..", "gen_emitted", `${name}.ts`].join("/");
  return import(specifier).then((loaded: { program: EmittedProgram }) => loaded.program);
}

function runBoot(seam: ISqlSeam, statements: readonly IEmittedBootStatement[]): Observable<unknown> {
  return statements.length === 0
    ? of(undefined)
    : from(statements).pipe(
        concatMap((statement) => seam.runner.execute(seam.db, { sql: statement.sql, args: [...statement.params] })),
        toArray(),
      );
}

function gradeAgainstOracle(name: string, actualLines: readonly string[]): IFixtureRunResult {
  const oracle = readOracleLines(name);
  if (oracle === null) {
    return { name, bucket: "no_oracle_log", detail: "oracle run threw (see oracle_dump.pl ORACLE_THROW output); nothing to diff" };
  }
  const identical = actualLines.length === oracle.length && actualLines.every((line, index) => line === oracle[index]);
  if (identical) return { name, bucket: "identical", detail: "" };
  const firstDiffIndex = actualLines.findIndex((line, index) => line !== oracle[index]);
  const excerptIndex = firstDiffIndex === -1 ? Math.min(actualLines.length, oracle.length) : firstDiffIndex;
  const actualExcerpt = actualLines[excerptIndex] ?? "<missing tick>";
  const oracleExcerpt = oracle[excerptIndex] ?? "<missing tick>";
  return { name, bucket: "wrong", detail: `first diff at line ${excerptIndex + 1}: actual=${actualExcerpt} oracle=${oracleExcerpt}` };
}

function runFixture(name: string): Observable<IFixtureRunResult> {
  return from(loadEmitted(name)).pipe(
    concatMap((program) => {
      const schedule = readSchedule(name);
      const seam = ScratchStore.open(":memory:");
      return ScratchStore.boot(seam, program.ddl).pipe(
        concatMap(() => runBoot(seam, program.boot)),
        concatMap(() => TickFold.run(program, seam, schedule).pipe(toArray())),
      );
    }),
    map((lines) => gradeAgainstOracle(name, lines)),
    catchError((error: unknown) =>
      of<IFixtureRunResult>({ name, bucket: "run_error", detail: error instanceof Error ? error.message : String(error) }),
    ),
  );
}

function summaryLine(results: readonly IFixtureRunResult[]): string {
  const countOf = (bucket: RunBucket): number => results.filter((result) => result.bucket === bucket).length;
  return `RUN total=${results.length} identical=${countOf("identical")} wrong=${countOf("wrong")} run_error=${countOf("run_error")} no_oracle_log=${countOf("no_oracle_log")}`;
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
      },
      error: (failure) => {
        process.stderr.write(`${failure instanceof Error ? failure.stack : String(failure)}\n`);
        process.exit(1);
      },
    });
}

main();
