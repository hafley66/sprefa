/**
 * serveCompileBudget.test.ts — THE COMPILE DOOR'S TIMEOUT GUN (timeout-gun
 * lane, 2026-07-31). Standing law: every compute invocation runs under a budget
 * with a NAMED timeout failure. A resource cliff is a named refusal, never a
 * hang, never an OOM death.
 *
 * The incident: `3_clock_check.pl`'s simple-path enumeration ground 9m40s into
 * 8GB and died as a stack overflow INSIDE the served compiler. POST /program
 * had no budget, so the request held open behind a live swipl for as long as
 * the compiler cared to run, and the only signal at the end was a crash.
 *
 * THE PLANTED SLOW COMPILE. Rather than fabricate a pathological program (whose
 * cost would drift with every compiler change and whose "slow" is a moving
 * target), these tests keep the real door program and set the budget below its
 * honest wall through `TSV2_COMPILE_BUDGET_MS`. Same discriminator, hermetic,
 * and it stays true when the compiler gets faster.
 *
 * THREE THINGS ARE ASSERTED, and only together do they mean anything:
 *   1  the answer NAMES the failure (`compile_timeout`), so a caller reading
 *      the 400 body knows a budget fired and not that its program was refused;
 *   2  the swipl PROCESS GROUP is dead afterwards -- an orphaned compiler that
 *      keeps burning a core is precisely what the process-group kill exists to
 *      prevent, and a test that only read the status code could never see it;
 *   3  the server SURVIVES and the next POST loads normally, which is the
 *      difference between a budget and a crash.
 *
 * SABOTAGE RECEIPT (both run 2026-07-31, both reverted, tree clean).
 *
 *   (a) FLIPS IT. The timeout gutted -- `setTimeout(() => {}, 0)` in place of
 *       the alarm, so `timedOut` never sets -- and BOTH tests go red at
 *         AssertionError: actual 200, expected 400
 *       because the compile simply completes and the budget did nothing.
 *
 *   (b) DOES NOT FLIP IT, and the honest reading matters more than a second
 *       green tick. Reducing `killGroup(child.pid)` to `child.kill()` leaves
 *       both tests PASSING: swipl running `compile_dl6/2` spawns no child of
 *       its own, so SIGTERM to the one process is enough and the pgrep leg
 *       sees nothing left either way. The process-group form is insurance
 *       against a compiler that DOES spawn (a future `sh` inside the door, a
 *       parallel driver), and this receipt does not prove it. What the pgrep
 *       leg does prove is that the timeout kills SOMETHING: with (a) applied
 *       and the budget raised past the honest wall the compile would still be
 *       running when the test read, and a status-code-only test could not
 *       tell a killed compiler from an abandoned one. (The process-group leg
 *       IS proven, on the shell side, by the run-capped.sh command-form
 *       receipt in docs/failure-modes.md class 38: a backgrounded grandchild
 *       dies with its parent there.)
 */

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { sha256 } from "@noble/hashes/sha2.js";
import { bytesToHex } from "@noble/hashes/utils.js";

import { postProgram, startServed } from "./serveHelpers.ts";

const DOOR_DL6 = fileURLToPath(new URL("../../dl/fixtures/door-handwritten.dl6", import.meta.url));

/**
 * A copy of the door program that no other test in this package can be
 * compiling. `npm test` runs the suites in PARALLEL, so "swipl processes that
 * were not running when this test started" is not the same set as "swipl
 * processes this test started" -- a neighbour's compile that began a moment
 * later lands in the difference and reads as a leak. The first run of this
 * receipt failed exactly that way, on three pids belonging to other files.
 *
 * The digest 0_compile.ts names the source file by is the source's own sha256,
 * so a unique trailing comment buys a unique file name, and `pgrep -f` on that
 * name matches THIS test's compiler and nothing else. Comments do not reach
 * the compiled program, so the two tests still compile the real door.
 */
function uniqueDoorSource(tag: string): { readonly source: string; readonly digest: string } {
  const source = `${readFileSync(DOOR_DL6, "utf8")}\n# compile-budget receipt ${tag} ${process.pid}\n`;
  return { source, digest: bytesToHex(sha256(new TextEncoder().encode(source))).slice(0, 32) };
}

/** Running swipl processes whose command line names this source digest. Empty
 *  on no match: pgrep exits 1 with nothing to report, which is not an error. */
function compilerPidsFor(digest: string): readonly string[] {
  try {
    return execFileSync("pgrep", ["-f", digest], { encoding: "utf8" })
      .split("\n")
      .map((line) => line.trim())
      .filter((line) => line.length > 0);
  } catch {
    return [];
  }
}

function withCompileBudget<T>(budgetMs: number, body: () => Promise<T>): Promise<T> {
  const previous = process.env.TSV2_COMPILE_BUDGET_MS;
  process.env.TSV2_COMPILE_BUDGET_MS = String(budgetMs);
  return body().finally(() => {
    if (previous === undefined) delete process.env.TSV2_COMPILE_BUDGET_MS;
    else process.env.TSV2_COMPILE_BUDGET_MS = previous;
  });
}

test("compile budget: a compile that outruns its budget is a NAMED compile_timeout, not a hang", async () => {
  const { source } = uniqueDoorSource("named");
  const served = await startServed();
  try {
    const started = Date.now();
    const answered = await withCompileBudget(50, () => postProgram(served.port, source));
    const elapsed = Date.now() - started;

    assert.equal(answered.statusCode, 400, answered.body);
    assert.match(answered.body, /compile_timeout/);
    assert.match(answered.body, /TSV2_COMPILE_BUDGET_MS/);
    // budget + epsilon: the point of a budget is that the answer arrives on
    // budget time, not on the compiler's time. 10s of slack is spawn latency
    // and node scheduling, not another compile.
    assert.ok(elapsed < 10_000, `answered in ${elapsed}ms, which is not budget + epsilon`);
  } finally {
    await served.stop();
  }
});

test("compile budget: the timed-out compiler's process group is dead, and the server still loads programs", async () => {
  const { source, digest } = uniqueDoorSource("group");
  const served = await startServed();
  try {
    const answered = await withCompileBudget(50, () => postProgram(served.port, source));
    assert.equal(answered.statusCode, 400, answered.body);

    // NOTHING SURVIVED THE KILL. Only the compiler working on THIS source is
    // examined -- see uniqueDoorSource: a neighbouring suite's swipl is none
    // of this test's business, and the first draft of this assertion went red
    // on three of them.
    await new Promise((resolve) => setTimeout(resolve, 500));
    const leaked = compilerPidsFor(digest);
    assert.deepEqual(leaked, [], `the timed-out compile left swipl process(es) running: ${leaked.join(",")}`);

    // THE SERVER SURVIVED. Same source, honest budget, ordinary 200.
    const loaded = await postProgram(served.port, source);
    assert.equal(loaded.statusCode, 200, loaded.body);
  } finally {
    await served.stop();
  }
});
