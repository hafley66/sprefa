/**
 * 8_leak_soak.test.ts - the long-running server no-leak gate.
 *
 * The test owns one serveDl subscription, exercises only HTTP routes, and samples
 * the same process after every complete swap/commit/SSE cycle. The clock bind uses
 * a long interval so its timer remains present while the old program branch is
 * unsubscribed on every program swap.
 */
import assert from "node:assert/strict";
import http from "node:http";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import type { Subscription } from "rxjs";

import { serveDl, type DlServer } from "../src/6_http.ts";
import { cleanupDbFile, freshDbPath } from "./1_helpers_db.ts";

interface ServerFixture {
  readonly server: DlServer;
  readonly dbPath: string;
  readonly running: Subscription;
}

interface HttpResult {
  readonly statusCode: number;
  readonly body: string;
}

interface ResourceCounts {
  readonly handles: Readonly<Record<string, number>>;
  readonly resources: Readonly<Record<string, number>>;
}

const ITERATION_COUNT = Number(process.env.DL_LEAK_ITERATIONS ?? "20");
const PORT = Number(process.env.DL_LEAK_PORT ?? "17492");
const PERF_LOG_PATH = process.env.DL_PERF_LOG;
const CLOCK_PERIOD_SECONDS = 3600;
const SOAK_ENABLED = PERF_LOG_PATH !== undefined;

const PROGRAM_A = `rel sample(name: text, value: int).
rel derived(name: text).
rel seed(value: text).
rel echo_result(value: text).
rel clock_period(period_secs: int).
rel clock_bucket(period: int, bucket: int).
sh echo(value: text, output: text) = \`printf '{"output":"%s"}\\n' {value}\`.
derived(name) <- sample(name, _).
echo_result(output) <- seed(value), echo?(value, output).
`;

const PROGRAM_B = `rel sample(name: text, value: int).
rel derived(name: text).
rel seed(value: text).
rel echo_result(value: text).
rel marker(value: text).
rel clock_period(period_secs: int).
rel clock_bucket(period: int, bucket: int).
sh echo(value: text, output: text) = \`printf '{"output":"%s"}\\n' {value}\`.
derived(name) <- sample(name, _).
echo_result(output) <- seed(value), echo?(value, output).
`;

function countNames(names: readonly string[]): Readonly<Record<string, number>> {
  const counts: Record<string, number> = {};
  for (const name of names) counts[name] = (counts[name] ?? 0) + 1;
  return counts;
}

function resourceCounts(): ResourceCounts {
  const activeHandles = (process as NodeJS.Process & { _getActiveHandles(): unknown[] })._getActiveHandles();
  const activeHandleNames = activeHandles.map(
    (handle: unknown) => (handle as { readonly constructor?: { readonly name?: string } }).constructor?.name ?? "unknown",
  );
  return {
    handles: countNames(activeHandleNames),
    resources: countNames(process.getActiveResourcesInfo()),
  };
}

function namesWithGrowth(
  before: Readonly<Record<string, number>>,
  after: Readonly<Record<string, number>>,
): readonly string[] {
  const names = new Set([...Object.keys(before), ...Object.keys(after)]);
  return [...names]
    .filter((name) => (after[name] ?? 0) > (before[name] ?? 0))
    .sort()
    .map((name) => `${name} ${before[name] ?? 0}->${after[name] ?? 0}`);
}

function formatCounts(counts: Readonly<Record<string, number>>): string {
  return Object.entries(counts)
    .sort(([nameA], [nameB]) => nameA.localeCompare(nameB))
    .map(([name, count]) => `${name}=${count}`)
    .join(",");
}

async function waitFor(predicate: () => boolean | Promise<boolean>, description: string, timeoutMs = 5_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (!(await predicate())) {
    if (Date.now() >= deadline) throw new Error(`timeout waiting for ${description}`);
    await new Promise<void>((resolve) => setTimeout(resolve, 25));
  }
}

async function settleEventLoop(): Promise<void> {
  await new Promise<void>((resolve) => setImmediate(resolve));
}

