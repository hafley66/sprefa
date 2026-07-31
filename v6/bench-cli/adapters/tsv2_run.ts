/**
 * tsv2_run.ts — the tsv2 engine as a CONTRACT.md-conformant executable.
 *
 * Seam is deliberately the SAME chain v6/tsv2/scripts/golden-run.ts and
 * scripts/sweep.ts already use -- ScratchStore.open -> ScratchStore.boot(ddl)
 * -> BootRunner.run(boot) -> TickFold.run(program, seam, schedule) -- so this
 * adapter measures the graded engine and not a second one written beside it.
 * It is a separate file rather than a flag on golden-run.ts because
 * golden-run.ts resolves its module out of v6/tsv2/gen_emitted/, which this
 * lane is fenced out of; here the compiled module lives in ../out/ and reaches
 * the runtime through the committed ../runtime symlink, byte-untouched.
 *
 * The one manual `.subscribe()` in THIS script, same standing as every other
 * one-shot CLI entry point under v6/tsv2/scripts (v6/tools/one-subscribe.sh
 * scans dl/src and tsv2/serve, neither of which this is).
 *
 * Usage:
 *   node --experimental-transform-types adapters/tsv2_run.ts \
 *     --program <compiled.ts> --schedule <s.json> --db <path> --perf-out <p.json>
 *
 * `--program` here is the COMPILED module (adapters/tsv2.sh runs compile_dl6.sh
 * first and passes the result); the .dl6 text never reaches this file. Compile
 * time is measured by the shell wrapper and reported as `compile_ms`, because
 * CONTRACT.md section 2.4 puts compile outside `wall_ms` on purpose.
 *
 * SABOTAGE RECEIPT (bench.sh's referee is real, not decorative). Changing the
 * stdout loop below to `lines.slice(0, -1)` -- one tick short, everything else
 * byte-perfect -- and re-running `BENCH_CASES=match_classify bash bench.sh`
 * flipped the row to:
 *
 *     oracle   reference   wall(median)=114
 *     tsv2     wrong
 *     BENCH-CLI timed=0 disqualified=1 hash-agreement=OK
 *
 * The verdict went `identical` -> `wrong` AND the wall column went blank: a
 * disqualified engine is not timed, which is the whole point of the v1
 * asymmetry rule. Reverted immediately after.
 */

import { readFileSync, statSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

import { concatMap, forkJoin, map, of, toArray, type Observable } from "rxjs";

import { stmt_counter } from "sprefa-store-engine/src/engine/counter.ts";

import { BootRunner } from "../runtime/2_boot.ts";
import { rowValueFromSql } from "../runtime/rows.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import { TickLogEmitter } from "../runtime/ticklog.ts";
import { TickFold } from "../runtime/tickLoop.ts";
import type {
  IArrivalBatch,
  IBootStatement,
  IGenProgram,
  IRowColumnType,
  IRowValue,
  ISqlSeam,
} from "../runtime/types.ts";

/** emit_ts.pl adds `boot` and `finalSelect` beyond IGenProgram's five pinned names. */
type EmittedProgram = IGenProgram & {
  readonly boot: readonly IBootStatement[];
  readonly finalSelect: Record<string, string>;
};

interface IArgs {
  readonly program: string;
  readonly schedule: string;
  readonly db: string;
  readonly perfOut: string;
  readonly compileMs: number | "N/A";
}

function parseArgs(argv: readonly string[]): IArgs | null {
  const flags = new Map<string, string>();
  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index];
    const value = argv[index + 1];
    if (key === undefined || value === undefined || !key.startsWith("--")) return null;
    flags.set(key.slice(2), value);
  }
  const program = flags.get("program");
  const schedule = flags.get("schedule");
  const perfOut = flags.get("perf-out");
  if (program === undefined || schedule === undefined || perfOut === undefined) return null;
  const compileText = flags.get("compile-ms");
  return {
    program,
    schedule,
    db: flags.get("db") ?? ":memory:",
    perfOut,
    compileMs: compileText === undefined ? "N/A" : Number(compileText),
  };
}

