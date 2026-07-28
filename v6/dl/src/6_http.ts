/**
 * 6_http.ts — the http front. curl is the CLI. node:http, localhost, no auth, no
 * framework (routing here is six literal path shapes; a router library would be
 * more machinery than the problem).
 *
 * Contract (plan M6, tasks.d.ts HttpSurface):
 *   POST /edb/program        text/plain .dl -> bridge -> runtime (re)boot
 *                             200 {loaded,rels,minted} | 400 {diags}
 *   POST /edb/file_changed   {path} -> ingestFile -> TickReport | 409 (no program)
 *   POST /edb/:rel           {rows} -> commit insert batch -> TickReport
 *                             | 409 (no program) | 400 (unknown rel)
 *   GET  /idb/:rel           -> {rows} | 404 (no program / unknown rel)
 *   GET  /subscribe/:rel     -> SSE DeltaEvent stream; unsubscribes on socket close
 *                             | 404 (no program / unknown rel)
 *   POST /query              `? rel(args).` -> one-shot SELECT -> {rows} | 409 (no program)
 *   anything else            -> 404 {error, routes}
 * SSE = deltas$.pipe(filter(byRel)) per connection, `data: {...}\n\n`; teardown on
 * socket close (refCount honesty — a dropped curl unsubscribes, proven by
 * activeSubscribeCount() returning to baseline in tests/6_http.test.ts).
 *
 * THE APP IS ONE COLD OBSERVABLE (`serveDl`), subscribed exactly once, at the bottom of
 * main.ts. Shape:
 *
 *   httpServer$ -> mergeMap(node =>
 *        merge( programExchanges$ -> switchMap(runProgram$)     the program's lifetime
 *             , otherExchanges$   -> mergeMap(routeRequest)     one inner per request
 *             , of(listening)                                   the DlServer handle ))
 *
 * `switchMap` is what makes a program swap work under one subscription: a new accepted
 * program unsubscribes the previous program's branch (its tick stream and its host
 * effects) and subscribes the new one. The bridge runs BEFORE that switch, so a
 * rejected program (400) leaves the running one untouched. Requests are `fromEvent` on
 * the server, so nothing pushes into a Subject to get into the graph, and every branch
 * emits a DlAppEvent rather than throwing its values away.
 *
 * Server state: ONE mutable slot (ServerState below), empty until a program loads via
 * POST /edb/program — the request branch and the program branch are siblings under the
 * merge, so the slot is how a request finds the program currently loaded. A re-POST
 * disposes the previous runtime + side db connection, then boots fresh — no two-worlds
 * split, no partial-reload machinery. The previous HostRunner needs no disposal: it is
 * pure composition over the runtime's deltas$, and the switch unsubscribes it.
 *
 * Second SQLite connection (sideDb): per the owner's own pinned resolution for
 * HostRunner's cacheDb (1_hosts.ts header: "the same db the runtime booted with...
 * or a fresh open_db() on the same file path; both are documented as
 * acceptable"), this file opens ONE extra libsql connection per program load and
 * reuses it for both HostRunner's effect_cache reads/writes AND this file's own
 * raw SELECT for POST /query (runtime.rows() has no WHERE-clause reader — the
 * DlRuntime contract in 0_types.ts only exposes commit/rows(rel)/deltas$/dispose,
 * by design; a second connection to the SAME on-disk file is not a new storage
 * scheme, it is the same pattern tests/2_helpers_hosts.ts already uses). Both
 * connections speak to one SQLite file; libsql/sqlite's own locking arbitrates.
 */
import * as http from "node:http";
import path from "node:path";

import {
  EMPTY,
  Observable,
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
} from "rxjs";

import { open_db, type SqliteDb } from "sprefa-store-engine/src/engine/lib.ts";

import { bridge } from "./0_ast_bridge.ts";
import { builtinExtract, builtinSg, HostRunner, shHost, type CacheDb, type HostDef } from "./1_hosts.ts";
import { DlRuntime } from "./3_runtime.ts";
import { DL_ROOT, ingestFile } from "./4_ingest.ts";
import { builtinDecls, DIAG_V5_VIEW_SQL } from "./5_diag.ts";
import type {
  AssertTrue,
  BridgeOk,
  DeltaEvent,
  DlAppEvent,
  DlServer,
  LoadDiag,
  Row,
  ServeDl,
  TickReport,
  Value,
} from "./0_types.ts";
import type { RelDecl } from "sprefa-store-engine/src/lower/ast.ts";

