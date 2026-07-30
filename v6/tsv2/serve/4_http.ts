/**
 * 4_http.ts — the served tsv2 engine's http front. curl is the CLI; node:http,
 * localhost, no framework (four literal path shapes; a router library would be
 * more machinery than the problem).
 *
 *   POST /program    text/plain .dl6 -> compile -> boot -> run
 *                    200 {loaded, rels, arrivalTargets, hosts, binds} | 400 {error}
 *   POST /arrivals   {batch:[{rel,sign,row}]} -> one tick (+ its drain ticks)
 *                    200 {ticks:[{tick,line}]} | 409 (no program) | 400 (bad batch)
 *   GET  /idb/:rel   -> {rows} | 404
 *   GET  /ticks      -> SSE, one `data: <tick log line>` per tick
 *   GET  /stats      -> {memory, sqlite} | 404 (no program loaded)
 *                    process.memoryUsage() + PRAGMA/dbstat storage stats
 *                    (runtime/serveStats.ts); `?tables=a,b` scopes the
 *                    dbstat pass, omitted or empty means PRAGMA-only.
 *   anything else    -> 404 {error, routes}
 *
 * THE APP IS ONE COLD OBSERVABLE (`serveTsv2`), subscribed exactly once, in
 * serve/main.ts. Same shape v6/dl's `serveDl` settled on, for the same reasons:
 *
 *   httpServer$ -> mergeMap(server =>
 *        merge( programExchanges$ -> switchMap(runProgram$)   the program's lifetime
 *             , otherExchanges$   -> mergeMap(routeRequest)   one inner per request
 *             , of(listening) ))
 *
 * `switchMap` is what makes a program swap work under one subscription: an
 * accepted program unsubscribes the previous program's whole branch -- its tick
 * loop, its host effects, and its bind timers (an rxjs interval's unsubscribe
 * IS clearInterval) -- and subscribes the new one. Compilation runs BEFORE the
 * switch, so a program that does not compile is answered 400 and the running
 * one keeps running.
 *
 * ARRIVAL VALIDATION lives here, not in the engine: a malformed POST is a
 * client error (400), while a fault raised by the tick loop itself is fatal and
 * must reach main.ts. Mixing the two would turn every engine bug into a 400.
 */

import * as http from "node:http";

import {
  EMPTY,
  Observable,
  type SchedulerLike,
  asyncScheduler,
  catchError,
  concatMap,
  defer,
  filter,
  finalize,
  from,
  fromEvent,
  map,
  merge,
  mergeMap,
  of,
  partition,
  switchMap,
  takeUntil,
  tap,
  toArray,
} from "rxjs";

import { ScratchStore } from "../runtime/scratchStore.ts";
import { ServeStats } from "../runtime/serveStats.ts";
import type {
  IArrivalRow,
  IServeConfig,
  IServeEvent,
  IServedProgram,
  IServeTsv2,
  ISqlSeam,
  ITickOutcome,
} from "../runtime/types.ts";
import { ProgramCompiler } from "./0_compile.ts";
import { HostRunner } from "./1_hosts.ts";
import { IntervalBindRunner, NodeWatchSource, WatchBindRunner, bindPlansFor } from "./2_binds.ts";
import { LiveEngine, bootServedProgram } from "./3_engine.ts";

export const ROUTE_LIST: readonly string[] = [
  "POST /program",
  "POST /arrivals",
  "GET /idb/:rel",
  "GET /ticks",
  "GET /stats",
];

interface Exchange {
  readonly request: http.IncomingMessage;
  readonly response: http.ServerResponse;
}

interface ServerState {
  program: IServedProgram | null;
  seam: ISqlSeam | null;
  engine: LiveEngine | null;
}

function newServerState(): ServerState {
  return { program: null, seam: null, engine: null };
}

function writeJson(response: http.ServerResponse, status: number, body: unknown): void {
  const text = JSON.stringify(body);
  response.writeHead(status, { "content-type": "application/json" });
  response.end(text);
}

/** The one Promise wrapper above the driver seam, and the same one v6/dl still
 *  carries: node's request body is an event stream with no observable form that
 *  is shorter than this. Recorded as standing law debt, not invented here. */
