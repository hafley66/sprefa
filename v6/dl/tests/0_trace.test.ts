/**
 * tests/0_trace.test.ts — Phase 0 perf tracing spine gate
 * (plans/2026-07-27-v5-port-perf-header.md). Two shapes:
 *
 *   1. off by default: no DL_PERF_LOG means every channel carries zero subscribers
 *      and no file is written, even across a real commit.
 *   2. on: a real DlRuntime commit against a program with a derived rel (so the SQL
 *      fixpoint seam actually runs) emits one parseable JSONL line per tick, with
 *      the exact contract fields (0_types.ts's PerfTickLine) and plausible values.
 *
 *   A third test exercises the effect/bind/ingest fold + flush path directly
 *   (bypassing 1_hosts.ts/4_ingest.ts/1_binds.ts, which have their own suites), so the
 *   exact field shapes of `effects`/`binds`/`ingest` are pinned without needing a real
 *   subprocess effect or bind commit (F4, 2026-07-28 cleanup audit: `binds` and its
 *   channel's hasSubscribers state were the one seam this file left unasserted).
 */
import assert from "node:assert/strict";
import crypto from "node:crypto";
import diagnostics_channel from "node:diagnostics_channel";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";

import { PERF_CHANNEL_NAMES, PerfTrace } from "../src/0_trace.ts";
import { bootParentFixture, cleanupDbFile, edbBatch } from "./1_helpers_db.ts";

const PARENT_ROWS = [
  { child_name: "alice", parent_name: "bob" },
  { child_name: "bob", parent_name: "carol" },
  { child_name: "carol", parent_name: "dana" },
];

function freshLogPath(): string {
  return path.join(os.tmpdir(), `dl-perf-${crypto.randomBytes(8).toString("hex")}.jsonl`);
}

function cleanupLogFile(logPath: string): void {
  try {
    fs.unlinkSync(logPath);
  } catch {
    // already gone; nothing to clean up.
  }
}

/** Flushes are scheduled on setImmediate (0_trace.ts's FLUSH TIMING note); this waits
 *  for one macrotask turn so any pending flush has run before the test reads the file. */
function waitOneMacrotask(): Promise<void> {
  return new Promise((resolve) => setImmediate(resolve));
}

function readJsonlLines(logPath: string): unknown[] {
  const text = fs.readFileSync(logPath, "utf8");
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0)
    .map((line) => JSON.parse(line));
}

test.afterEach(() => {
  PerfTrace.uninstall();
  delete process.env.DL_PERF_LOG;
});

test("off by default: channels carry no subscriber, no file written, even across a real commit", async () => {
  assert.equal(process.env.DL_PERF_LOG, undefined);
  PerfTrace.installFromEnv();

  assert.equal(diagnostics_channel.channel(PERF_CHANNEL_NAMES.sql).hasSubscribers, false);
  assert.equal(diagnostics_channel.channel(PERF_CHANNEL_NAMES.effect).hasSubscribers, false);
  assert.equal(diagnostics_channel.channel(PERF_CHANNEL_NAMES.bind).hasSubscribers, false);
  assert.equal(diagnostics_channel.channel(PERF_CHANNEL_NAMES.ingest).hasSubscribers, false);
  assert.equal(PerfTrace.enabled, false);

  // No DL_PERF_LOG was ever pointed here; a real commit (exercising sqlTraceFor,
  // finishSqlTrace, tickSettled) must not conjure a destination out of nothing.
  const logPath = freshLogPath();
  const { rt, dbPath } = await bootParentFixture();
  try {
    await rt.commit(edbBatch({ parent: PARENT_ROWS }));
    await waitOneMacrotask();
    assert.equal(fs.existsSync(logPath), false);
    assert.equal(diagnostics_channel.channel(PERF_CHANNEL_NAMES.sql).hasSubscribers, false);
    assert.equal(diagnostics_channel.channel(PERF_CHANNEL_NAMES.bind).hasSubscribers, false);
  } finally {
    await rt.dispose();
    cleanupDbFile(dbPath);
  }
});

