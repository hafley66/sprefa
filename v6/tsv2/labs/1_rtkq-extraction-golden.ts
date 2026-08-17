/**
 * One deterministic RTK Query extraction receipt.
 *
 * Clock:
 *   initial source -> one demand -> one Rust process -> 9 flat capture rows
 *   edit source    -> replace demand -> one Rust process -> 6 capture rows
 *   delete source  -> retract demand and every derived endpoint, no process
 *
 * Four ast-grep patterns share one parse inside each extractor process. DL6
 * performs the create/inject and generic/plain unions, capture joins, and scope
 * containment. Exact half-open byte spans are asserted on the final relations.
 */

import assert from "node:assert/strict";
import diagnostics_channel from "node:diagnostics_channel";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { Observable, Subject, VirtualTimeScheduler } from "rxjs";

import type {
  IServeEffectEvent,
  IServeEvent,
  IServeTickEvent,
  IWatchSource,
} from "../runtime/types.ts";
import { SERVE_CHANNEL_NAMES } from "../serve/0_trace.ts";
import { serve_tsv2 } from "../serve/4_http.ts";

const PROGRAM = fileURLToPath(
  new URL("../../dl/fixtures/1_rtkq-extraction-golden.dl6", import.meta.url),
);
const INITIAL = fileURLToPath(
  new URL("../../sprefa-extract/tests/fixtures/ast_pattern/0_rtkq.ts", import.meta.url),
);
const EDITED = fileURLToPath(
  new URL("../../sprefa-extract/tests/fixtures/ast_pattern/2_rtkq_edited.ts", import.meta.url),
);
const COALESCE_MS = 10;

class ScriptedWatchSource implements IWatchSource {
  readonly paths = new Subject<string>();

  constructor(readonly scheduler: VirtualTimeScheduler) {}

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

function startServer(
  root: string,
  scheduler: VirtualTimeScheduler,
  watch_source: IWatchSource,
): Promise<RunningServer> {
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
      error: (failure: unknown) =>
        reject(failure instanceof Error ? failure : new Error(String(failure))),
    });
  });
}

async function json(port: number, path: string, init?: RequestInit): Promise<unknown> {
  // Without a signal, a server that accepts but never writes headers costs undici's
  // 300s default, which sets the whole green-all makespan. Same deadline as waitUntil.
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    signal: AbortSignal.timeout(10_000),
    ...init,
  });
  const body = await response.text();
  assert.equal(response.status, 200, `${path} -> ${response.status} ${body}`);
  return JSON.parse(body);
}

/** The /idb read, narrowed to its row block. `json` answers `unknown`, so every
 *  caller that reaches for `.rows` needs the shape named once rather than
 *  per call site. */