// DlServer (the handle serveDl emits once it is listening) is declared in 0_types.ts
// (M7: a cross-file contract -- tests/6_http.test.ts imports the type) and re-exported
// here so that existing `from "./6_http.ts"` importers keep working unchanged.
export type { DlServer };

/** One http request and the response it is owed, as a value the graph can carry. */
interface Exchange {
  readonly request: http.IncomingMessage;
  readonly response: http.ServerResponse;
}

// ─────────────────────────────────────────────────────────────────────────────
// Route list.
// ─────────────────────────────────────────────────────────────────────────────

export const ROUTE_LIST: readonly string[] = [
  "POST /edb/program",
  "POST /edb/file_changed",
  "POST /edb/:rel",
  "GET /idb/:rel",
  "GET /subscribe/:rel",
  "POST /query",
];

// ─────────────────────────────────────────────────────────────────────────────
// Server state: one mutable slot, empty until a program loads.
// ─────────────────────────────────────────────────────────────────────────────

interface ServerState {
  bridgeOk: BridgeOk | null;
  runtime: DlRuntime | null;
  sideDb: SqliteDb | null;
}

function newServerState(): ServerState {
  return { bridgeOk: null, runtime: null, sideDb: null };
}

async function disposeCurrentProgram(state: ServerState): Promise<void> {
  if (state.runtime) {
    await state.runtime.dispose();
    state.runtime = null;
  }
  if (state.sideDb) {
    state.sideDb.close();
    state.sideDb = null;
  }
  state.bridgeOk = null;
}

function relDeclOf(state: ServerState, relName: string): RelDecl | undefined {
  return state.bridgeOk?.program.rels.find((decl) => decl.name === relName);
}

// ─────────────────────────────────────────────────────────────────────────────
// Value / body plumbing.
// ─────────────────────────────────────────────────────────────────────────────

function normalizeQueryValue(raw: unknown): Value {
  if (raw === undefined || raw === null) return null;
  if (typeof raw === "bigint") return Number(raw);
  if (typeof raw === "number" || typeof raw === "string" || typeof raw === "boolean") return raw;
  return String(raw);
}

function rowFromRawQueryResult(rawRow: unknown, columns: readonly string[]): Row {
  const rawRecord = rawRow as Record<string, unknown>;
  const row: Record<string, Value> = {};
  for (const column of columns) row[column] = normalizeQueryValue(rawRecord[column]);
  return row as Row;
}

function resolvePathSurface(rows: readonly Row[]): Row[] {
  return rows.map((row) => {
    if (typeof row.path !== "string") return row;
    return { ...row, path: path.resolve(DL_ROOT, row.path) };
  });
}

function normalizePathSurface(rows: readonly Row[]): Row[] {
  return rows.map((row) => {
    if (typeof row.path !== "string") return row;
    return { ...row, path: path.relative(DL_ROOT, path.resolve(DL_ROOT, row.path)) };
  });
}

/** Duplicated in miniature (not imported): 3_runtime.ts's sqlLiteral/sqlAnyRowMatch
 *  are module-private, and this file's only WHERE-building need is /query's
 *  Lit-arg conjunction (NULL-safe `col IS <lit>`, same convention). */
function sqlLiteralForQuery(value: Value): string {
  if (value === null || value === undefined) return "NULL";
  if (typeof value === "number") return String(value);
  if (typeof value === "boolean") return value ? "1" : "0";
  return `'${value.replace(/'/g, "''")}'`;
}

function readBody(req: http.IncomingMessage): Promise<string> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    req.on("data", (chunk: Buffer) => chunks.push(chunk));
    req.on("end", () => resolve(Buffer.concat(chunks).toString("utf8")));
    req.on("error", reject);
  });
}

function writeJson(res: http.ServerResponse, status: number, body: unknown): void {
  const text = JSON.stringify(body);
  res.writeHead(status, { "content-type": "application/json" });
  res.end(text);
}

// ─────────────────────────────────────────────────────────────────────────────
// Route handlers. Each takes the shared state (+ cfg where a fresh boot needs the db
// path) explicitly, mirroring 3_runtime.ts's explicit-state-over-`this` convention.
// ─────────────────────────────────────────────────────────────────────────────

/** An accepted program and the response still owed to whoever POSTed it. A rejected
 *  program is answered right here (400) and never reaches the switch, so the program
 *  already running keeps running. */
interface ProgramLoad {
  readonly bridgeOk: BridgeOk;
  readonly response: http.ServerResponse;
}