function readBody(request: http.IncomingMessage): Promise<string> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    request.on("data", (chunk: Buffer) => chunks.push(chunk));
    request.on("end", () => resolve(Buffer.concat(chunks).toString("utf8")));
    request.on("error", reject);
  });
}

// ─────────────────────────────────────────────────────────────────────────────
// Program lifetime.
// ─────────────────────────────────────────────────────────────────────────────

interface ProgramLoad {
  readonly program: IServedProgram;
  readonly response: http.ServerResponse;
}

function loadProgram$(exchange: Exchange): Observable<ProgramLoad | null> {
  return from(readBody(exchange.request)).pipe(
    concatMap((source) => ProgramCompiler.compile(source)),
    map((program): ProgramLoad => ({ program, response: exchange.response })),
    catchError((failure: unknown) => {
      writeJson(exchange.response, 400, { error: failure instanceof Error ? failure.message : String(failure) });
      return of(null);
    }),
  );
}

function disposeProgram(state: ServerState): void {
  state.seam?.db.close();
  state.seam = null;
  state.engine = null;
  state.program = null;
}

/** The loaded program's whole life as one observable: the tick loop (subscribing
 *  it is what makes ticks turn), the live host effects, and the bind firings,
 *  all until the next accepted program swaps them out. The 200 is written LAST,
 *  from a `defer` merged after the three, so a client that reads `loaded:true`
 *  and immediately POSTs arrivals cannot beat the tick loop into existence. */
function runProgram$(state: ServerState, config: IServeConfig, load: ProgramLoad): Observable<IServeEvent> {
  const scheduler: SchedulerLike = config.scheduler ?? asyncScheduler;
  return defer(() => {
    disposeProgram(state);
    const seam = ScratchStore.open(config.dbUrl);
    const engine = new LiveEngine(load.program, seam);
    state.seam = seam;
    state.engine = engine;
    state.program = load.program;
    return bootServedProgram(seam, load.program).pipe(
      concatMap(() =>
        merge(
          engine.ticks$.pipe(map((outcome): IServeEvent => ({ kind: "tick", outcome }))),
          new HostRunner(engine, seam, load.program.hostPlans).effects$.pipe(
            map((done): IServeEvent => ({ kind: "effect", done })),
          ),
          new IntervalBindRunner(engine, bindPlansFor(load.program.bindPlans, "live_interval"), scheduler).firings$.pipe(
            map((fired): IServeEvent => ({ kind: "bind", fired })),
          ),
          new WatchBindRunner(engine, bindPlansFor(load.program.bindPlans, "live_watch"), {
            root: config.watchRoot ?? process.cwd(),
            coalesceMs: config.watchCoalesceMs ?? 100,
            scheduler,
            source: config.watchSource ?? new NodeWatchSource(),
          }).firings$.pipe(map((fired): IServeEvent => ({ kind: "watch", fired }))),
          defer(() => {
            writeJson(load.response, 200, {
              loaded: true,
              rels: Object.keys(load.program.relColumns).sort(),
              arrivalTargets: load.program.arrivalTargets,
              hosts: load.program.hostPlans.map((plan) => plan.name),
              binds: load.program.bindPlans.map((plan) => ({ name: plan.name, literals: plan.literals })),
            });
            return of<IServeEvent>({ kind: "loaded", program: load.program.name });
          }),
        ),
      ),
    );
  });
}

// ─────────────────────────────────────────────────────────────────────────────
// Routes.
// ─────────────────────────────────────────────────────────────────────────────

/** A batch is rejected here, before the engine sees it, when it names a rel the
 *  program does not accept arrivals for or carries a row of the wrong width.
 *  Returns the reason, or null when the batch is well formed. */
function batchProblem(program: IServedProgram, batch: readonly IArrivalRow[]): string | null {
  for (const arrival of batch) {
    if (!program.arrivalTargets.includes(arrival.rel)) return `'${arrival.rel}' is not an arrival target`;
    if (arrival.sign !== "add" && arrival.sign !== "del") return `sign must be add or del, got '${String(arrival.sign)}'`;
    const width = program.relColumns[arrival.rel]?.length ?? 0;
    if (arrival.row.length !== width) {
      return `'${arrival.rel}' takes ${width} columns, got ${arrival.row.length}`;
    }
  }
  return null;
}

