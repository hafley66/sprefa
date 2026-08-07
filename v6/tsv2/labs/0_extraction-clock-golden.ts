/**
 * 0_extraction-clock-golden.ts -- one deterministic live clock receipt.
 *
 * The watcher backend is driven, but its shipped digest/diff/coalesce code is
 * still the path under test. The host is the in-tree release `extract` binary.
 *
 * Clock:
 *   write a.ts (eval + JSON.parse) -> watch tick -> extract demand
 *   extractor JSONL (two site records) -> one response tick -> two findings
 *   replace a.ts with a call-free file -> watch tick -> old findings retract
 *
 * The receipt deliberately pins only stable information from ticks: relation
 * names and arrival counts. Digests are content-addressed opaque values and
 * are asserted through their observable replacement behaviour instead.
 */

import assert from "node:assert/strict";
import diagnostics_channel from "node:diagnostics_channel";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { Observable, Subject, VirtualTimeScheduler } from "rxjs";

import { SERVE_CHANNEL_NAMES } from "../serve/0_trace.ts";
import { serve_tsv2 } from "../serve/4_http.ts";
import type { IServeEvent, IServeTickEvent, IWatchSource } from "../runtime/types.ts";

const PROGRAM = fileURLToPath(new URL("../../dl/fixtures/0_extraction-clock-golden.dl6", import.meta.url));
const COALESCE_MS = 10;

class ScriptedWatchSource implements IWatchSource {
  private readonly paths = new Subject<string>();

  constructor(private readonly scheduler: VirtualTimeScheduler) {}

  watch(): Observable<string> {
    return this.paths.asObservable();
  }

  notify(path: string): void {
    this.paths.next(path);
  }

  settle(): void {
    this.scheduler.maxFrames = this.scheduler.frame + COALESCE_MS * 2;
    this.scheduler.flush();
  }
}

type RunningServer = {
  readonly port: number;
  readonly events: IServeEvent[];
  readonly stop: () => Promise<void>;
};

function startServer(root: string, scheduler: VirtualTimeScheduler, watch_source: IWatchSource): Promise<RunningServer> {
  return new Promise((resolve, reject) => {
    const events: IServeEvent[] = [];
    let listening = false;
    const subscription = serve_tsv2({
      db_url: ":memory:",
      port: 0,
      scheduler,
      watch_root: root,
      watch_coalesce_ms: COALESCE_MS,
      watch_source,
    }).subscribe({
      next: (event) => {
        events.push(event);
        if (event.kind === "listening" && !listening) {
          listening = true;
          resolve({
            port: event.port,
            events,
            stop: async () => {
              subscription.unsubscribe();
              await new Promise<void>((done) => setTimeout(done, 25));
            },
          });
        }
      },
      error: (failure: unknown) => reject(failure instanceof Error ? failure : new Error(String(failure))),
    });
  });
}

async function json(port: number, path: string, init?: RequestInit): Promise<unknown> {
  // A server that accepts but never writes headers otherwise costs undici's 300s default.
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    signal: AbortSignal.timeout(10_000),
    ...init,
  });
  const body = await response.text();
  assert.equal(response.status, 200, `${path} -> ${response.status} ${body}`);
  return JSON.parse(body);
}

async function waitUntil(predicate: () => boolean, what: string): Promise<void> {
  const deadline = Date.now() + 10_000;
  while (!predicate()) {
    if (Date.now() >= deadline) throw new Error(`timeout waiting for ${what}`);
    await new Promise<void>((done) => setTimeout(done, 10));
  }
}

function ticks(events: readonly IServeEvent[]) {
  return events.flatMap((event) => (event.kind === "tick" ? [event.outcome] : []));
}

function tickShape(events: readonly IServeEvent[]): readonly string[] {
  return ticks(events).map((outcome) =>
    `${outcome.tick} ${outcome.deltas.rels
      .map((delta) => `${delta.rel}:+${delta.add.length}/-${delta.del.length}`)
      .join(" ")}`,
  );
}

