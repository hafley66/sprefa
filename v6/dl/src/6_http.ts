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
 * Server state: ONE mutable slot (ServerState below), empty until a program loads via
 * POST /edb/program. A re-POST disposes the previous runtime + HostRunner + side db
 * connection, then boots fresh — no two-worlds split, no partial-reload machinery.
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

import { filter } from "rxjs";

import { open_db, type SqliteDb } from "sprefa-store-engine/src/engine/lib.ts";

import { bridge } from "./0_ast_bridge.ts";
import { builtinExtract, builtinSg, HostRunner, shHost, type CacheDb, type HostDef } from "./1_hosts.ts";
import { DlRuntime } from "./3_runtime.ts";
import { DL_ROOT, ingestFile } from "./4_ingest.ts";
import { builtinDecls, DIAG_V5_VIEW_SQL } from "./5_diag.ts";
import type { BridgeOk, DeltaEvent, DlServer, LoadDiag, Row, TickReport, Value } from "./0_types.ts";
import type { RelDecl } from "sprefa-store-engine/src/lower/ast.ts";

// DlServer (startServer()'s public return shape) is declared in 0_types.ts (M7: a
// cross-file contract -- tests/6_http.test.ts imports the type) and re-exported here
// so that existing `from "./6_http.ts"` importers keep working unchanged.
export type { DlServer };

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
  hostRunner: HostRunner | null;
  sideDb: SqliteDb | null;
}

function newServerState(): ServerState {
  return { bridgeOk: null, runtime: null, hostRunner: null, sideDb: null };
}

