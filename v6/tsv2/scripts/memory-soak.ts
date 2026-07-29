/**
 * memory-soak.ts -- NEW DIRECTIVE (user, 2026-07-29 late): the reactivity
 * engine needs an interval-driven soak proving node memory pressure stays
 * CONSTANT under sustained assert/retract churn. `scripts/leak-soak.sh`
 * covers handles/RSS across PROGRAM SWAPS; this covers sustained write
 * churn on ONE running program -- a different axis, the two are additive.
 * `scripts/memory-soak.sh` is the entry point (mirrors `leak-soak.sh`'s and
 * `endurance.sh`'s shape: env-var config, a receipt printed at the end).
 *
 * THE CHURN PROGRAM (below, `churnProgram`) has exactly the three shapes the
 * arc's brief named:
 *   - `session(id: int, tag: text) key(1)`: a KEYED rel driven by constant-
 *     rate REPLACEMENTS. Every arrival deletes the key's old row and inserts
 *     the new one (assert+retract, every tick), yet the steady-state row
 *     count is exactly `keys` -- FLAT BY CONSTRUCTION.
 *   - `activity(id, tag) log keep(count(cap))`: a LOG rel churning past its
 *     retention cap. Every arrival appends, and once past `cap` rows the
 *     retention statement prunes the oldest -- steady-state row count is
 *     `cap`, FLAT BY CONSTRUCTION once warmed.
 *   - `session_activity(...) <+ activity(...), latest(session(...))`: a
 *     DERIVED (edge) rel over both, so every churn tick runs the incremental
 *     dispatch, not just arrival absorption. Same `keep(count(cap))`
 *     retention, same flat steady state.
 * Verified empirically before this file was written (memory-soak arc
 * report carries the numbers): `latest(session(id, session_tag))` samples
 * `session`'s CURRENT value within the SAME tick an `activity` arrival for
 * that key lands, so a warmup batch seeding every key once is enough for
 * every later `activity` arrival to find its match.
 *
 * WHY THIS SCRIPT DOES NOT REUSE `tests/serveHelpers.ts`'s `startServed`:
 * that helper's `next` handler pushes EVERY event (including every tick
 * outcome, full delta rows and all) into an array it never clears, for the
 * fixture's whole lifetime -- correct for the short, few-tick fixtures it
 * was built for (replay grading needs the full history), and MEASURED here
 * to be a real, misleading growth source at soak scale: an early draft of
 * this script reused it and saw heapUsed climb from ~20MB to ~90MB over
 * 2500 ticks with `keep(count(200))` in place, i.e. WHILE THE ENGINE'S OWN
 * storage stayed flat at 5 pages. Switching to the private, non-retaining
 * subscribe below (same shape as `run-fixture.ts`'s one exempt subscribe --
 * `v6/tools/one-subscribe.sh` scans `dl/src` and `tsv2/serve` only, per its
 * own header, never `tsv2/scripts`) made heapUsed flat at ~21.5MB from
 * tick ~500 onward. `postProgram`/`postArrivals`/`request` themselves ARE
 * reused (stateless per call, no retention) -- only `startServed` is not.
 *
 * PACING: a plain `for` loop with `await sleep(arrivalIntervalMs)` between
 * POSTs, not `setInterval`: a bare timer would let requests queue up if a
 * POST ever took longer than the interval, and the awaited loop paces
 * itself instead. Same idiom `goal-endurance.sh`/`endurance.sh` already use
 * in bash; this is that shape in TypeScript because the slope arithmetic at
 * the end needs real numbers, not shell `awk`.
 *
 * ASSERTIONS: total samples split into 4 equal quarters after the run; the
 * first quarter (warmup: JIT/heap ramp, retention cap not yet reached) is
 * discarded, and each of RSS / heapUsed / SQLite page count must have its
 * final-quarter mean within `TSV2_SOAK_TOLERANCE` (default +10%) of its
 * second-quarter mean. Statement-per-tick flatness reuses
 * `serveLeak.test.ts`'s own check (max of the last 5 perf-log entries <=
 * max of the first 5), read off the SAME `DL_PERF_LOG` JSONL the P0 tracing
 * spine already writes -- no parallel telemetry pipeline.
 *
 * SABOTAGE RECEIPT (`bash scripts/memory-soak.sh` vs
 * `TSV2_SOAK_SABOTAGE=keep_all bash scripts/memory-soak.sh`, run 2026-07-29,
 * reverted -- the env var swaps every `keep(count(cap))` in the churn
 * program for `keep(all)`, i.e. disables the retention DELETE, a real leak).
 * Default config both runs: 2500 ticks, keys=25, cap=200, arrival interval
 * 40ms (~100s), 101 samples, quarters of 25.
 *
 *   HEALTHY:   rss_flat                second-quarter 190,097,000  final-quarter 194,254,000  PASS
 *              heap_used_flat           second-quarter  21,672,067  final-quarter  21,806,888  PASS
 *              sqlite_page_count_flat   second-quarter          10  final-quarter          10  PASS
 *              statements_per_tick_flat second-quarter max 37      final-quarter max 37       PASS
 *              -> MEMORY SOAK HOLDS, exit 0.
 *
 *   SABOTAGED: rss_flat                second-quarter 172,698,501  final-quarter 174,964,081  PASS
 *              heap_used_flat           second-quarter  21,652,350  final-quarter  21,781,295  PASS
 *              sqlite_page_count_flat   second-quarter        17.2  final-quarter        33.2  FAIL (ceiling 19.0)
 *              statements_per_tick_flat second-quarter max 33      final-quarter max 33       PASS
 *              -> TSV2_SOAK_FAIL: sqlite_page_count_flat, exit 1.
 *
 * The page-count assertion is what catches this; RSS/heapUsed stayed
 * comparatively close between the two runs at this scale/duration (the
 * leaked rows are SQLite pages, not retained JS objects) -- which is why
 * this soak asserts SQLite stats and process memory as INDEPENDENT named
 * findings rather than one blended check. A defect that grows the database
 * and a defect that grows the JS heap are different failures and should
 * name themselves differently; a longer or larger-scale run would likely
 * eventually move heapUsed too (more rows means more transient JS row
 * objects per query), but the honest claim is only the one this run
 * actually measured.
 */