test("on: a real commit against a derived-rel program emits one parseable JSONL line with the contract fields", async () => {
  const logPath = freshLogPath();
  process.env.DL_PERF_LOG = logPath;
  PerfTrace.installFromEnv();
  try {
    assert.equal(diagnostics_channel.channel(PERF_CHANNEL_NAMES.sql).hasSubscribers, true);
    assert.equal(diagnostics_channel.channel(PERF_CHANNEL_NAMES.bind).hasSubscribers, true);
    assert.equal(PerfTrace.enabled, true);

    const { rt, dbPath } = await bootParentFixture();
    try {
      // buildParentGrandparentProgram's "grandparent" rel is derived, so this commit
      // exercises the SQL fixpoint seam (evalProgramSql), not just the EDB write.
      const report = await rt.commit(edbBatch({ parent: PARENT_ROWS }));
      assert.equal(report.tick, 1);

      await waitOneMacrotask();

      const lines = readJsonlLines(logPath) as Record<string, unknown>[];
      const line = lines.find((entry) => entry.tick === report.tick);
      assert.ok(line, `no perf line found for tick ${report.tick} among ${lines.length} line(s)`);

      assert.equal(typeof line.wall_ms, "number");
      assert.equal(typeof line.stmt_count, "number");
      assert.equal(typeof line.stmt_ms_total, "number");
      assert.equal(typeof line.stmt_ms_max, "number");
      assert.ok(Array.isArray(line.effects));
      assert.equal(line.ingest, null);
      assert.equal(typeof line.rss_kb, "number");

      assert.ok((line.stmt_count as number) > 0, "expected at least one SQL statement (the fixpoint ran)");
      assert.ok(
        (line.wall_ms as number) >= (line.stmt_ms_max as number),
        `wall_ms (${line.wall_ms}) should be >= stmt_ms_max (${line.stmt_ms_max})`,
      );
      assert.ok((line.rss_kb as number) > 0);
      assert.deepEqual(line.effects, []);
    } finally {
      await rt.dispose();
      cleanupDbFile(dbPath);
    }
  } finally {
    cleanupLogFile(logPath);
  }
});

test("EDB plane: execute$ publishes seam:\"edb\" sql events with a numeric duration", async () => {
  // The EDB write phase (applyEdbTxn's insertRows/deleteRows/insertDeltaRows, all
  // funneled through 3_runtime.ts's execute$) is invisible to the tick-keyed
  // sqlTraceFor seam, which only sees the fixpoint phase. Subscribe directly to the
  // shared sprefa:sql channel and commit an EDB write: the captured events must carry
  // seam "edb" and a numeric ms, proving the ingest path now reaches the trace spine.
  const sqlChannel = diagnostics_channel.channel(PERF_CHANNEL_NAMES.sql);
  const { rt, dbPath } = await bootParentFixture();
  const captured: Record<string, unknown>[] = [];
  const listener = (message: unknown): void => {
    captured.push(message as Record<string, unknown>);
  };
  sqlChannel.subscribe(listener);
  try {
    await rt.commit(edbBatch({ parent: PARENT_ROWS }));
  } finally {
    sqlChannel.unsubscribe(listener);
    await rt.dispose();
    cleanupDbFile(dbPath);
  }

  const edbLines = captured.filter((event) => event.seam === "edb");
  assert.ok(
    edbLines.length > 0,
    `expected at least one EDB-plane sql event tagged seam:"edb", saw ${captured.length} total captured`,
  );
  for (const line of edbLines) {
    assert.equal(typeof line.sql, "string");
    assert.equal(typeof line.ms, "number", "EDB sql event must carry a numeric duration");
  }
});

test("on: effectDone/bindDone/ingestDone fold into the same tick's line with the exact field shapes", async () => {
  // F4 (2026-07-28 cleanup audit): this case pinned effects/ingest exactly but left
  // `binds` (PerfBindEntry, the input-side twin of `effects` -- 1_binds.ts's BindRunner
  // via bindDone) entirely unasserted, and the "off by default" test above checked
  // hasSubscribers on 3 of the 4 seam channels, omitting `bind`. Both gaps closed here:
  // a `bindDone` call alongside effectDone/ingestDone, and a `deepEqual` on `line.binds`.
  const logPath = freshLogPath();
  process.env.DL_PERF_LOG = logPath;
  PerfTrace.installFromEnv();
  try {
    const tick = 42;
    PerfTrace.effectDone(tick, "sg", 12345, 3.5, "done");
    PerfTrace.bindDone(tick, "clock_bucket", 1, 0.8);
    PerfTrace.ingestDone(tick, {
      path: "src/foo.ts",
      read_ms: 0.5,
      extract_wall_ms: 1.25,
      fact_ms: 0.75,
      diff_ms: 0.4,
      commit_ms: 0.3,
      fact_lines: 7,
      diff_size: 2,
    });
    PerfTrace.tickSettled(tick);

    await waitOneMacrotask();

    const lines = readJsonlLines(logPath) as Record<string, unknown>[];
    const line = lines.find((entry) => entry.tick === tick);
    assert.ok(line, `no perf line found for manufactured tick ${tick}`);

    assert.equal(line.stmt_count, 0); // no sqlTraceFor calls this test
    assert.deepEqual(line.effects, [{ host: "sg", effect_id: 12345, ms: 3.5, status: "done" }]);
    assert.deepEqual(line.binds, [{ rel: "clock_bucket", rows: 1, ms: 0.8 }]);
    assert.deepEqual(line.ingest, {
      path: "src/foo.ts",
      read_ms: 0.5,
      extract_wall_ms: 1.25,
      fact_ms: 0.75,
      diff_ms: 0.4,
      commit_ms: 0.3,
      fact_lines: 7,
      diff_size: 2,
    });
  } finally {
    cleanupLogFile(logPath);
  }
});
