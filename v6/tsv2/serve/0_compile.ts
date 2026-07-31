/** Compiles `.dl6` through the Prolog text door and imports the emitted module.
 * Output is content-addressed in gitignored `gen_served/` so relative runtime
 * imports resolve and repeated source loads share the module cache. The
 * detached child runs under `TSV2_COMPILE_BUDGET_MS`; timeout kills its group.
 */

import { spawn } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { sha256 } from "@noble/hashes/sha2.js";
import { bytesToHex } from "@noble/hashes/utils.js";
import { Observable, concatMap, from, map } from "rxjs";

import type { IProgramCompiler, IServedProgram } from "../runtime/types.ts";

const COMPILE_PL = fileURLToPath(new URL("../../prolog/compile.pl", import.meta.url));
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
