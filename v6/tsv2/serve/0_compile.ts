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

function sourceDigest(source: string): string {
  return bytesToHex(sha256(new TextEncoder().encode(source))).slice(0, 32);
}

/** swipl as an observable: stderr is collected so a refusal reaches the caller
 *  as its own text (`unsupported_construct(...)`, a parse finding) rather than
 *  as a bare nonzero exit. */
function runSwipl(args: readonly string[]): Observable<string> {
  return new Observable<string>((subscriber) => {
    const child = spawn("swipl", [...args], { shell: false });
    let stdout = "";
    let stderr = "";
    child.stdout?.on("data", (chunk: Buffer) => {
      stdout += chunk.toString();
    });
    child.stderr?.on("data", (chunk: Buffer) => {
      stderr += chunk.toString();
    });
    child.on("error", (failure) => subscriber.error(failure));
    child.on("close", (code) => {
      if (code !== 0) {
        subscriber.error(new Error(`dl6 compile failed (swipl exit ${code}): ${stderr.trim() || stdout.trim()}`));
        return;
      }
      subscriber.next(stdout);
      subscriber.complete();
    });
    return () => child.kill();
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
    return runSwipl([
      "-q",
      "-l",
      COMPILE_PL,
      "-g",
      `compile_dl6('${sourcePath}', '${modulePath}')`,
      "-g",
      "halt",
    ]).pipe(
      concatMap(() => from(import(modulePath) as Promise<{ readonly program: IServedProgram }>)),
      map((loaded) => loaded.program),
    );
  },
};
