/**
 * HTTP front for the served tsv2 engine. Routes are literal node:http paths.
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
 * `serveTsv2` is one cold observable subscribed exactly once in serve/main.ts:
 *
 *   httpServer$ -> mergeMap(server =>
 *        merge( programExchanges$ -> switchMap(runProgram$)   the program's lifetime
 *             , otherExchanges$   -> mergeMap(routeRequest)   one inner per request
 *             , of(listening) ))
 *
 * `switchMap` makes a program swap work under one subscription: an
 * accepted program unsubscribes the previous program's whole branch -- its tick
 * loop, its host effects, and its bind timers (an rxjs interval's unsubscribe
 * IS clearInterval) -- and subscribes the new one. Compilation runs BEFORE the
 * switch, so a program that does not compile is answered 400 and the running
 * one keeps running.
 *
 * Arrival validation lives here: a malformed POST is a client error (400). A
 * tick-loop fault is a 500 and
 * reaches only the submitter that caused it -- the tick lane absorbs it and
 * keeps turning (serve/3_engine.ts runBatch, tests/engineFault.test.ts). The
 * two must not be mixed: a 400 for an engine bug would blame the client, and a
 * process exit for a bad row lets any client end the server.
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
  IArrivalBatch,
  IRowColumnType,
  IRowValue,
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

// Program lifetime.

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

/**
 * THE ARRIVAL BOUNDARY. Everything a POST body claims is checked here, before
 * the engine sees any of it, and a failure is a 400 that names the rel and the
 * column rather than a leaked JS TypeError.
 *
 * WHY HERE AND NOT IN EMITTED CODE. This is the only trust boundary: the other
 * two arrival producers (bind timers, host responses) build rows in typed code
 * inside this process, and only http accepts arbitrary bytes. It is also the
 * only place that can name the mistake, because the program's `relColumns` and
 * `relColumnTypes` are in hand here while the emitted `validateArrivals` sees
 * one value and an index. And emitted validation runs INSIDE the tick, after
 * writes have begun, so a rejection there is a partially-applied tick and a
 * 500, never a clean 400. The emitted pass stays what it is: a per-value
 * COERCION (bool to 0/1, -0 to 0) that keeps its own bool/float refusals as a
 * second line for arrivals that never crossed this boundary.
 *
 * The shape this replaced checked three things (target rel, sign, row LENGTH)
 * and never that `row` was an array or that its elements were scalars. Measured
 * consequences: `row: "ab"` killed the whole server, and `row: [{a:1},[2,3]]`
 * answered 200 with a tick log of `{"deltas":{}}` while storing `[null,"[2,3]"]`
 * into a TEXT NOT NULL column, where it polluted every later read. The tick log
 * is the cross-target grading contract; a path that stores a row and prints an
 * empty delta line breaks it in silence. Receipt:
 * tests/serveArrivalValidation.test.ts.
 */
function isRowValue(value: unknown): value is IRowValue {
  return typeof value === "string" || typeof value === "number" || typeof value === "boolean";
}

/**
 * What one value may be, given the column's declared storage type.
 *
 * `int`, `float` and `bool` are exact; the last two match the emitted
 * `validateArrivals` refusals word for word. `text` takes any scalar (SQLite
 * affinity is the contract there) and nothing else -- that arm is the review's
 * measured corruption, an object landing in a TEXT NOT NULL column as NULL.
 *
 * `ref` IS DIFFERENT AND MUST STAY PERMISSIVE. Under the struct-as-rows ruling
 * a struct value arrives WHOLE, as a JSON object, and the engine interns it
 * into its own rel (golden-flex's `tree(tree_id, species, site: patch)` posts
 * `{label, at:{row, col}}`, two levels deep). A first draft of this function
 * required a scalar everywhere and golden-flex went red on exactly that row:
 *   Error: POST /arrivals -> 400 {"error":"'tree' column 'site' must be a
 *   string, number or boolean"}
 * So a ref takes a scalar (its canonical text) or any JSON value, and the only
 * thing refused is the absent one. Whether the object MATCHES the declared
 * struct shape is the type graph's question, not the boundary's.
 *
 * An UNDECLARED column type (a program carrying no `relColumnTypes`) is refused
 * nothing but absence, for the same reason: nothing here knows what it is.
 */