function httpRequest(port: number, path: string, method: string, body?: string): Promise<HttpResult> {
  return new Promise((resolve, reject) => {
    const request = http.request(
      {
        hostname: "127.0.0.1",
        port,
        path,
        method,
        agent: false,
        headers: {
          connection: "close",
          ...(body === undefined ? {} : { "content-length": Buffer.byteLength(body).toString() }),
        },
      },
      (response) => {
        const chunks: Buffer[] = [];
        response.on("data", (chunk: Buffer | string) => {
          chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
        });
        response.on("end", () => resolve({ statusCode: response.statusCode ?? 0, body: Buffer.concat(chunks).toString("utf8") }));
        response.on("error", reject);
      },
    );
    request.on("error", reject);
    request.end(body);
  });
}

async function postProgram(port: number, program: string): Promise<void> {
  const response = await httpRequest(port, "/edb/program", "POST", program);
  assert.equal(response.statusCode, 200, `program load failed: ${response.body}`);
}

async function postRows(port: number, relName: string, rows: readonly Record<string, unknown>[]): Promise<void> {
  const response = await httpRequest(port, `/edb/${relName}`, "POST", JSON.stringify({ rows }));
  assert.equal(response.statusCode, 200, `commit failed for ${relName}: ${response.body}`);
}

async function openSse(port: number): Promise<http.ClientRequest> {
  return new Promise((resolve, reject) => {
    const request = http.get(
      {
        hostname: "127.0.0.1",
        port,
        path: "/subscribe/sample",
        agent: false,
        headers: { connection: "close" },
      },
      (response) => {
        response.on("error", () => {});
        response.resume();
        if (response.statusCode !== 200) {
          reject(new Error(`SSE status ${response.statusCode ?? 0}`));
          return;
        }
        resolve(request);
      },
    );
    request.on("error", reject);
  });
}

function perfLines(): readonly { readonly tick: number; readonly stmt_count: number }[] {
  if (!PERF_LOG_PATH) throw new Error("DL_PERF_LOG is required for the leak soak");
  let text: string;
  try {
    text = readFileSync(PERF_LOG_PATH, "utf8");
  } catch (failure: unknown) {
    if (failure instanceof Error && "code" in failure && failure.code === "ENOENT") return [];
    throw failure;
  }
  return text
    .split("\n")
    .filter((line) => line.length > 0)
    .map((line) => JSON.parse(line) as { readonly tick: number; readonly stmt_count: number });
}

async function waitForPerfLine(previousLineCount: number): Promise<readonly { readonly tick: number; readonly stmt_count: number }[]> {
  await waitFor(() => perfLines().length > previousLineCount, `DL_PERF_LOG line ${previousLineCount + 1}`);
  await settleEventLoop();
  return perfLines();
}

async function bootServer(): Promise<ServerFixture> {
  const dbPath = freshDbPath();
  let running: Subscription | undefined;
  const server = await new Promise<DlServer>((resolve, reject) => {
    running = serveDl({ dbPath, port: PORT }).subscribe({
      next: (event) => {
        if (event.kind === "listening") resolve(event.server);
      },
      error: reject,
    });
  });
  return { server, dbPath, running: running! };
}

async function closeServer(fixture: ServerFixture): Promise<void> {
  await fixture.server.close();
  fixture.running.unsubscribe();
  cleanupDbFile(fixture.dbPath);
}

