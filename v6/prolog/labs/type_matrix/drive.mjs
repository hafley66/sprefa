// drive.mjs -- run every generated cell through BOTH doors and bank the raw
// outcomes. Grading lives in classify.mjs; this file only executes and records.
//
// Per cell, four child processes:
//
//   1  swipl compile_all.pl one(<id>)   the text door (compile:compile_dl6/2)
//   2  swipl oracle_all.pl  one(<id>)   the reference engine
//   3  node golden-run.ts <module> <schedule> --final          incremental
//   4  the same with SPREFA_TSV2_EMITTER_MODE=naive            snapshot referee
//
// Step 3/4 reuse v6/tsv2/scripts/golden-run.ts UNCHANGED rather than growing a
// second copy of the final-state encoder (the repo already has two, in
// golden-run.ts and sweep.ts). The emitted cell module lives in this lab's
// out/, and golden-run.ts's module argument is path-joined, so `../../prolog/
// labs/type_matrix/out/<id>` reaches it; the module's own `../runtime/...`
// imports and its bare `rxjs` import resolve through the two symlinks this lab
// owns (runtime -> tsv2/runtime, node_modules -> tsv2/node_modules).
//
// Run: node v6/prolog/labs/type_matrix/drive.mjs [--only <substring>]

import { spawn } from "node:child_process";
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const OUT = join(HERE, "out");
const TSV2 = resolve(HERE, "..", "..", "..", "tsv2");
const MODULE_PREFIX = "../../prolog/labs/type_matrix/out";
const CONCURRENCY = 8;

function run(command, args, options) {
  return new Promise((settle) => {
    const child = spawn(command, args, { ...options, stdio: ["ignore", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => { stdout += chunk; });
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.on("close", (code) => settle({ code, stdout, stderr }));
  });
}

/** The two prolog doors answer with one JSON line; a missing line means the
 *  process died before printing (dl6_oracle.pl's halt/1 path). */
function prologVerdict(result) {
  const line = result.stdout.trim().split("\n").filter(Boolean).pop();
  if (line !== undefined && line.startsWith("{")) {
    try {
      return JSON.parse(line);
    } catch {
      /* fall through to the halt reading */
    }
  }
  return {
    status: "halt",
    detail: (result.stderr.trim().split("\n").pop() ?? `exit ${result.code}`).slice(0, 300),
  };
}

async function runCell(cell) {
  const compiled = prologVerdict(
    await run("swipl", ["-q", "-l", "compile_all.pl", "-g", `one(${cell.id})`, "-g", "halt"], { cwd: HERE }),
  );
  const oracle = prologVerdict(
    await run("swipl", ["-q", "-l", "oracle_all.pl", "-g", `one(${cell.id})`, "-g", "halt"], { cwd: HERE }),
  );

  const oracleLogPath = join(OUT, `${cell.id}.oracle.jsonl`);
  const goldenLogPath = join(OUT, `${cell.id}.golden.jsonl`);
  const oracleLog = oracle.status === "ok" && existsSync(oracleLogPath)
    ? readFileSync(oracleLogPath, "utf8")
    : null;
  // The second .dl6 oracle door (golden_oracle.pl). oracle_all.pl runs both
  // per cell because neither carries the whole value axis; see its header.
  const goldenLog = oracle.goldenStatus === "ok" && existsSync(goldenLogPath)
    ? readFileSync(goldenLogPath, "utf8")
    : null;

  const emitter = {};
  if (compiled.status === "ok") {
    for (const mode of ["incremental", "naive"]) {
      const result = await run(
        "node",
        [
          "--experimental-transform-types",
          "scripts/golden-run.ts",
          `${MODULE_PREFIX}/${cell.id}`,
          join(OUT, `${cell.id}.schedule.json`),
          "--final",
        ],
        { cwd: TSV2, env: { ...process.env, SPREFA_TSV2_EMITTER_MODE: mode } },
      );
      // The FIRST non-warning stderr line is the thrown error's own message;
      // the rest is a stack through node_modules. Taking the tail instead
      // (first draft) recorded 19 identical rxjs frames and hid a RangeError.
      const firstError = result.stderr
        .split("\n")
        .map((line) => line.trim())
        .find((line) => line.length > 0 && !line.startsWith("(node:") && !line.startsWith("(Use ") && !line.startsWith("at "));
      emitter[mode] = result.code === 0
        ? { status: "ok", log: result.stdout }
        : { status: "run_error", detail: (firstError ?? `exit ${result.code}`).slice(0, 300) };
    }
  }

  return { id: cell.id, compiled, oracle, oracleLog, goldenLog, emitter };
}

async function main() {
  const only = process.argv.includes("--only")
    ? process.argv[process.argv.indexOf("--only") + 1]
    : null;
  const cells = JSON.parse(readFileSync(join(OUT, "cells.json"), "utf8"))
    .filter((cell) => only === null || cell.id.includes(only));

  const results = new Array(cells.length);
  let next = 0;
  let done = 0;
  const workers = Array.from({ length: Math.min(CONCURRENCY, cells.length) }, async () => {
    for (;;) {
      const index = next;
      next += 1;
      if (index >= cells.length) return;
      results[index] = await runCell(cells[index]);
      done += 1;
      if (done % 40 === 0) process.stderr.write(`drive: ${done}/${cells.length}\n`);
    }
  });
  await Promise.all(workers);

  writeFileSync(join(OUT, "run-results.json"), `${JSON.stringify(results)}\n`);
  process.stdout.write(`drive: ${results.length} cells executed\n`);
}

await main();
