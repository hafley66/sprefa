/**
 * spineSchema.test.ts — the emitted ts row interfaces match the curated files.
 * spine_schema facts (3a) are the single source; this test is the grep-style
 * cross-check that stands in for a hand-synced second copy. It asserts, for
 * each marker zone, that the file region between the begin/end markers equals
 * the emitter's output byte-for-byte.
 *
 * Mirrors bopCommandInventory.test.ts:52-59 (spawnSync swipl, asserted-equal).
 */

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const EMITTER_PL = fileURLToPath(new URL("../../prolog/compile/3_emit_spine_schema.pl", import.meta.url));
const SPINE_TS = fileURLToPath(new URL("../../sprefa-store/js/src/engine/spine.ts", import.meta.url));
const TYPES_TS = fileURLToPath(new URL("../../sprefa-store/js/src/engine/types.ts", import.meta.url));

function emittedBody(zone: string): string {
  const result = spawnSync(
    "swipl",
    ["-q", "-l", EMITTER_PL, "-g", `emit_spine_schema:rows_ts_text(${zone},Text),format('~s',[Text])`, "-g", "halt"],
    { encoding: "utf8" },
  );
  assert.equal(result.status, 0, result.stderr);
  return result.stdout;
}

function assertZone(file: string, zone: string, begin: string, end: string): void {
  const source = readFileSync(file, "utf8");
  const body = emittedBody(zone);
  const markerRegion = `${begin}\n${body}${end}`;
  assert.ok(
    source.includes(markerRegion),
    `generated ${zone} zone drifted from canonical facts; run 3_emit_spine_schema`,
  );
}

test("spine.ts marker zone is current with canonical Prolog facts", () => {
  assertZone(SPINE_TS, "spine", "// BEGIN GENERATED spine row interfaces", "// END GENERATED spine row interfaces");
});

test("types.ts marker zone is current with canonical Prolog facts", () => {
  assertZone(TYPES_TS, "types", "// BEGIN GENERATED node/edge row interfaces", "// END GENERATED node/edge row interfaces");
});