test("long-running server leak soak", { skip: SOAK_ENABLED ? false : "run via scripts/leak-soak.sh" }, async () => {
  assert.ok(Number.isInteger(ITERATION_COUNT) && ITERATION_COUNT >= 20, `iterations=${ITERATION_COUNT}, want >= 20`);
  assert.ok(Number.isInteger(PORT) && PORT >= 17400 && PORT <= 17499, `port=${PORT}, want 174xx`);

  const fixture = await bootServer();
  const statementCounts: number[] = [];
  const rssByIteration: number[] = [];
  let beforeLoop: ResourceCounts | undefined;
  let timerBaseline: number | undefined;

  try {
    await postProgram(fixture.server.port, PROGRAM_A);
    await postRows(fixture.server.port, "clock_period", [{ period_secs: CLOCK_PERIOD_SECONDS }]);
    await postRows(fixture.server.port, "seed", [{ value: "soak-seed" }]);
    await waitFor(async () => {
      const response = await httpRequest(fixture.server.port, "/idb/echo_result", "GET");
      if (response.statusCode !== 200) return false;
      const body = JSON.parse(response.body) as { readonly rows?: readonly unknown[] };
      return (body.rows?.length ?? 0) > 0;
    }, "HostRunner seed response");
    await postProgram(fixture.server.port, PROGRAM_B);
    await settleEventLoop();

    beforeLoop = resourceCounts();
    timerBaseline = beforeLoop.resources.Timeout ?? 0;
    assert.ok(timerBaseline > 0, `clock bind timer is absent from getActiveResourcesInfo(): ${formatCounts(beforeLoop.resources)}`);

    for (let iteration = 1; iteration <= ITERATION_COUNT; iteration += 1) {
      await postProgram(fixture.server.port, iteration % 2 === 0 ? PROGRAM_A : PROGRAM_B);
      await settleEventLoop();

      const timerCountAfterSwap = resourceCounts().resources.Timeout ?? 0;
      assert.equal(timerCountAfterSwap, timerBaseline, `program-swap timer count iteration=${iteration}`);

      const firstRow = [{ name: `soak-${iteration}-first`, value: 1 }];
      const secondRow = [{ name: `soak-${iteration}-second`, value: 1 }];
      let lineCount = perfLines().length;
      await postRows(fixture.server.port, "sample", firstRow);
      const firstLines = await waitForPerfLine(lineCount);

      lineCount = firstLines.length;
      await postRows(fixture.server.port, "sample", secondRow);
      const secondLines = await waitForPerfLine(lineCount);
      statementCounts.push(secondLines.at(-1)!.stmt_count);

      const sseRequest = await openSse(fixture.server.port);
      await waitFor(() => fixture.server.activeSubscribeCount() === 1, `one active SSE subscription iteration=${iteration}`);
      sseRequest.destroy();
      await waitFor(() => fixture.server.activeSubscribeCount() === 0, `SSE teardown iteration=${iteration}`);

      rssByIteration.push(process.memoryUsage().rss);
    }

    await settleEventLoop();
    assert.equal(fixture.server.activeSubscribeCount(), 0);

    const afterLoop = resourceCounts();
    const handleGrowth = namesWithGrowth(beforeLoop!.handles, afterLoop.handles);
    const resourceGrowth = namesWithGrowth(beforeLoop!.resources, afterLoop.resources);
    assert.deepEqual(handleGrowth, [], `active handle growth: ${handleGrowth.join(", ")}`);
    assert.deepEqual(resourceGrowth, [], `active resource growth: ${resourceGrowth.join(", ")}`);

    const rssIteration3 = rssByIteration[2]!;
    const rssAfterLoop = process.memoryUsage().rss;
    const rssLimit = Math.floor(rssIteration3 * 1.25);
    assert.ok(rssAfterLoop <= rssLimit, `RSS grew beyond 25% after warmup: ${rssAfterLoop} > ${rssLimit}`);

    const statementAtIteration5 = statementCounts[4]!;
    const statementAtIteration20 = statementCounts[19]!;
    assert.equal(statementAtIteration20, statementAtIteration5, "identical commit statement count drifted");

    const finalTimerCount = afterLoop.resources.Timeout ?? 0;
    assert.equal(finalTimerCount, timerBaseline, "bind timer count did not return to post-boot baseline");

    process.stdout.write(
      `RECEIPT 1 active handles before=${formatCounts(beforeLoop!.handles)} after=${formatCounts(afterLoop.handles)}; ` +
        `resources before=${formatCounts(beforeLoop!.resources)} after=${formatCounts(afterLoop.resources)}\n`,
    );
    process.stdout.write(`RECEIPT 2 rss iteration3=${rssIteration3} after_loop=${rssAfterLoop} limit=${rssLimit}\n`);
    process.stdout.write(
      `RECEIPT 3 stmt_count iteration5=${statementAtIteration5} iteration20=${statementAtIteration20} ` +
        `perf_lines=${perfLines().length}\n`,
    );
    process.stdout.write(
      `RECEIPT 4 SSE signal=server.activeSubscribeCount() final=${fixture.server.activeSubscribeCount()} ` +
        `iterations=${ITERATION_COUNT}\n`,
    );
    process.stdout.write(`RECEIPT 5 bind timer resource=Timeout post_boot=${timerBaseline} after_loop=${finalTimerCount}\n`);
  } finally {
    await closeServer(fixture);
  }
});
