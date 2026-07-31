/**
 * 0_compile.ts — `.dl6` source text -> an imported, runnable program module.
 *
 * The compiler is v6/prolog/compile's own `compile_dl6/2` (the text door), run
 * as one swipl process per load. Nothing about lowering or emission is
 * reimplemented here: this file writes the source to a file, runs the door,
 * and imports what it wrote. That is the whole of the TS side's compiler
 * knowledge (v6 REORIENTATION: prolog owns the compiler front, TypeScript
 * keeps the static runtime and the generated modules).
 *
 * WHERE THE MODULE LANDS, and why it is not a temp directory: an emitted
 * module's imports are relative (`../runtime/1_incremental.ts`), so the file
 * has to sit as a direct child of this package or those specifiers resolve
 * against the wrong tree. `gen_served/` is that child, gitignored, and named
 * apart from `gen/` (hand-carved) and `gen_emitted/` (the sweep's fixture
 * output, which sweep.sh rewrites).
 *
 * The file name is the source's own sha256. Two consequences, both wanted:
 * loading the same program twice reuses node's module cache instead of
 * minting a second copy, and two different programs can never collide on one
 * specifier. Emitted modules hold no module-level mutable state (checked: no
 * top-level `let` in any emitted file), so a cached module is as good as a
 * fresh one.
 *
 * ── THE COMPILE BUDGET (timeout-gun lane, 2026-07-31) ────────────────────────
 *
 * Standing law: every compute invocation runs under a budget with a NAMED
 * timeout failure. This spawn is the one place a caller's own text decides how
 * long a subprocess runs, so it is the sharpest instance of the law: a program
 * whose compile does not terminate would otherwise hold POST /program open
 * forever with a live swipl behind it, and the incident that produced this
 * rail was exactly that shape -- `3_clock_check.pl`'s simple-path enumeration
 * grinding 9m40s into 8GB and dying as a stack overflow inside the served
 * compiler, with no budget to name it.
 *
 * The timeout answers the POST with an ordinary `compile_timeout` error (400,
 * the same door a parse finding takes) and the SERVER SURVIVES: the running
 * program is untouched, because 4_http.ts swaps only on accepted loads.
 *
 * BUDGET, measured not guessed. The slowest program the repository actually
 * compiles is v6/dl/fixtures/dataflow-atlas.dl6 at ~256s (4m16s), all of it
 * the filed `clock_check_path_blowup`; every other program in v6/dl/fixtures
 * compiles in seconds. Default 600s = ~2.3x the worst honest wall. It is
 * deliberately not the 60s this budget wants to be: a budget that trips on
 * today's honest wall is a mis-set budget. When clock_check_path_blowup lands,
 * this default drops to 60s and the atlas rail's own budget with it.
 * Override with TSV2_COMPILE_BUDGET_MS.
 *
 * The child is spawned `detached`, so it leads its own process group and the
 * timeout kills the GROUP (`process.kill(-pid)`), never just swipl. The bare
 * `child.kill()` this file used to unsubscribe with would leave any
 * grandchild the compiler spawned running -- the same orphaning class
 * v6/tools/run-capped.sh exists to delete on the shell side.
 */

import { spawn } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { sha256 } from "@noble/hashes/sha2.js";
import { bytesToHex } from "@noble/hashes/utils.js";
import { Observable, concatMap, from, map } from "rxjs";

import type { IProgramCompiler, IServedProgram } from "../runtime/types.ts";

const COMPILE_PL = fileURLToPath(new URL("../../prolog/compile/compile.pl", import.meta.url));
const GEN_SERVED_DIR = fileURLToPath(new URL("../gen_served", import.meta.url));

const DEFAULT_COMPILE_BUDGET_MS = 600_000;

function compileBudgetMs(): number {
  const raw = Number(process.env.TSV2_COMPILE_BUDGET_MS ?? "");
  return Number.isFinite(raw) && raw > 0 ? raw : DEFAULT_COMPILE_BUDGET_MS;
}

function sourceDigest(source: string): string {
  return bytesToHex(sha256(new TextEncoder().encode(source))).slice(0, 32);
}

/** Kill the child's whole process group, then the child itself if it never led
 *  one. `detached` makes the group exist; a spawn that failed outright has no
 *  pid and there is nothing to kill. */
function killGroup(pid: number | undefined): void {
  if (pid === undefined) return;
  try {
    process.kill(-pid, "SIGKILL");
  } catch {
    try {
      process.kill(pid, "SIGKILL");
    } catch {
      // Already gone. Nothing to report: this is teardown, not a result.
    }
  }
}

/** swipl as an observable under a budget: stderr is collected so a refusal
 *  reaches the caller as its own text (`unsupported_construct(...)`, a parse
 *  finding) rather than as a bare nonzero exit, and a compile that outruns
 *  `budgetMs` errors as a NAMED `compile_timeout` with its process group
 *  killed rather than holding the request open behind a live swipl. */
function runSwipl(args: readonly string[], budgetMs: number): Observable<string> {
  return new Observable<string>((subscriber) => {
    const child = spawn("swipl", [...args], { shell: false, detached: true });
    let stdout = "";
    let stderr = "";
    let timedOut = false;
    const alarm = setTimeout(() => {
      timedOut = true;
      killGroup(child.pid);
    }, budgetMs);
    child.stdout?.on("data", (chunk: Buffer) => {
      stdout += chunk.toString();
    });
    child.stderr?.on("data", (chunk: Buffer) => {
      stderr += chunk.toString();
    });
    child.on("error", (failure) => {
      clearTimeout(alarm);
      subscriber.error(failure);
    });
    child.on("close", (code) => {
      clearTimeout(alarm);
      if (timedOut) {
        subscriber.error(
          new Error(
            `compile_timeout: the dl6 compiler exceeded its ${budgetMs}ms budget and its process group was killed ` +
              `(raise TSV2_COMPILE_BUDGET_MS if this program is honestly that slow)`,
          ),
        );
        return;
      }
      if (code !== 0) {
        subscriber.error(new Error(`dl6 compile failed (swipl exit ${code}): ${stderr.trim() || stdout.trim()}`));
        return;
      }
      subscriber.next(stdout);
      subscriber.complete();
    });
    return () => {
      clearTimeout(alarm);
      killGroup(child.pid);
    };
  });
}

export const ProgramCompiler: IProgramCompiler = {
  compile(source: string): Observable<IServedProgram> {
    const name = sourceDigest(source);
    const sourcePath = `${GEN_SERVED_DIR}/${name}.dl6`;
    const modulePath = `${GEN_SERVED_DIR}/${name}.ts`;
    mkdirSync(GEN_SERVED_DIR, { recursive: true });
    writeFileSync(sourcePath, source, "utf8");
    // The `-g` goal is a prolog term, so the two paths are quoted as atoms.
    // Both are this process's own absolute paths under gen_served/, never
    // caller text, so there is no quote to escape.
    return runSwipl(
      ["-q", "-l", COMPILE_PL, "-g", `compile_dl6('${sourcePath}', '${modulePath}')`, "-g", "halt"],
      compileBudgetMs(),
    ).pipe(
      concatMap(() => from(import(modulePath) as Promise<{ readonly program: IServedProgram }>)),
      map((loaded) => loaded.program),
    );
  },
};
