/**
 * spineSchema.test.ts — the generated spine row-interface zones are current.
 * 3a_spine_schema_facts.pl is the single source; this gate asserts the
 * emitter's zone text equals the file's marker-zone body byte-for-byte, so a
 * hand edit that drifts facts vs ts fails here (pattern:
 * bopCommandInventory.test.ts).
 */

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const EMITTER_PL = fileURLToPath(new URL("../../prolog/compile/3_emit_spine_schema.pl", import.meta.url));

const ZONES = [
  {
    zone: "spine_ts",
    file: fileURLToPath(new URL("../../sprefa-store/js/src/engine/spine.ts", import.meta.url)),
    begin: "// BEGIN GENERATED spine row interfaces (v6/prolog/compile/3_emit_spine_schema.pl)",
    end: "// END GENERATED spine row interfaces",
  },
  {
    zone: "types_ts",
    file: fileURLToPath(new URL("../../sprefa-store/js/src/engine/types.ts", import.meta.url)),
    begin: "// BEGIN GENERATED node/edge row interfaces (v6/prolog/compile/3_emit_spine_schema.pl)",
    end: "// END GENERATED node/edge row interfaces",
  },
] as const;

function emittedZoneText(zone: string): string {
  const result = spawnSync(
    "swipl",
    ["-q", "-l", EMITTER_PL, "-g", `emit_spine_schema:rows_ts_text(${zone}, Text),format('~s',[Text])`, "-g", "halt"],
    { encoding: "utf8" },
  );
  assert.equal(result.status, 0, result.stderr);
  return result.stdout;
}

function fileZoneText(file: string, begin: string, end: string): string {
  const source = readFileSync(file, "utf8");
  const beginAt = source.indexOf(begin);
  assert.notEqual(beginAt, -1, `${file} lacks marker: ${begin}`);
  assert.equal(source[beginAt + begin.length], "\n", "begin marker must end its line");
  const bodyStart = beginAt + begin.length + 1;
  const endAt = source.indexOf(end, bodyStart);
  assert.notEqual(endAt, -1, `${file} lacks marker: ${end}`);
  return source.slice(bodyStart, endAt);
}

for (const { zone, file, begin, end } of ZONES) {
  test(`generated ${zone} row-interface zone is current with canonical Prolog facts`, () => {
    assert.equal(fileZoneText(file, begin, end), emittedZoneText(zone));
  });
}