async function readProgram(exchange: Exchange): Promise<ProgramLoad | null> {
  const dlText = await readBody(exchange.request);
  const result = bridge(dlText, builtinDecls);
  if (result.kind === "err") {
    writeJson(exchange.response, 400, { diags: result.diags satisfies readonly LoadDiag[] });
    return null;
  }
  return { bridgeOk: result, response: exchange.response };
}

async function bootProgram(
  state: ServerState,
  config: { readonly dbPath: string },
  load: ProgramLoad,
): Promise<{ readonly runtime: DlRuntime; readonly hostRunner: HostRunner }> {
  await disposeCurrentProgram(state);

  const result = load.bridgeOk;
  const runtime = await DlRuntime.boot({ dbPath: config.dbPath, bridge: result, extraDdl: [DIAG_V5_VIEW_SQL] });
  const sideDb = open_db(`file:${config.dbPath}`);
  // Order matters: HostRunner keys hosts by name (last write wins on a collision).
  // Builtins are listed LAST on purpose so they win over a same-named `sh` decl in
  // the loaded program. EMPIRICALLY FOUND (2026-07-24, running this exact golden):
  // sg-rail.dl declares its own `sh sg(...)` with no `jq` reshape of ast-grep's
  // nested `--json` output (range.byteOffset.start/end); shHost's generic parser
  // falls back to positional Object.values() zipping when top-level keys don't
  // match, producing garbage rows (a stringified `range` object landing in `end`,
  // etc.) -- which then never joins span_line and starves `diag` forever. Builtins
  // winning is the only ordering under which the demo program's declared `sg`
  // (identical name, no reshape) doesn't silently shadow the working implementation.
  const hosts: HostDef[] = [...result.hosts.map(shHost), builtinSg, builtinExtract];
  const hostRunner = new HostRunner(runtime, hosts, sideDb as unknown as CacheDb);

  state.bridgeOk = result;
  state.runtime = runtime;
  state.sideDb = sideDb;

  return { runtime, hostRunner };
}

/** The loaded program's whole life as one observable: the tick stream (subscribing it
 *  is what makes the tick loop turn) and the host effects, both live until the next
 *  accepted program swaps them out. The 200 is written LAST, from a `defer` merged
 *  after those two, so a client that reads `loaded:true` and immediately POSTs a row
 *  cannot beat the tick loop into existence. */
function runProgram$(
  state: ServerState,
  config: { readonly dbPath: string },
  load: ProgramLoad,
): Observable<DlAppEvent> {
  // Only a BOOT failure is caught here (answer 500, no program runs). A fault raised
  // later by the tick loop or by a host effect stays on the stream and reaches main.ts,
  // which is fatal — the same loudness the old keepAlive's rethrow had.
  const booted$ = from(bootProgram(state, config, load)).pipe(
    catchError((failure: unknown) => {
      serveFailure(load.response, "POST", "/edb/program", failure);
      return EMPTY;
    }),
  );

  return booted$.pipe(
    concatMap(({ runtime, hostRunner }) =>
      merge(
        runtime.deltas$.pipe(map((delta): DlAppEvent => ({ kind: "delta", delta }))),
        hostRunner.effects$.pipe(map((done): DlAppEvent => ({ kind: "effect", done }))),
        defer(() => {
          writeJson(load.response, 200, {
            loaded: true,
            rels: load.bridgeOk.program.rels.map((decl) => decl.name),
            minted: load.bridgeOk.minted,
          });
          return of<DlAppEvent>({ kind: "served", method: "POST", path: "/edb/program" });
        }),
      ),
    ),
  );
}

async function handleFileChanged(state: ServerState, request: http.IncomingMessage, response: http.ServerResponse): Promise<void> {
  if (!state.runtime) {
    writeJson(response, 409, { error: "no program loaded" });
    return;
  }
  const body = JSON.parse(await readBody(request)) as { path: string };
  const report: TickReport = await ingestFile(state.runtime, body.path);
  writeJson(response, 200, report);
}

async function handleEdbInsert(
  state: ServerState,
  relName: string,
  request: http.IncomingMessage,
  response: http.ServerResponse,
): Promise<void> {
  if (!state.runtime || !state.bridgeOk) {
    writeJson(response, 409, { error: "no program loaded" });
    return;
  }
  if (!relDeclOf(state, relName)) {
    writeJson(response, 400, { error: `unknown rel '${relName}'` });
    return;
  }
  const body = JSON.parse(await readBody(request)) as { rows?: Row[] };
  const report: TickReport = await state.runtime.commit({
    insert: new Map([[relName, normalizePathSurface(body.rows ?? [])]]),
    retract: new Map(),
  });
  writeJson(response, 200, report);
}