function columnProblem(type: IRowColumnType | undefined, value: unknown): string | null {
  if (value === null || value === undefined) return "must not be null";
  if (type === "int") return Number.isInteger(value) ? null : "must be an int";
  if (type === "float") return typeof value === "number" && Number.isFinite(value) ? null : "must be a float";
  if (type === "bool") return typeof value === "boolean" ? null : "must be a bool";
  // `json` takes exactly what `text` takes, and deliberately so: it WAS text
  // at this seam until the tick-log encoder needed the two separated, and
  // widening what the boundary accepts is a different decision from teaching
  // the log how to print it. A json document arrives as its text.
  if (type === "text" || type === "json") return isRowValue(value) ? null : "must be a string, number or boolean";
  return null;
}

type BatchCheck =
  | { readonly kind: "ok"; readonly batch: IArrivalBatch }
  | { readonly kind: "bad"; readonly problem: string };

function checkArrivalBody(program: IServedProgram, text: string): BatchCheck {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return { kind: "bad", problem: "body is not valid json" };
  }
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    return { kind: "bad", problem: "body must be a json object carrying a 'batch' array" };
  }
  const batch = (parsed as { readonly batch?: unknown }).batch ?? [];
  if (!Array.isArray(batch)) return { kind: "bad", problem: "'batch' must be an array" };

  for (const [index, entry] of batch.entries()) {
    if (typeof entry !== "object" || entry === null || Array.isArray(entry)) {
      return { kind: "bad", problem: `arrival ${index} must be an object carrying rel, sign and row` };
    }
    const arrival = entry as { readonly rel?: unknown; readonly sign?: unknown; readonly row?: unknown };
    if (typeof arrival.rel !== "string" || !program.arrivalTargets.includes(arrival.rel)) {
      return { kind: "bad", problem: `'${String(arrival.rel)}' is not an arrival target` };
    }
    if (arrival.sign !== "add" && arrival.sign !== "del") {
      return { kind: "bad", problem: `sign must be add or del, got '${String(arrival.sign)}'` };
    }
    const columns = program.relColumns[arrival.rel] ?? [];
    if (!Array.isArray(arrival.row)) {
      return { kind: "bad", problem: `'${arrival.rel}' row must be an array of ${columns.length} values` };
    }
    if (arrival.row.length !== columns.length) {
      return { kind: "bad", problem: `'${arrival.rel}' takes ${columns.length} columns, got ${arrival.row.length}` };
    }
    const types = program.relColumnTypes?.[arrival.rel];
    for (const [column, value] of (arrival.row as readonly unknown[]).entries()) {
      const problem = columnProblem(types?.[column], value);
      if (problem !== null) {
        return { kind: "bad", problem: `'${arrival.rel}' column '${columns[column]}' ${problem}` };
      }
    }
  }
  // Every field of every arrival has now been checked against the program's own
  // declaration, which is what makes this the one honest place to name the type.
  return { kind: "ok", batch: batch as IArrivalBatch };
}

function handleArrivals$(state: ServerState, exchange: Exchange): Observable<IServeEvent> {
  const { engine, program } = state;
  if (!engine || !program) {
    writeJson(exchange.response, 409, { error: "no program loaded" });
    return of({ kind: "served", method: "POST", path: "/arrivals" });
  }
  return from(readBody(exchange.request)).pipe(
    concatMap((text) => {
      const checked = checkArrivalBody(program, text);
      if (checked.kind === "bad") {
        writeJson(exchange.response, 400, { error: checked.problem });
        return EMPTY;
      }
      return engine.submit(checked.batch).pipe(toArray());
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

/**
 * The DRAIN-THEN-DISPOSE contract. `server.close(callback)` stops accepting new
 * connections and calls back only once every open one has ended, so an /idb read
 * already in flight still gets its answer.
 *
 * The order matters and used to be the other way round: `disposeProgram` ran
 * FIRST, which closed the sqlite handle out from under any request still being
 * served, and a client that had issued its last reads got a dead socket or a 500
 * after a run that had otherwise succeeded (bug serve_lifecycle_idb_read_race --
 * the flow rig lost its paired TSVs exactly here). Nothing about the old order
 * was needed: the program's rows are only reachable THROUGH a request, so there
 * is no reader left to protect once close's callback has fired.
 */
function closeServer$(state: ServerState, server: http.Server): Observable<void> {
  return new Observable<void>((subscriber) => {
    server.close(() => {
      disposeProgram(state);
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
