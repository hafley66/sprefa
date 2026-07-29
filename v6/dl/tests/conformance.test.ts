/**
 * The language test. Input is fixtures/conformance.dl. Output is what SQLite holds,
 * plus wall time and peak heap. Nothing between those two points is asserted.
 */
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { test } from "node:test";

import { bridge } from "../src/0_ast_bridge.ts";
import { DlRuntime } from "../src/3_runtime.ts";
import { builtinRelsForTests, readFixture } from "./0_helpers.ts";
import { cleanupDbFile, freshDbPath } from "./1_helpers_db.ts";

const GOLDEN_PATH = path.join(import.meta.dirname, "..", "fixtures", "golden", "conformance.json");

function sortedRows(rows: readonly Record<string, unknown>[]): unknown[] {
  return rows.map((row) => JSON.stringify(row)).sort();
}

test("conformance.dl: every language case, asserted on the resulting sqlite", async () => {
  const bridgeResult = bridge(readFixture("conformance.dl6"), builtinRelsForTests());
  assert.equal(bridgeResult.kind, "ok", JSON.stringify(bridgeResult, null, 2));

  const databasePath = freshDbPath();
  const startedAt = process.hrtime.bigint();
  const runtime = await DlRuntime.boot({ dbPath: databasePath, bridge: bridgeResult });

  try {
    const declaredRelNames = bridgeResult.program.rels
      .map((declaration) => declaration.name)
      .filter((relationName) => !relationName.startsWith("__"))
      .sort();

    const output: Record<string, unknown[]> = {};
    for (const relationName of declaredRelNames) {
      output[relationName] = sortedRows(await runtime.rows(relationName));
    }

    const elapsedMilliseconds = Number(process.hrtime.bigint() - startedAt) / 1e6;
    const heapMegabytes = process.memoryUsage().heapUsed / 1024 / 1024;
    const databaseKilobytes = fs.statSync(databasePath).size / 1024;
    console.log(
      `[conformance] wall=${elapsedMilliseconds.toFixed(1)}ms heap=${heapMegabytes.toFixed(1)}MB db=${databaseKilobytes.toFixed(0)}KB rels=${declaredRelNames.length}`,
    );

    if (process.env.REGEN_GOLDEN === "1") {
      fs.mkdirSync(path.dirname(GOLDEN_PATH), { recursive: true });
      fs.writeFileSync(GOLDEN_PATH, `${JSON.stringify(output, null, 2)}\n`);
    }
    assert.deepEqual(output, JSON.parse(fs.readFileSync(GOLDEN_PATH, "utf8")));
  } finally {
    await runtime.dispose();
    cleanupDbFile(databasePath);
  }
});
