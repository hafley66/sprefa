/**
 * bopCheck.test.ts — the CLI's `check` verb, exit-code contract only (0 clean
 * / 2 named-unsupported construct findings / 1 broken). `check` boots no server (spec:
 * "NO server needed if compilation is pure"), so this file spawns the CLI
 * itself as a subprocess and reads only its exit code + stderr, the same
 * black-box shape a real caller sees.
 *
 * SABOTAGE RECEIPT (run at authoring time against bop_check.pl, reverted --
 * full transcript in that file's own header): flipping the compile-unsupported construct
 * branch's exit code from 2 to 1 made the findings-fixture assertion below go
 * red (ghcacher.dl6 reported 1, asserted 2) and green again once reverted.
 */

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const BOP = fileURLToPath(new URL("../cli/bop.ts", import.meta.url));
const CLEAN_DL6 = fileURLToPath(new URL("../../dl/fixtures/door-handwritten.dl6", import.meta.url));

function run_check(file: string): { readonly status: number | null; readonly stderr: string } {
  const result = spawnSync("node", ["--experimental-transform-types", BOP, "check", file], { encoding: "utf8" });
  return { status: result.status, stderr: result.stderr };
}

test("check: a program with zero findings that compiles clean exits 0, silently", () => {
  const outcome = run_check(CLEAN_DL6);
  assert.equal(outcome.status, 0, outcome.stderr);
});

test("check: use declarations compile through the same module expansion as compile_dl6", () => {
  const work_dir = mkdtempSync(join(tmpdir(), "bop-check-use-"));
  writeFileSync(join(work_dir, "model.dl6"), "rel Item(name: text).\n", "utf8");
  const program_path = join(work_dir, "main.dl6");
  writeFileSync(
    program_path,
    'use "model.dl6" as model.\nrel Seen(name: text).\nSeen(Name) <- model.Item(Name).\n',
    "utf8",
  );
  const outcome = run_check(program_path);
  assert.equal(outcome.status, 0, outcome.stderr);
});

test("check: a program that hits a named compiler unsupported construct exits 2 and names it on stderr", () => {
  const work_dir = mkdtempSync(join(tmpdir(), "bop-check-unsupported-"));
  const program_path = join(work_dir, "unsupported.dl6");
  writeFileSync(
    program_path,
    "rel Item(value: list(option(bytes))).\n",
    "utf8",
  );
  const outcome = run_check(program_path);
  assert.equal(outcome.status, 2, outcome.stderr);
  assert.match(outcome.stderr, /unsupported_construct/);
});

test("check: a file that does not exist exits 1, broken", () => {
  const outcome = run_check(join(tmpdir(), "bop-check-missing-does-not-exist.dl6"));
  assert.equal(outcome.status, 1, outcome.stderr);
});

/* Cold-author defect D3: `check` and compile_dl6.sh are two doors onto one
 * compile, and only the script's door threaded the location -- the CLI printed
 * "rule-index unavailable" for the very file the script located at line 4. The
 * assertion is the FILE and the LINE together, not the word "at": a unsupported construct
 * that names neither is the defect, and a unsupported construct that names a wrong line is
 * worse than none.
 *
 * FAIL-FIRST RECEIPT (run at authoring time, reverted): with bop_check.pl's
 * catch/3 around compile_program/6 removed, this test reads
 *   unsupported construct: rule-index unavailable: unsupported_construct: ...
 * and goes red on the location match; restoring the catch makes it green. */
test("check: a located unsupported construct names file and line, the same location compile_dl6.sh prints", () => {
  const work_dir = mkdtempSync(join(tmpdir(), "bop-check-located-"));
  const program_path = join(work_dir, "broken.dl6");
  // `beat` is declared `log` and headed by a level rule, which is
  // log_on_level_headed_rel. The rule is on line 4 of this exact text.
  writeFileSync(
    program_path,
    [
      "bind interval(period: int, bucket: int).",
      "",
      "rel beat(bucket: int) log keep(all).",
      "beat(bucket) <- interval(1, bucket).",
      "",
    ].join("\n"),
    "utf8",
  );
  const outcome = run_check(program_path);
  assert.equal(outcome.status, 2, outcome.stderr);
  assert.match(outcome.stderr, /log_on_level_headed_rel/);
  assert.ok(
    outcome.stderr.includes(`${program_path}:4:`),
    `expected the unsupported construct to carry ${program_path}:4, got: ${outcome.stderr}`,
  );
});

test("check: a file that does not parse at all exits 1, broken", () => {
  const work_dir = mkdtempSync(join(tmpdir(), "bop-check-broken-"));
  const broken_path = join(work_dir, "broken.dl6");
  // An unclosed paren: no grammar production in parse_dl.pl accepts this, so
  // parse_dl_file/4 either fails outright or throws dl_parse_error -- either
  // way bop_check.pl's own broken/1 path, never a "finding".
  writeFileSync(broken_path, "rel foo(a: int\n", "utf8");
  const outcome = run_check(broken_path);
  assert.equal(outcome.status, 1, outcome.stderr);
});