async function disposeCurrentProgram(state: ServerState): Promise<void> {
  if (state.hostRunner) {
    state.hostRunner.dispose();
    state.hostRunner = null;
  }
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
  const raw = rawRow as Record<string, unknown>;
  const row: Record<string, Value> = {};
  for (const column of columns) row[column] = normalizeQueryValue(raw[column]);
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

async function handleProgramLoad(
  state: ServerState,
  cfg: { readonly dbPath: string },
  req: http.IncomingMessage,
  res: http.ServerResponse,
): Promise<void> {
  const dlText = await readBody(req);
  const result = bridge(dlText, builtinDecls);
  if (result.kind === "err") {
    writeJson(res, 400, { diags: result.diags satisfies readonly LoadDiag[] });
    return;
  }

  await disposeCurrentProgram(state);

  const runtime = await DlRuntime.boot({ dbPath: cfg.dbPath, bridge: result, extraDdl: [DIAG_V5_VIEW_SQL] });
  const sideDb = open_db(`file:${cfg.dbPath}`);
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
  hostRunner.start();

  state.bridgeOk = result;
  state.runtime = runtime;
  state.hostRunner = hostRunner;
  state.sideDb = sideDb;

  writeJson(res, 200, {
    loaded: true,
    rels: result.program.rels.map((decl) => decl.name),
    minted: result.minted,
  });
}

async function handleFileChanged(state: ServerState, req: http.IncomingMessage, res: http.ServerResponse): Promise<void> {
  if (!state.runtime) {
    writeJson(res, 409, { error: "no program loaded" });
    return;
  }
  const body = JSON.parse(await readBody(req)) as { path: string };
  const report: TickReport = await ingestFile(state.runtime, body.path);
  writeJson(res, 200, report);
}

async function handleEdbInsert(
  state: ServerState,
  relName: string,
  req: http.IncomingMessage,
  res: http.ServerResponse,
): Promise<void> {
  if (!state.runtime || !state.bridgeOk) {
    writeJson(res, 409, { error: "no program loaded" });
    return;
  }
  if (!relDeclOf(state, relName)) {
    writeJson(res, 400, { error: `unknown rel '${relName}'` });
    return;
  }
  const body = JSON.parse(await readBody(req)) as { rows?: Row[] };
  const report: TickReport = await state.runtime.commit({
    insert: new Map([[relName, normalizePathSurface(body.rows ?? [])]]),
    retract: new Map(),
  });
  writeJson(res, 200, report);
}

async function handleIdbRead(state: ServerState, relName: string, res: http.ServerResponse): Promise<void> {
  if (!state.runtime || !relDeclOf(state, relName)) {
    writeJson(res, 404, { error: `unknown rel '${relName}'` });
    return;
  }
  const rows = await state.runtime.rows(relName);
  writeJson(res, 200, { rows: resolvePathSurface(rows) });
}

function handleSubscribe(
  state: ServerState,
  relName: string,
  req: http.IncomingMessage,
  res: http.ServerResponse,
  bumpActive: (delta: number) => void,
): void {
  if (!state.runtime || !relDeclOf(state, relName)) {
    writeJson(res, 404, { error: `unknown rel '${relName}'` });
    return;
  }

  res.writeHead(200, {
    "content-type": "text/event-stream",
    "cache-control": "no-cache",
    connection: "keep-alive",
  });
  res.flushHeaders();

  bumpActive(1);
  let torn = false;
  const subscription = state.runtime.deltas$.pipe(filter((event: DeltaEvent) => event.rel === relName)).subscribe({
    next: (event) => {
      res.write(`data: ${JSON.stringify(event)}\n\n`);
    },
    complete: () => teardown(),
  });

  function teardown(): void {
    if (torn) return;
    torn = true;
    subscription.unsubscribe();
    bumpActive(-1);
  }

  // Teardown law: a dropped curl (socket close) must unsubscribe -- refCount
  // honesty. req.socket is the one reliable signal across both a graceful client
  // disconnect and an abrupt one (curl -N killed, `res.destroy()` from the client
  // side, etc.).
  req.socket.once("close", teardown);
}

async function handleQuery(state: ServerState, req: http.IncomingMessage, res: http.ServerResponse): Promise<void> {
  if (!state.runtime || !state.bridgeOk || !state.sideDb) {
    writeJson(res, 409, { error: "no program loaded" });
    return;
  }
  const queryText = await readBody(req);
  // A lone `? rel(args).` line is a valid Program on its own (grammar: statements*).
  // The current program's rels are passed as builtinRels so the query resolves
  // without re-declaring anything.
  const result = bridge(queryText, state.bridgeOk.program.rels);
  if (result.kind === "err") {
    writeJson(res, 400, { diags: result.diags });
    return;
  }
  const queryRef = result.queries[0];
  if (!queryRef) {
    writeJson(res, 400, { error: "no query statement found (expected `? rel(args).`)" });
    return;
  }
  const decl = relDeclOf(state, queryRef.rel);
  if (!decl) {
    writeJson(res, 400, { error: `unknown rel '${queryRef.rel}'` });
    return;
  }

  const whereClauses: string[] = [];
  queryRef.args.forEach((arg, index) => {
    const column = decl.columns[index];
    if (column === undefined) return; // trailing elision: unconstrained
    if (arg.kind === "lit") {
      const value = column === "path" && typeof arg.value === "string"
        ? path.relative(DL_ROOT, path.resolve(DL_ROOT, arg.value))
        : arg.value;
      whereClauses.push(`${column} IS ${sqlLiteralForQuery(value)}`);
    }
    // "var"/"wild": unconstrained (a one-shot query has no other body atom to bind
    // a Var against, so a bound-looking var reads exactly like a wildcard here).
  });
  const whereSql = whereClauses.length > 0 ? ` WHERE ${whereClauses.join(" AND ")}` : "";
  const sql = `SELECT ${decl.columns.join(",")} FROM rel_${decl.name}${whereSql}`;

  const sqlResult = await state.sideDb.execute(sql);
  const rows = sqlResult.rows.map((rawRow) => rowFromRawQueryResult(rawRow, decl.columns));
  writeJson(res, 200, { rows: resolvePathSurface(rows) });
}

// ─────────────────────────────────────────────────────────────────────────────
// Router: six literal path shapes, matched by method + split segments.
// ─────────────────────────────────────────────────────────────────────────────

async function routeRequest(
  state: ServerState,
  cfg: { readonly dbPath: string },
  req: http.IncomingMessage,
  res: http.ServerResponse,
  bumpActive: (delta: number) => void,
): Promise<void> {
  const method = req.method ?? "GET";
  const url = new URL(req.url ?? "/", "http://localhost");
  const segments = url.pathname.split("/").filter((segment) => segment.length > 0);

  if (method === "POST" && segments.length === 2 && segments[0] === "edb" && segments[1] === "program") {
    return handleProgramLoad(state, cfg, req, res);
  }
  if (method === "POST" && segments.length === 2 && segments[0] === "edb" && segments[1] === "file_changed") {
    return handleFileChanged(state, req, res);
  }
  if (method === "POST" && segments.length === 2 && segments[0] === "edb") {
    return handleEdbInsert(state, segments[1]!, req, res);
  }
  if (method === "GET" && segments.length === 2 && segments[0] === "idb") {
    return handleIdbRead(state, segments[1]!, res);
  }
  if (method === "GET" && segments.length === 2 && segments[0] === "subscribe") {
    handleSubscribe(state, segments[1]!, req, res, bumpActive);
    return;
  }
  if (method === "POST" && segments.length === 1 && segments[0] === "query") {
    return handleQuery(state, req, res);
  }

  writeJson(res, 404, { error: "not found", routes: ROUTE_LIST });
}

// ─────────────────────────────────────────────────────────────────────────────
// startServer: the public entry point.
// ─────────────────────────────────────────────────────────────────────────────

export async function startServer(cfg: { dbPath: string; port: number }): Promise<DlServer> {
  const state = newServerState();
  let activeSubscriptions = 0;
  const bumpActive = (delta: number): void => {
    activeSubscriptions += delta;
  };

  const server = http.createServer((req, res) => {
    routeRequest(state, cfg, req, res, bumpActive).catch((err: unknown) => {
      if (!res.headersSent) {
        writeJson(res, 500, { error: err instanceof Error ? err.message : String(err) });
      } else {
        res.end();
      }
    });
  });

  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(cfg.port, () => resolve());
  });

  const address = server.address();
  const actualPort = typeof address === "object" && address !== null ? address.port : cfg.port;

  return {
    port: actualPort,
    activeSubscribeCount: () => activeSubscriptions,
    async close(): Promise<void> {
      await disposeCurrentProgram(state);
      await new Promise<void>((resolve, reject) => {
        server.close((err) => (err ? reject(err) : resolve()));
      });
    },
  };
}