import { existsSync, readFileSync, writeFileSync } from "node:fs";

import { serveTsv2 } from "../serve/4_http.ts";
import { postArrivals, postProgram, request } from "../tests/serveHelpers.ts";

interface ChurnConfig {
  readonly keys: number;
  readonly retentionCap: number;
  readonly sabotageKeepAll: boolean;
}

function churnProgram(config: ChurnConfig): string {
  const retention = config.sabotageKeepAll ? "keep(all)" : `keep(count(${config.retentionCap}))`;
  return [
    "rel session(id: int, tag: text) key(1).",
    `rel activity(id: int, tag: text) log ${retention}.`,
    `rel session_activity(id: int, session_tag: text, activity_tag: text) log ${retention}.`,
    "session_activity(id, session_tag, activity_tag) <+ activity(id, activity_tag), latest(session(id, session_tag)).",
    "",
  ].join("\n");
}

interface RunningServer {
  readonly port: number;
  readonly stop: () => void;
}

/** The private, non-retaining subscribe this file's header explains. */
function startServer(port: number): Promise<RunningServer> {
  return new Promise((resolve, reject) => {
    let settled = false;
    const running = serveTsv2({ dbUrl: ":memory:", port }).subscribe({
      next: (event) => {
        if (event.kind === "listening" && !settled) {
          settled = true;
          resolve({ port: event.port, stop: () => running.unsubscribe() });
        }
      },
      error: (failure: unknown) => {
        if (!settled) reject(failure instanceof Error ? failure : new Error(String(failure)));
      },
    });
  });
}

interface Sample {
  readonly atMs: number;
  readonly tick: number;
  readonly rssBytes: number;
  readonly heapUsedBytes: number;
  readonly externalBytes: number;
  readonly pageCount: number;
  readonly freelistCount: number;
  readonly dbBytes: number;
  readonly dbstatAvailable: boolean;
}

interface StatsBody {
  readonly memory: { readonly rssBytes: number; readonly heapUsedBytes: number; readonly externalBytes: number };
  readonly sqlite: { readonly pageCount: number; readonly freelistCount: number; readonly dbBytes: number; readonly dbstatAvailable: boolean };
}

