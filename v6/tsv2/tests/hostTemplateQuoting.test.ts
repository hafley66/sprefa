/**
 * hostTemplateQuoting.test.ts — a `{col}` splice cannot execute anything.
 *
 * THE DEFECT (review finding 3, plans/2026-07-30-ts-lowering-review.md).
 * `fillTemplate` spliced the row value into the command text by plain string
 * replacement and `runShellLine` spawned it with `shell: true`. No quoting, no
 * escaping, no opt-in safe mode. The reviewer's probe CREATED A FILE ON DISK
 * from an ordinary arrival. Host inputs are file paths and globs read off disk
 * (`scripts/crawl-bench.sh:128` is `ls-files -- '{glob}'`), so the payload does
 * not have to come from a hostile client to get there.
 *
 * WHY NOT ARGV-FORM SPAWN. The brief prefers it where the template shape
 * allows, and no shipped template allows it: they carry pipes, `while` loops,
 * command substitution, redirections and `$VAR` expansion the child's own shell
 * is meant to perform (see `v6/dl/fixtures/v5-git-diags.dl6:108`). Splitting
 * those into argv would need a shell parser, and would change what the
 * templates mean. So the fix is real quoting.
 *
 * WHY NOT ONE QUOTING FUNCTION. Because the placeholder's own QUOTING CONTEXT
 * decides what escaping is correct, and all three contexts are in live use:
 *   `printf '%s' '{name}'`   inside single quotes  (v5-git-diags, sg-rail)
 *   `printf '%s' "{name}"`   inside double quotes  (golden-flex, served-host-clock)
 *   `sg {file_digest} ...`   bare                  (extraction-live, comment rails)
 * Wrapping the value in single quotes unconditionally is the usual advice and
 * it is wrong here twice over: inside `'...'` it produces `''value''` and
 * inside `"..."` it puts literal quote characters into the output, which breaks
 * byte identity for every existing golden. So `fillTemplate` tracks the shell's
 * own quote state as it walks the template and escapes each value FOR THE
 * CONTEXT IT LANDS IN. A value carrying no metacharacters splices exactly as it
 * did before, in all three contexts, which is what keeps the goldens byte
 * identical.
 *
 * THE PROBE. Five payloads, each an escape attempt against a different
 * construct, all carrying `:>FILE` (a redirection that creates a file and needs
 * NO SPACE, so the assertion below is about quoting and not about how
 * `parseWhitespace` splits an answer). Every payload is fed to all three hosts,
 * so the matrix is 15 invocations: single-quote break, double-quote break,
 * `$(...)`, backticks, and bare metacharacters, against single, double and bare
 * splice sites.
 *
 * RED FIRST, verbatim, at b8485ea3 before the fix (`node --test
 * --experimental-transform-types tests/hostTemplateQuoting.test.ts`):
 *
 *   ✖ a {col} splice cannot execute anything, in any quoting context
 *     AssertionError [ERR_ASSERTION]: host templates executed spliced values:
 *     m1, m2, m3, m4, m5
 *     + actual - expected
 *     + [ 'm1', 'm2', 'm3', 'm4', 'm5' ]
 *     - []
 *
 * ALL FIVE fired. Across three templates every payload finds a splice site
 * where its own escape works: the single-quote break lands unquoted in
 * `look_bare`, the double-quote break lands unquoted there too, and `$(...)`
 * and backticks reach the double-quoted template as well as the bare one.
 *
 * SABOTAGE RECEIPTS, both run after the fix and reverted.
 *
 *   (a) `escapeForShell` returns its value unchanged in every context. Same red
 *   as above, same five markers.
 *
 *   (b) The naive single-quote-everything fix: force the "bare" arm for every
 *   context. It is WORSE than doing nothing in one direction and breaks the
 *   goldens in the other, and both halves were measured:
 *     ✖ a {col} splice cannot execute anything, in any quoting context
 *       AssertionError: host templates executed spliced values: m2, m3, m4, m5
 *     FAIL  served e2e leg          (`just golden-flex`)
 *       AssertionError: Expected values to be strictly deep-equal
 *   Four payloads still fire because `'value'` spliced INSIDE `'...'` produces
 *   `''value''`, which closes the template's own quoting and hands the rest of
 *   the payload to the shell. That is the whole reason the escaping is per
 *   context, and the golden-flex line beside it is the byte-identity half.
 */