async function idbRows(port: number, path: string): Promise<unknown[][]> {
  const body = (await json(port, path)) as { readonly rows: unknown[][] };
  return body.rows;
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

const sorted = (rows: unknown[][]) =>
  [...rows].sort((a, b) =>
    JSON.stringify(a).localeCompare(JSON.stringify(b)),
  );

function demandTotals(events: readonly IServeEvent[]) {
  let add = 0;
  let del = 0;
  for (const outcome of ticks(events)) {
    const delta = outcome.deltas.rels.find(
      (candidate) => candidate.rel === "__host_demand_extract",
    );
    add += delta?.add.length ?? 0;
    del += delta?.del.length ?? 0;
  }
  return { add, del };
}

async function main(): Promise<void> {
  const extract = process.env.DL_EXTRACT_BIN;
  assert.ok(extract && extract.length > 0, "DL_EXTRACT_BIN must name the in-tree release extractor");

  const root = mkdtempSync(join(tmpdir(), "tsv2-rtkq-extraction-"));
  const sourcePath = join(root, "api.ts");
  const priorWorkingDirectory = process.cwd();
  process.chdir(root);

  const scheduler = new VirtualTimeScheduler();
  const watchSource = new ScriptedWatchSource(scheduler);
  const effects: IServeEffectEvent[] = [];
  const effectChannel = diagnostics_channel.channel(SERVE_CHANNEL_NAMES.effect);
  const captureEffect = (message: unknown) => effects.push(message as IServeEffectEvent);
  effectChannel.subscribe(captureEffect);
  const server = await startServer(root, scheduler, watchSource);

  try {
    const loaded = (await json(server.port, "/program", {
      method: "POST",
      body: readFileSync(PROGRAM, "utf8"),
    })) as { readonly loaded: boolean };
    assert.equal(loaded.loaded, true);

    writeFileSync(sourcePath, readFileSync(INITIAL));
    watchSource.notify(sourcePath);
    watchSource.settle();
    await waitUntil(() => effects.length === 1, "initial extractor process");

    assert.deepEqual(
      sorted(await idbRows(server.port, "/idb/api_scope")),
      sorted([
        ["api.ts", 271, 475],
        ["api.ts", 505, 643],
      ]),
    );
    assert.deepEqual(
      sorted(await idbRows(server.port, "/idb/api_endpoint")),
      sorted([
        ["api.ts", 271, 475, "getUser", "query", 339, 346],
        ["api.ts", 271, 475, "health", "query", 416, 422],
        ["api.ts", 505, 643, "createOrder", "mutation", 560, 571],
      ]),
    );

    writeFileSync(sourcePath, readFileSync(EDITED));
    watchSource.notify(sourcePath);
    watchSource.settle();
    await waitUntil(() => effects.length === 2, "edited extractor process");

    assert.deepEqual(await json(server.port, "/idb/api_scope"), {
      rows: [["api.ts", 272, 468]],
    });
    assert.deepEqual(
      sorted(await idbRows(server.port, "/idb/api_endpoint")),
      sorted([
        ["api.ts", 272, 468, "listUsers", "query", 407, 416],
        ["api.ts", 272, 468, "updateUser", "mutation", 327, 337],
      ]),
    );

    rmSync(sourcePath, { force: true });
    watchSource.notify(sourcePath);
    watchSource.settle();
    await waitUntil(
      () =>
        ticks(server.events).length >= 5 &&
        demandTotals(server.events).del === 2,
      "deletion retraction tick",
    );

    assert.deepEqual(await json(server.port, "/idb/ast_capture"), { rows: [] });
    assert.deepEqual(await json(server.port, "/idb/api_scope"), { rows: [] });
    assert.deepEqual(await json(server.port, "/idb/endpoint_shape"), { rows: [] });
    assert.deepEqual(await json(server.port, "/idb/api_endpoint"), { rows: [] });

    assert.deepEqual(
      effects.map((effect) => ({
        host: effect.host,
        outcome: effect.outcome,
        rows: effect.rows,
      })),
      [
        { host: "extract", outcome: "done", rows: 9 },
        { host: "extract", outcome: "done", rows: 6 },
      ],
    );
    assert.deepEqual(demandTotals(server.events), { add: 2, del: 2 });
    assert.deepEqual(tickShape(server.events), [
      "1 __host_demand_extract:+1/-0 __host_response_extract:+0/-0 api_endpoint:+0/-0 api_scope:+0/-0 ast_capture:+0/-0 endpoint_shape:+0/-0 file:+1/-0 watch:+1/-0",
      "2 __host_demand_extract:+0/-0 __host_response_extract:+9/-0 api_endpoint:+3/-0 api_scope:+2/-0 ast_capture:+9/-0 endpoint_shape:+3/-0 file:+0/-0 watch:+0/-0",
      "3 __host_demand_extract:+1/-1 __host_response_extract:+0/-0 api_endpoint:+0/-3 api_scope:+0/-2 ast_capture:+0/-9 endpoint_shape:+0/-3 file:+1/-1 watch:+1/-1",
      "4 __host_demand_extract:+0/-0 __host_response_extract:+6/-0 api_endpoint:+2/-0 api_scope:+1/-0 ast_capture:+6/-0 endpoint_shape:+2/-0 file:+0/-0 watch:+0/-0",
      "5 __host_demand_extract:+0/-1 __host_response_extract:+0/-0 api_endpoint:+0/-2 api_scope:+0/-1 ast_capture:+0/-6 endpoint_shape:+0/-2 file:+0/-1 watch:+0/-1",
    ]);

    process.stdout.write(
      `${JSON.stringify({
        ticks: tickShape(server.events),
        demands: demandTotals(server.events),
        processes: effects.length,
        responseRows: effects.map((effect) => effect.rows),
        final: { ast_capture: [], api_scope: [], endpoint_shape: [], api_endpoint: [] },
      })}\n`,
    );
  } finally {
    effectChannel.unsubscribe(captureEffect);
    await server.stop();
    process.chdir(priorWorkingDirectory);
    rmSync(root, { recursive: true, force: true });
  }
}

await main();