function handleArrivals$(state: ServerState, exchange: Exchange): Observable<IServeEvent> {
  const { engine, program } = state;
  if (!engine || !program) {
    writeJson(exchange.response, 409, { error: "no program loaded" });
    return of({ kind: "served", method: "POST", path: "/arrivals" });
  }
  return from(readBody(exchange.request)).pipe(
    concatMap((text) => {
      const batch = (JSON.parse(text) as { readonly batch?: readonly IArrivalRow[] }).batch ?? [];
      const problem = batchProblem(program, batch);
      if (problem !== null) {
        writeJson(exchange.response, 400, { error: problem });
        return EMPTY;
      }
      return engine.submit(batch).pipe(toArray());
    }),
    map((outcomes: readonly ITickOutcome[]): IServeEvent => {
      writeJson(exchange.response, 200, {
        ticks: outcomes.map((outcome) => ({ tick: outcome.tick, line: outcome.line })),
      });
      return { kind: "served", method: "POST", path: "/arrivals" };
    }),
  );
}

function handleIdbRead$(state: ServerState, relName: string, response: http.ServerResponse): Observable<IServeEvent> {
  if (!state.engine) {
    writeJson(response, 404, { error: "no program loaded" });
    return of({ kind: "served", method: "GET", path: `/idb/${relName}` });
  }
  return state.engine.rows(relName).pipe(
    map((rows): IServeEvent => {
      writeJson(response, 200, { rows });
      return { kind: "served", method: "GET", path: `/idb/${relName}` };
    }),
  );
}

/** `GET /stats?tables=a,b` -- process memory plus storage stats for the
 *  running program's own seam (runtime/serveStats.ts). 404 with no program
 *  loaded, same convention as `/idb/:rel`: there is no connection to read. */
function handleStats$(state: ServerState, request: http.IncomingMessage, response: http.ServerResponse): Observable<IServeEvent> {
  if (!state.seam) {
    writeJson(response, 404, { error: "no program loaded" });
    return of({ kind: "served", method: "GET", path: "/stats" });
  }
  const tableNames = (new URL(request.url ?? "/", "http://localhost").searchParams.get("tables") ?? "")
    .split(",")
    .map((name) => name.trim())
    .filter((name) => name.length > 0);
  return ServeStats.sqliteSnapshot(state.seam, tableNames).pipe(
    map((sqlite): IServeEvent => {
      writeJson(response, 200, { memory: ServeStats.processMemory(), sqlite });
      return { kind: "served", method: "GET", path: "/stats" };
    }),
  );
}

/** One SSE client as one inner under the app's single subscription. Teardown
 *  law (refCount honesty): a dropped curl closes its socket, `takeUntil` ends
 *  the inner, `finalize` decrements the active count exactly once. */
function ticksClient$(
  state: ServerState,
  request: http.IncomingMessage,
  response: http.ServerResponse,
  bumpActive: (delta: number) => void,
): Observable<IServeEvent> {
  const engine = state.engine;
  if (!engine) {
    writeJson(response, 404, { error: "no program loaded" });
    return of({ kind: "served", method: "GET", path: "/ticks" });
  }
  response.writeHead(200, { "content-type": "text/event-stream", "cache-control": "no-cache", connection: "keep-alive" });
  response.flushHeaders();
  bumpActive(1);

  let endedBySocketClose = false;
  const socketClosed$ = fromEvent(request.socket, "close").pipe(
    tap(() => {
      endedBySocketClose = true;
    }),
  );

  return engine.ticks$.pipe(
    tap((outcome) => response.write(`data: ${outcome.line}\n\n`)),
    map((outcome): IServeEvent => ({ kind: "tick", outcome })),
    takeUntil(socketClosed$),
    finalize(() => {
      bumpActive(-1);
      if (!endedBySocketClose) response.end();
    }),
  );
}