async function handleIdbRead(state: ServerState, relName: string, response: http.ServerResponse): Promise<void> {
  if (!state.runtime || !relDeclOf(state, relName)) {
    writeJson(response, 404, { error: `unknown rel '${relName}'` });
    return;
  }
  const rows = await state.runtime.rows(relName);
  writeJson(response, 200, { rows: resolvePathSurface(rows) });
}

/** One SSE client as one inner observable under the app's single subscription: the
 *  filtered delta stream, written to the socket as it flows.
 *
 *  Teardown law: a dropped curl (socket close) must unsubscribe -- refCount honesty.
 *  request.socket's "close" is the one reliable signal across both a graceful client
 *  disconnect and an abrupt one (curl -N killed, `response.destroy()` from the client
 *  side, etc.), and `takeUntil` on it ends this inner. `finalize` then returns the
 *  count to baseline exactly once, whether the end came from the socket, from the
 *  runtime's stream completing (program disposed), or from the app graph itself
 *  shutting down. */
function sseClient$(
  state: ServerState,
  relName: string,
  request: http.IncomingMessage,
  response: http.ServerResponse,
  bumpActive: (delta: number) => void,
): Observable<DlAppEvent> {
  const runtime = state.runtime;
  const path = `/subscribe/${relName}`;
  if (!runtime || !relDeclOf(state, relName)) {
    writeJson(response, 404, { error: `unknown rel '${relName}'` });
    return of({ kind: "served", method: "GET", path });
  }

  response.writeHead(200, {
    "content-type": "text/event-stream",
    "cache-control": "no-cache",
    connection: "keep-alive",
  });
  response.flushHeaders();
  bumpActive(1);

  return runtime.deltas$.pipe(
    filter((event: DeltaEvent) => event.rel === relName),
    tap((event) => response.write(`data: ${JSON.stringify(event)}\n\n`)),
    map((delta): DlAppEvent => ({ kind: "delta", delta })),
    takeUntil(fromEvent(request.socket, "close")),
    finalize(() => bumpActive(-1)),
  );
}

async function handleQuery(state: ServerState, request: http.IncomingMessage, response: http.ServerResponse): Promise<void> {
  if (!state.runtime || !state.bridgeOk || !state.sideDb) {
    writeJson(response, 409, { error: "no program loaded" });
    return;
  }
  const queryText = await readBody(request);
  // A lone `? rel(args).` line is a valid Program on its own (grammar: statements*).
  // The current program's rels are passed as builtinRels so the query resolves
  // without re-declaring anything.
  const result = bridge(queryText, state.bridgeOk.program.rels);
  if (result.kind === "err") {
    writeJson(response, 400, { diags: result.diags });
    return;
  }
  const queryRef = result.queries[0];
  if (!queryRef) {
    writeJson(response, 400, { error: "no query statement found (expected `? rel(args).`)" });
    return;
  }
  const decl = relDeclOf(state, queryRef.rel);
  if (!decl) {
    writeJson(response, 400, { error: `unknown rel '${queryRef.rel}'` });
    return;
  }

  const whereClauses: string[] = [];
  queryRef.args.forEach((argument, index) => {
    const column = decl.columns[index];
    if (column === undefined) return; // trailing elision: unconstrained
    if (argument.kind === "lit") {
      const value = column === "path" && typeof argument.value === "string"
        ? path.relative(DL_ROOT, path.resolve(DL_ROOT, argument.value))
        : argument.value;
      whereClauses.push(`${column} IS ${sqlLiteralForQuery(value)}`);
    }
    // "var"/"wild": unconstrained (a one-shot query has no other body atom to bind
    // a Var against, so a bound-looking var reads exactly like a wildcard here).
  });
  const whereSql = whereClauses.length > 0 ? ` WHERE ${whereClauses.join(" AND ")}` : "";
  const sql = `SELECT ${decl.columns.join(",")} FROM rel_${decl.name}${whereSql}`;

  const sqlResult = await state.sideDb.execute(sql);
  const rows = sqlResult.rows.map((rawRow) => rowFromRawQueryResult(rawRow, decl.columns));
  writeJson(response, 200, { rows: resolvePathSurface(rows) });
}

// ─────────────────────────────────────────────────────────────────────────────
// Router: six literal path shapes, matched by method + split segments.
// ─────────────────────────────────────────────────────────────────────────────