async function sample(port: number, startedAtMs: number, tick: number): Promise<Sample> {
  const gcFn = (global as { readonly gc?: () => void }).gc;
  if (gcFn) gcFn();
  const result = await request(port, "/stats?tables=session,activity,session_activity", "GET");
  if (result.statusCode !== 200) throw new Error(`GET /stats -> ${result.statusCode} ${result.body}`);
  const body = JSON.parse(result.body) as StatsBody;
  return {
    atMs: Date.now() - startedAtMs,
    tick,
    rssBytes: body.memory.rssBytes,
    heapUsedBytes: body.memory.heapUsedBytes,
    externalBytes: body.memory.externalBytes,
    pageCount: body.sqlite.pageCount,
    freelistCount: body.sqlite.freelistCount,
    dbBytes: body.sqlite.dbBytes,
    dbstatAvailable: body.sqlite.dbstatAvailable,
  };
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function mean(values: readonly number[]): number {
  return values.reduce((total, value) => total + value, 0) / values.length;
}

type Quarters<Value> = readonly [readonly Value[], readonly Value[], readonly Value[], readonly Value[]];

/** Four equal slices; drops up to 3 trailing samples so every quarter is the
 *  same size (a partial quarter would bias its own mean). A fixed-length
 *  tuple return type, not `readonly Value[][]`, so callers index `[1]`/`[3]`
 *  without `noUncheckedIndexedAccess` seeing a possibly-missing element. */
function quarters<Value>(items: readonly Value[]): Quarters<Value> {
  const size = Math.floor(items.length / 4);
  return [
    items.slice(0, size),
    items.slice(size, size * 2),
    items.slice(size * 2, size * 3),
    items.slice(size * 3, size * 4),
  ];
}

interface Finding {
  readonly name: string;
  readonly ok: boolean;
  readonly detail: string;
}

function checkFlatMean(name: string, secondQuarterMean: number, finalQuarterMean: number, tolerance: number): Finding {
  const ceiling = secondQuarterMean * (1 + tolerance);
  const ok = finalQuarterMean <= ceiling;
  return {
    name,
    ok,
    detail: `second-quarter mean ${secondQuarterMean.toFixed(1)}, final-quarter mean ${finalQuarterMean.toFixed(1)}, ceiling ${ceiling.toFixed(1)} (tolerance +${(tolerance * 100).toFixed(0)}%)`,
  };
}

interface PerfLine {
  readonly rows: number;
  readonly statements: number;
}

function readStatementCounts(perfLogPath: string): readonly number[] {
  if (!existsSync(perfLogPath)) return [];
  return readFileSync(perfLogPath, "utf8")
    .split("\n")
    .filter((line) => line.trim().length > 0)
    .map((line) => JSON.parse(line) as PerfLine)
    .filter((entry) => entry.rows > 0)
    .map((entry) => entry.statements);
}

/**
 * Post-warmup comparison, same quarters() split as the memory/sqlite checks
 * -- NOT `serveLeak.test.ts`'s literal first-5-vs-last-5 (measured to false-
 * positive here: this soak's retention cap is not yet full for the run's
 * first several dozen ticks, so statements/tick legitimately steps up ONCE
 * when the prune statements start actually deleting rows, then stays flat.
 * Comparing quarter 2 to quarter 4 -- both fully inside the post-warmup
 * steady state at this soak's default cadence (cap reached well inside
 * quarter 1) -- is the honest form of the same count-test law check). */
function checkStatementsFlat(statementCounts: readonly number[]): Finding {
  if (statementCounts.length < 8) {
    return { name: "statements_per_tick_flat", ok: false, detail: `perf log has only ${statementCounts.length} data ticks, need >= 8` };
  }
  const quartered = quarters(statementCounts);
  const secondQuarterMax = Math.max(...quartered[1]);
  const finalQuarterMax = Math.max(...quartered[3]);
  return {
    name: "statements_per_tick_flat",
    ok: finalQuarterMax <= secondQuarterMax,
    detail: `second-quarter max ${secondQuarterMax}, final-quarter max ${finalQuarterMax} (${statementCounts.length} data ticks total)`,
  };
}

async function main(): Promise<void> {
  const port = Number(process.env.TSV2_SOAK_PORT ?? "17571");
  const durationS = Number(process.env.TSV2_SOAK_DURATION_S ?? "100");
  const arrivalIntervalMs = Number(process.env.TSV2_SOAK_ARRIVAL_MS ?? "40");
  const sampleIntervalMs = Number(process.env.TSV2_SOAK_SAMPLE_MS ?? "1000");
  const keys = Number(process.env.TSV2_SOAK_KEYS ?? "25");
  const retentionCap = Number(process.env.TSV2_SOAK_RETENTION_CAP ?? "200");
  const tolerance = Number(process.env.TSV2_SOAK_TOLERANCE ?? "0.10");
  const sabotageKeepAll = process.env.TSV2_SOAK_SABOTAGE === "keep_all";
  const perfLogPath = process.env.DL_PERF_LOG;
  const receiptPath = process.env.TSV2_SOAK_RECEIPT;
  if (perfLogPath === undefined) {
    process.stderr.write("TSV2_SOAK_FAIL: DL_PERF_LOG must be set (scripts/memory-soak.sh sets it)\n");
    process.exitCode = 1;
    return;
  }
  if (receiptPath === undefined) {
    process.stderr.write("TSV2_SOAK_FAIL: TSV2_SOAK_RECEIPT must be set (scripts/memory-soak.sh sets it)\n");
    process.exitCode = 1;
    return;
  }

  const totalTicks = Math.max(1, Math.round((durationS * 1000) / arrivalIntervalMs));
  const sampleEveryNTicks = Math.max(1, Math.round(sampleIntervalMs / arrivalIntervalMs));

  process.stdout.write(
    `memory-soak: port=${port} duration=${durationS}s ticks=${totalTicks} arrival_interval=${arrivalIntervalMs}ms ` +
      `sample_interval=${sampleIntervalMs}ms keys=${keys} retention_cap=${retentionCap} tolerance=${tolerance} ` +
      `sabotage=${sabotageKeepAll ? "keep_all" : "none"}\n`,
  );

  const server = await startServer(port);
  const startedAtMs = Date.now();
  const samples: Sample[] = [];

  try {
    const loaded = await postProgram(server.port, churnProgram({ keys, retentionCap, sabotageKeepAll }));
    if (loaded.statusCode !== 200) throw new Error(`POST /program -> ${loaded.statusCode} ${loaded.body}`);

    // Warmup: every key gets a session row once, so the first churn tick that
    // touches a key already has a `latest(session(...))` match (module header).
    const warmupBatch = Array.from({ length: keys }, (_unused, key) => ({
      rel: "session" as const,
      sign: "add" as const,
      row: [key, `warmup-${key}`] as readonly (string | number)[],
    }));
    await postArrivals(server.port, warmupBatch);

    samples.push(await sample(server.port, startedAtMs, 0));

    for (let tick = 1; tick <= totalTicks; tick += 1) {
      const key = tick % keys;
      await postArrivals(server.port, [
        { rel: "session", sign: "add", row: [key, `tag-${tick}`] },
        { rel: "activity", sign: "add", row: [key, `activity-${tick}`] },
      ]);
      if (tick % sampleEveryNTicks === 0 || tick === totalTicks) {
        samples.push(await sample(server.port, startedAtMs, tick));
      }
      await sleep(arrivalIntervalMs);
    }
  } finally {
    server.stop();
  }

  writeFileSync(receiptPath, samples.map((entry) => JSON.stringify(entry)).join("\n") + "\n", "utf8");

  const findings: Finding[] = [];
  const quartered = quarters(samples);
  if (quartered[0].length < 2) {
    findings.push({ name: "enough_samples", ok: false, detail: `only ${samples.length} samples total, need enough for 4 quarters of >= 2 each` });
  } else {
    const secondQuarter = quartered[1];
    const finalQuarter = quartered[3];
    findings.push(checkFlatMean("rss_flat", mean(secondQuarter.map((entry) => entry.rssBytes)), mean(finalQuarter.map((entry) => entry.rssBytes)), tolerance));
    findings.push(checkFlatMean("heap_used_flat", mean(secondQuarter.map((entry) => entry.heapUsedBytes)), mean(finalQuarter.map((entry) => entry.heapUsedBytes)), tolerance));
    findings.push(checkFlatMean("sqlite_page_count_flat", mean(secondQuarter.map((entry) => entry.pageCount)), mean(finalQuarter.map((entry) => entry.pageCount)), tolerance));
    const dbstatAvailable = samples[samples.length - 1]?.dbstatAvailable ?? false;
    findings.push({ name: "dbstat_available", ok: dbstatAvailable, detail: dbstatAvailable ? "dbstat vtab answered through this driver" : "dbstat unavailable -- PRAGMA-only fallback in effect" });
  }
  findings.push(checkStatementsFlat(readStatementCounts(perfLogPath)));

  for (const finding of findings) {
    process.stdout.write(`${finding.ok ? "PASS" : "FAIL"}  ${finding.name}: ${finding.detail}\n`);
  }
  process.stdout.write(`receipt: ${receiptPath} (${samples.length} samples)\n`);

  // dbstat_available is informational (a fallback exists and is documented,
  // module header); it does not gate PASS/FAIL on its own.
  const gating = findings.filter((finding) => finding.name !== "dbstat_available");
  const allOk = gating.every((finding) => finding.ok);
  if (allOk) {
    process.stdout.write("MEMORY SOAK HOLDS: node memory pressure stayed flat under sustained churn.\n");
    process.exitCode = 0;
  } else {
    const failedNames = gating.filter((finding) => !finding.ok).map((finding) => finding.name);
    process.stderr.write(`TSV2_SOAK_FAIL: ${failedNames.join(", ")}\n`);
    process.exitCode = 1;
  }
}

main().catch((failure: unknown) => {
  process.stderr.write(`${failure instanceof Error ? (failure.stack ?? failure.message) : String(failure)}\n`);
  process.exitCode = 1;
});
