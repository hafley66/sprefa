/**
 * bopRun.test.ts — `bop run`, a full subprocess smoke test: boots its own
 * ephemeral in-process server, loads a program, streams tick-log JSONL to
 * stdout, and shuts down cleanly.
 *
 * Two programs, two stopping rules:
 *   - door-handwritten.dl6 has no bind/host and no arrivals to seed with, so
 *     it produces ZERO ticks; `run` must still exit 0 once it goes idle
 *     (`BOP_RUN_IDLE_MS` set low so the test does not wait the 2s default).
 *   - served-host-clock.dl6 declares a real `bind interval(1, bucket)`, which
 *     fires on the real wall clock (this is `run`, not a test-scheduler
 *     receipt), so `--ticks 1` is the deterministic stop and the test bounds
 *     its own wait accordingly.
 */

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const BOP = fileURLToPath(new URL("../cli/bop.ts", import.meta.url));
const EMPTY_DL6 = fileURLToPath(new URL("../../dl/fixtures/door-handwritten.dl6", import.meta.url));
const CLOCK_DL6 = fileURLToPath(new URL("../../dl/fixtures/served-host-clock.dl6", import.meta.url));

test("run: a program with no binds/hosts quiesces at zero ticks and exits 0", () => {
  const result = spawnSync("node", ["--experimental-transform-types", BOP, "run", EMPTY_DL6], {
    encoding: "utf8",
    env: { ...process.env, BOP_RUN_IDLE_MS: "300" },
    timeout: 10_000,
  });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout.trim(), "", `expected zero tick lines, got: ${result.stdout}`);
});

test("run --ticks 1: a live interval bind produces exactly one tick line, then a clean exit", () => {
  const result = spawnSync("node", ["--experimental-transform-types", BOP, "run", CLOCK_DL6, "--ticks", "1"], {
    encoding: "utf8",
    env: { ...process.env, BOP_RUN_IDLE_MS: "5000" },
    timeout: 10_000,
  });
  assert.equal(result.status, 0, result.stderr);
  const lines = result.stdout.trim().split("\n").filter((line) => line.length > 0);
  assert.equal(lines.length, 1, `expected exactly one tick line, got: ${result.stdout}`);
  const parsed = JSON.parse(lines[0] ?? "{}") as { readonly tick: number; readonly deltas: unknown };
  assert.equal(parsed.tick, 1);
  assert.ok(parsed.deltas !== undefined, "tick line carries a deltas object");
});
