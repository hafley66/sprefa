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

import { concatMap, toArray } from "rxjs";

import { stmt_counter } from "sprefa-store-engine/src/engine/counter.ts";

import { BootRunner } from "../runtime/2_boot.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import { TickFold } from "../runtime/tickLoop.ts";
import type { IArrivalBatch, IBootStatement, IGenProgram } from "../runtime/types.ts";

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
        )
        .subscribe({
          next: (lines) => {
            const wallMs = performance.now() - started;
            const statements = stmt_counter.get();
            for (const line of lines) process.stdout.write(`${line}\n`);
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