async function main(): Promise<void> {
  const extract = process.env.DL_EXTRACT_BIN;
  assert.ok(extract && extract.length > 0, "DL_EXTRACT_BIN must name the in-tree release extractor");

  const root = mkdtempSync(join(tmpdir(), "tsv2-extraction-clock-"));
  const sourcePath = join(root, "a.ts");
  const priorWorkingDirectory = process.cwd();
  // The host template takes the watcher's relative `path`; the served process
  // convention is therefore that its cwd is the watched root.
  process.chdir(root);
  const scheduler = new VirtualTimeScheduler();
  const watchSource = new ScriptedWatchSource(scheduler);
  const spans: IServeTickEvent[] = [];
  const tickChannel = diagnostics_channel.channel(SERVE_CHANNEL_NAMES.tick);
  const capture = (message: unknown) => spans.push(message as IServeTickEvent);
  tickChannel.subscribe(capture);
  const server = await startServer(root, scheduler, watchSource);

  try {
    const programResponse = await fetch(`http://127.0.0.1:${server.port}/program`, {
      method: "POST",
      body: readFileSync(PROGRAM, "utf8"),
      signal: AbortSignal.timeout(10_000),
    });
    const programText = await programResponse.text();
    assert.equal(programResponse.status, 200, `/program -> ${programResponse.status} ${programText}`);
    const program = JSON.parse(programText) as { readonly loaded: boolean };
    assert.equal(program.loaded, true);

    writeFileSync(
      sourcePath,
      [
        "export function first(input: string): unknown { return eval(input); }",
        "export function second(input: string): unknown { return JSON.parse(input); }",
        "",
      ].join("\n"),
    );
    watchSource.notify(sourcePath);
    watchSource.settle();
    await waitUntil(() => ticks(server.events).length === 2, "two ticks after the two-row extract response");

    assert.deepEqual(await json(server.port, "/idb/finding"), {
      rows: [["a.ts", "eval"], ["a.ts", "parse"]],
    });
    assert.deepEqual(await json(server.port, "/idb/banned_call"), { rows: [["a.ts", "eval"]] });

    writeFileSync(sourcePath, "export const identity = (input: string): string => input;\n");
    watchSource.notify(sourcePath);
    watchSource.settle();
    await waitUntil(() => ticks(server.events).length === 3, "the edit retraction tick");

    assert.deepEqual(tickShape(server.events), [
      "1 __host_demand_extract:+1/-0 __host_response_extract:+0/-0 banned_call:+0/-0 call_site:+0/-0 file:+1/-0 finding:+0/-0 watch:+1/-0",
      "2 __host_demand_extract:+0/-0 __host_response_extract:+2/-0 banned_call:+1/-0 call_site:+2/-0 file:+0/-0 finding:+2/-0 watch:+0/-0",
      "3 __host_demand_extract:+1/-1 __host_response_extract:+0/-0 banned_call:+0/-1 call_site:+0/-2 file:+1/-1 finding:+0/-2 watch:+1/-1",
    ]);
    assert.deepEqual(await json(server.port, "/idb/call_site"), { rows: [] });
    assert.deepEqual(await json(server.port, "/idb/finding"), { rows: [] });
    assert.deepEqual(await json(server.port, "/idb/banned_call"), { rows: [] });

    const responseSpan = spans.find((span) => span.tick === 2);
    assert.ok(responseSpan, "the host response tick must publish one statement span");
    assert.equal(responseSpan.rows, 7, "two response rows cross one response tick and derive two findings plus one rail row");
    assert.equal(responseSpan.statements, 59, "the two-row response tick statement count is pinned");

    process.stdout.write(`${JSON.stringify({ ticks: tickShape(server.events), final: { call_site: [], finding: [], banned_call: [] }, responseStatements: responseSpan.statements })}\n`);
  } finally {
    tickChannel.unsubscribe(capture);
    await server.stop();
    process.chdir(priorWorkingDirectory);
    rmSync(root, { recursive: true, force: true });
  }
}

await main();