function pathSegments(request: http.IncomingMessage): readonly string[] {
  return new URL(request.url ?? "/", "http://localhost").pathname.split("/").filter((segment) => segment.length > 0);
}

function isProgramLoad(request: http.IncomingMessage): boolean {
  const segments = pathSegments(request);
  return request.method === "POST" && segments.length === 1 && segments[0] === "program";
}

/** The 500 path, shared by every branch: answer if the response is still open,
 *  and report the request as served so one bad request cannot end the app. */
function serveFailure(response: http.ServerResponse, method: string, route: string, failure: unknown): IServeEvent {
  if (response.headersSent) response.end();
  else writeJson(response, 500, { error: failure instanceof Error ? failure.message : String(failure) });
  return { kind: "served", method, path: route };
}

function routeRequest$(state: ServerState, exchange: Exchange, bumpActive: (delta: number) => void): Observable<IServeEvent> {
  const { request, response } = exchange;
  const method = request.method ?? "GET";
  const segments = pathSegments(request);
  const route = `/${segments.join("/")}`;

  const answered = ((): Observable<IServeEvent> => {
    if (method === "POST" && segments.length === 1 && segments[0] === "arrivals") {
      return handleArrivals$(state, exchange);
    }
    if (method === "GET" && segments.length === 2 && segments[0] === "idb") {
      return handleIdbRead$(state, segments[1]!, response);
    }
    if (method === "GET" && segments.length === 1 && segments[0] === "ticks") {
      return ticksClient$(state, request, response, bumpActive);
    }
    if (method === "GET" && segments.length === 1 && segments[0] === "stats") {
      return handleStats$(state, request, response);
    }
    writeJson(response, 404, { error: "not found", routes: ROUTE_LIST });
    return of({ kind: "served", method, path: route });
  })();

  return answered.pipe(catchError((failure: unknown) => of(serveFailure(response, method, route, failure))));
}

// ─────────────────────────────────────────────────────────────────────────────
// serveTsv2: the app. Cold; one subscription (serve/main.ts) runs it.
// ─────────────────────────────────────────────────────────────────────────────

function httpServer$(port: number): Observable<{ readonly server: http.Server; readonly port: number }> {
  return new Observable<{ readonly server: http.Server; readonly port: number }>((subscriber) => {
    const server = http.createServer();
    server.once("error", (failure) => subscriber.error(failure));
    server.listen(port, () => {
      const address = server.address();
      subscriber.next({ server, port: typeof address === "object" && address !== null ? address.port : port });
    });
    return () => {
      server.close();
    };
  });
}

function closeServer$(state: ServerState, server: http.Server): Observable<void> {
  return new Observable<void>((subscriber) => {
    disposeProgram(state);
    server.close(() => {
      subscriber.next(undefined);
      subscriber.complete();
    });
  });
}

export const serveTsv2: IServeTsv2 = (config: IServeConfig): Observable<IServeEvent> =>
  defer(() => {
    const state = newServerState();
    let activeSubscriptions = 0;
    const bumpActive = (delta: number): void => {
      activeSubscriptions += delta;
    };

    return httpServer$(config.port).pipe(
      mergeMap(({ server, port }) => {
        const exchanges$ = fromEvent(
          server,
          "request",
          (request: http.IncomingMessage, response: http.ServerResponse): Exchange => ({ request, response }),
        );
        const [programExchanges$, otherExchanges$] = partition(exchanges$, (exchange) => isProgramLoad(exchange.request));
        const accepted$ = programExchanges$.pipe(
          mergeMap((exchange) => loadProgram$(exchange)),
          filter((load): load is ProgramLoad => load !== null),
        );

        return merge(
          accepted$.pipe(switchMap((load) => runProgram$(state, config, load))),
          otherExchanges$.pipe(mergeMap((exchange) => routeRequest$(state, exchange, bumpActive))),
          of<IServeEvent>({
            kind: "listening",
            port,
            activeSubscribeCount: () => activeSubscriptions,
            close: () => closeServer$(state, server),
          }),
        );
      }),
    );
  });