import assert from "node:assert/strict";
import { mkdtempSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { postArrivals, postProgram, request, startServed } from "./serveHelpers.ts";

/** One host per quoting context, and one rule per host so every payload is
 *  demanded through all three. */
const PROGRAM = [
  "rel payload(name: text).",
  "",
  "sh look_single(name: text) -> (out: text) = `printf '%s' '{name}'`.",
  'sh look_double(name: text) -> (out: text) = `printf \'%s\' "{name}"`.',
  "sh look_bare(name: text) -> (out: text) = `printf '%s' {name}`.",
  "",
  "rel seen_single(name: text, out: text).",
  "seen_single(Name, Out) <- payload(Name), look_single(Name, Out).",
  "rel seen_double(name: text, out: text).",
  "seen_double(Name, Out) <- payload(Name), look_double(Name, Out).",
  "rel seen_bare(name: text, out: text).",
  "seen_bare(Name, Out) <- payload(Name), look_bare(Name, Out).",
  "",
].join("\n");

/** `:>path` creates a file and contains no space, so a payload that fires is
 *  visible on disk without depending on how the host answer is split. */
function payloads(markerDir: string): readonly string[] {
  const marker = (name: string): string => join(markerDir, name);
  return [
    `x';:>${marker("m1")};echo'`, // break out of single quotes
    `x";:>${marker("m2")};echo"`, // break out of double quotes
    `x$(:>${marker("m3")})y`, // command substitution
    `x\`:>${marker("m4")}\`y`, // backtick substitution
    `x;:>${marker("m5")};echo`, // bare metacharacters
  ];
}

interface RowsReply {
  readonly rows: readonly (readonly unknown[])[];
}

async function rowsOf(port: number, rel: string): Promise<RowsReply["rows"]> {
  const reply = await request(port, `/idb/${rel}`, "GET");
  assert.equal(reply.statusCode, 200, `GET /idb/${rel} -> ${reply.statusCode} ${reply.body}`);
  return (JSON.parse(reply.body) as RowsReply).rows;
}

async function waitUntil(predicate: () => Promise<boolean>, what: string, timeoutMs = 10_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await predicate()) return;
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  throw new Error(`timeout waiting for ${what}`);
}

test("a {col} splice cannot execute anything, in any quoting context", async () => {
  const markerDir = mkdtempSync(join(tmpdir(), "tsv2-host-quote-"));
  const values = payloads(markerDir);
  const served = await startServed();
  try {
    const loaded = await postProgram(served.port, PROGRAM);
    assert.equal(loaded.statusCode, 200, loaded.body);

    await postArrivals(
      served.port,
      values.map((value) => ({ rel: "payload", sign: "add" as const, row: [value] })),
    );

    // Every host answer has to land before the disk can be judged: a marker
    // written by a subprocess that has not run yet proves nothing. An injected
    // payload can also make a host answer WRONG rather than late, so a timeout
    // here is itself a finding and must not hide the marker assertion below it.
    let settleFailure: unknown = null;
    try {
      for (const rel of ["seen_single", "seen_double", "seen_bare"]) {
        await waitUntil(
          async () => (await rowsOf(served.port, rel)).length === values.length,
          `${rel} to hold ${values.length} host answers`,
        );
      }
    } catch (failure) {
      settleFailure = failure;
    }

    assert.deepEqual(
      readdirSync(markerDir).sort(),
      [],
      `host templates executed spliced values: ${readdirSync(markerDir).sort().join(", ")}`,
    );
    if (settleFailure !== null) throw settleFailure;

    // And the values came back through the shell UNCHANGED, which is the other
    // half: escaping that neutralized the payload by mangling it would be a
    // different defect, not a fix.
    for (const rel of ["seen_single", "seen_double", "seen_bare"]) {
      const seen = await rowsOf(served.port, rel);
      assert.deepEqual(
        seen.map((row) => [row[0], row[1]]).sort(),
        values.map((value) => [value, value]).sort(),
        `${rel} must echo every payload verbatim`,
      );
    }
  } finally {
    await served.stop();
    rmSync(markerDir, { recursive: true, force: true });
  }
});