function pathSegments(request: http.IncomingMessage): readonly string[] {
  return new URL(request.url ?? "/", "http://localhost").pathname.split("/").filter((segment) => segment.length > 0);
}

function isProgramLoad(request: http.IncomingMessage): boolean {
  const segments = pathSegments(request);
  return request.method === "POST" && segments.length === 2 && segments[0] === "edb" && segments[1] === "program";
}

/** The 500 path, shared by every branch: answer if the response is still open, and
 *  report the request as served so one bad request cannot end the app's stream. */
function serveFailure(
  response: http.ServerResponse,
  method: string,
  path: string,
  failure: unknown,
): DlAppEvent {
  if (response.headersSent) response.end();
  else writeJson(response, 500, { error: failure instanceof Error ? failure.message : String(failure) });
  return { kind: "served", method, path };
}

/** Every route except POST /edb/program (that one owns the program's lifetime, so it
 *  rides its own switchMap branch). Handlers stay `async` and write their own response;
 *  this wraps each one as a one-value inner so the app graph can hold them all. */
function routeRequest(
  state: ServerState,
  exchange: Exchange,
  bumpActive: (delta: number) => void,
): Observable<DlAppEvent> {
  const { request, response } = exchange;
  const method = request.method ?? "GET";
  const segments = pathSegments(request);
  const path = `/${segments.join("/")}`;
  const served = (work: Promise<void>): Observable<DlAppEvent> =>
    from(work).pipe(map((): DlAppEvent => ({ kind: "served", method, path })));

  const answered = ((): Observable<DlAppEvent> => {
    if (method === "POST" && segments.length === 2 && segments[0] === "edb" && segments[1] === "file_changed") {
      return served(handleFileChanged(state, request, response));
    }
    if (method === "POST" && segments.length === 2 && segments[0] === "edb") {
      return served(handleEdbInsert(state, segments[1]!, request, response));
    }
    if (method === "GET" && segments.length === 2 && segments[0] === "idb") {
      return served(handleIdbRead(state, segments[1]!, response));
    }
    if (method === "GET" && segments.length === 2 && segments[0] === "subscribe") {
      return sseClient$(state, segments[1]!, request, response, bumpActive);
    }
    if (method === "POST" && segments.length === 1 && segments[0] === "query") {
      return served(handleQuery(state, request, response));
    }
    writeJson(response, 404, { error: "not found", routes: ROUTE_LIST });
    return of({ kind: "served", method, path });
  })();

  return answered.pipe(catchError((failure: unknown) => of(serveFailure(response, method, path, failure))));
}

// ─────────────────────────────────────────────────────────────────────────────
// serveDl: the app. Cold; one subscription (main.ts) runs it.
// ─────────────────────────────────────────────────────────────────────────────

/** The listener as an observable: created and listening on subscribe, closed on
 *  teardown. Replaces the `new Promise` around `server.listen`. */
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

function serverHandle(
  state: ServerState,
  server: http.Server,
  port: number,
  activeSubscribeCount: () => number,
): DlServer {
  return {
    port,
    activeSubscribeCount,
    async close(): Promise<void> {
      await disposeCurrentProgram(state);
      await new Promise<void>((resolve, reject) => {
        server.close((failure) => (failure ? reject(failure) : resolve()));
      });
    },
  };
}

export function serveDl(config: { dbPath: string; port: number }): Observable<DlAppEvent> {
  return defer(() => {
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

        // Read + bridge BEFORE the switch: only an accepted program may replace the
        // running one. A rejected one has already been answered 400 and drops out here.
        const accepted$ = programExchanges$.pipe(
          concatMap((exchange) =>
            from(readProgram(exchange)).pipe(
              catchError((failure: unknown) => {
                serveFailure(exchange.response, "POST", "/edb/program", failure);
                return of(null);
              }),
            ),
          ),
          filter((load): load is ProgramLoad => load !== null),
        );

        return merge(
          accepted$.pipe(switchMap((load) => runProgram$(state, config, load))),
          otherExchanges$.pipe(mergeMap((exchange) => routeRequest(state, exchange, bumpActive))),
          of<DlAppEvent>({ kind: "listening", server: serverHandle(state, server, port, () => activeSubscriptions) }),
        );
      }),
    );
  });
}

// ---- dataflow proof (src/0_types.ts) -----------------------------------------
export type ServeDlHolds = AssertTrue<typeof serveDl extends ServeDl ? true : false>;