/**
 * FINAL STATE — the contract's third check (CONTRACT.md section 2.7).
 *
 * The three functions below are a deliberate mirror of `v6/tsv2/scripts/
 * sweep.ts`'s `finalValueJson` / `finalStateLine` / `readFinalState`, and the
 * mirror is stated rather than hidden: sweep.ts is a self-executing script
 * with no exports, and importing it here would run a 190-fixture sweep as a
 * side effect of a bench run. What matters is that the two copies cannot
 * drift on the ENCODING, which is the part a divergence would hide: both
 * delegate every value to `TickLogEmitter.valueText` and every column read to
 * `rowValueFromSql`, the shared runtime encoders. Only the assembly (sort the
 * rel names, sort the row texts, drop empty rels) is restated, and it is
 * restated byte-for-byte so that the hash computed here is comparable with
 * the `<name>.oracle.final.jsonl` artifacts the sweep already diffs against.
 */
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

/** db_bytes is N/A for :memory:, and CONTRACT 2.4 wants the reason with it. */
function databaseBytes(dbPath: string): { value: number | "N/A"; reason?: string } {
  if (dbPath === ":memory:" || dbPath.startsWith(":")) {
    return { value: "N/A", reason: "db_bytes: run used an in-memory database, so there is no file to size" };
  }
  try {
    return { value: statSync(dbPath).size };
  } catch {
    return { value: "N/A", reason: `db_bytes: no file at ${dbPath} after the run` };
  }
}

function main(): void {
  const args = parseArgs(process.argv.slice(2));
  if (args === null) {
    process.stderr.write("usage: tsv2_run.ts --program <compiled.ts> --schedule <s.json> --db <path> --perf-out <p.json> [--compile-ms <n>]\n");
    process.exitCode = 2;
    return;
  }

  const schedule = JSON.parse(readFileSync(args.schedule, "utf8")) as readonly IArrivalBatch[];

  import(resolve(args.program))
    .then((loaded: { program: EmittedProgram }) => {
      const program = loaded.program;
      const seam = ScratchStore.open(args.db);

      // Counted from here so DDL and boot seeding are OUTSIDE the reported
      // statement count, matching PERF-REPORT's "setup is untimed" rule.
      const started = performance.now();
      ScratchStore.boot(seam, program.ddl)
        .pipe(
          concatMap(() => BootRunner.run(seam, program.boot)),
          concatMap(() => {
            stmt_counter.reset();
            return TickFold.run(program, seam, schedule).pipe(toArray());
          }),
          // wall_ms is read INSIDE this map, before the final-state SELECTs
          // run: the third check is bench bookkeeping, not engine work, and
          // charging its reads to the engine would inflate every scale row.
          map((lines) => ({ lines, wallMs: performance.now() - started, statements: stmt_counter.get() })),
          concatMap((run) => readFinalState(seam, program).pipe(map((finalLine) => ({ ...run, finalLine })))),
        )
        .subscribe({
          next: ({ lines, wallMs, statements, finalLine }) => {
            for (const line of lines) process.stdout.write(`${line}\n`);
            writeFileSync(`${args.perfOut}.final.jsonl`, `${finalLine}\n`);
            const db = databaseBytes(args.db);
            const notes: Record<string, string> = {};
            if (db.reason !== undefined) notes["db_bytes"] = db.reason;
            writeFileSync(
              args.perfOut,
              `${JSON.stringify({
                engine: "tsv2",
                wall_ms: Number(wallMs.toFixed(3)),
                compile_ms: args.compileMs,
                ticks: lines.length,
                statements,
                db_bytes: db.value,
                notes,
              })}\n`,
            );
          },
          error: (error: unknown) => {
            // Stack, not just message: this adapter's failures are read out of
            // a log by a harness, and a bare message cost a whole debugging
            // round on the intermittent BigInt failure (see the HERMETICITY
            // note in bench.sh's header).
            process.stderr.write(`${error instanceof Error ? (error.stack ?? error.message) : String(error)}\n`);
            process.exitCode = 1;
          },
        });
    })
    .catch((error: unknown) => {
      process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
      process.exitCode = 1;
    });
}

main();
