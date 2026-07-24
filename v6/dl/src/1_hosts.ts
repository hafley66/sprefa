/**
 * 1_hosts.ts — HostDef registry: sh executor, builtin sg (ast-grep), builtin extract,
 * and HostRunner (the `?` probe machinery: demand rows -> digest-cached effect ->
 * response rows land as one commit per effect).
 *
 * Contract (plan M4, tasks.d.ts): `HostDef{name, requestCols, responseCols, run}`;
 * `shHost(decl)` fills the backtick template ({col} raw / $col env) and spawns;
 * `builtinSg` = `sg run --pattern <p> --json <path>` (bin from node_modules/.bin,
 * devDep @ast-grep/cli); `builtinExtract` = DL_EXTRACT_BIN (task 4.4: exposing the
 * extraction machinery as a demand-driven host, not a second ingest path). `HostRunner`
 * subscribes deltas$ for __req_* inserts, digest-caches via effect_cache (the `?`
 * idempotence law), commits __resp_* rows in one batch. One in-flight run per digest
 * (the cache row IS the lock); errors land as cache state 'error'; the stream never
 * dies.
 *
 * NUMBERING LAW (why this file looks the way it does): 1_hosts.ts imports ONLY
 * 0_types.ts types + 0_digest.ts (shared fold) + sprefa-store-engine (store
 * ast/engine helpers) + rxjs + node stdlib + the two owner-pinned law types
 * (ExtractBinDefault) straight from ../tasks.d.ts (M7 recomposition: that type
 * stays declared in the plan ledger, not 0_types.ts, by owner ruling) -- never
 * src/2_schema.ts or src/3_runtime.ts (both are numbered ABOVE this file; importing
 * them would be an upward import). HostRunner's constructor therefore takes the
 * runtime as a STRUCTURAL parameter typed by 0_types.ts's `DlRuntime` interface
 * (deltas$/commit/rows), never the concrete `DlRuntime` class from 3_runtime.ts.
 * The digest fold (effectDigest below) shares 2_schema.ts's rowDigest law via
 * 0_digest.ts's foldRowDigest (M7 consolidation; this file used to duplicate the
 * fold in miniature, documented rather than imported -- now imported instead).
 */

import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

import { EMPTY, Subscription, concatMap, filter, from, mergeMap, type Observable } from "rxjs";

import { foldRowDigest } from "./0_digest.ts";
import type { CacheDb, DlRuntime, EdbBatch, HostDecl, HostDef, Row, Value } from "./0_types.ts";
import type { ExtractBinDefault } from "../tasks.d.ts";

export type { CacheDb, HostDef };

// ─────────────────────────────────────────────────────────────────────────────
// Value plumbing shared by every host below.
// ─────────────────────────────────────────────────────────────────────────────

function normalizeValue(raw: unknown): Value {
  if (raw === undefined || raw === null) return null;
  if (typeof raw === "number" || typeof raw === "string" || typeof raw === "boolean") return raw;
  return String(raw);
}

function valueToShellText(value: Value): string {
  if (value === null || value === undefined) return "";
  if (typeof value === "boolean") return value ? "1" : "0";
  return String(value);
}

/** Same law as 2_schema.ts's rowDigest, shared via 0_digest.ts's foldRowDigest (see
 *  file header). effect_cache digest = mix(host, ...requestTuple) -- the v5
 *  pending_effect law. */
function effectDigest(hostName: string, requestRow: Row, requestCols: readonly string[]): string {
  const narrowed = foldRowDigest(requestRow, requestCols);
  return `${hostName}:${narrowed}`;
}

// ─────────────────────────────────────────────────────────────────────────────
// Output-contract parsing shared by shHost and any future line-oriented host: stdout
// is EITHER a JSON array of objects, OR JSON-lines of objects, OR whitespace-separated
// columns; detected in that order. Map each parsed item to a response Row by
// responseCols order (objects: by key when keys match, else by position of
// Object.values; whitespace: by position with Number() coercion when the target col's
// first seen value parses numeric).
// ─────────────────────────────────────────────────────────────────────────────

function mapItemToRow(item: unknown, responseCols: readonly string[]): Row {
  if (item !== null && typeof item === "object" && !Array.isArray(item)) {
    const record = item as Record<string, unknown>;
    const hasAllKeys = responseCols.every((column) => column in record);
    const values = hasAllKeys ? responseCols.map((column) => record[column]) : Object.values(record);
    const row: Record<string, Value> = {};
    responseCols.forEach((column, index) => {
      row[column] = normalizeValue(values[index] ?? null);
    });
    return row as Row;
  }
  const values = Array.isArray(item) ? item : [item];
  const row: Record<string, Value> = {};
  responseCols.forEach((column, index) => {
    row[column] = normalizeValue(values[index] ?? null);
  });
  return row as Row;
}

function tryParseJsonArray(text: string): unknown[] | null {
  try {
    const parsed: unknown = JSON.parse(text);
    return Array.isArray(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

function tryParseJsonLines(text: string): unknown[] | null {
  const lines = text.split("\n").filter((line) => line.trim().length > 0);
  if (lines.length === 0) return null;
  const parsedLines: unknown[] = [];
  for (const line of lines) {
    try {
      parsedLines.push(JSON.parse(line));
    } catch {
      return null;
    }
  }
  return parsedLines;
}

function parseWhitespaceColumns(text: string, responseCols: readonly string[]): Row[] {
  const splitLines = text
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0)
    .map((line) => line.split(/\s+/));
  const columnLooksNumeric = responseCols.map((_, columnIndex) => {
    const firstValue = splitLines[0]?.[columnIndex];
    return firstValue !== undefined && firstValue !== "" && !Number.isNaN(Number(firstValue));
  });
  return splitLines.map((parts) => {
    const row: Record<string, Value> = {};
    responseCols.forEach((column, columnIndex) => {
      const raw = parts[columnIndex] ?? "";
      row[column] = columnLooksNumeric[columnIndex] ? Number(raw) : raw;
    });
    return row as Row;
  });
}

function parseHostOutput(stdout: string, responseCols: readonly string[]): Row[] {
  const trimmed = stdout.trim();
  if (trimmed.length === 0) return [];
  const asJsonArray = tryParseJsonArray(trimmed);
  if (asJsonArray) return asJsonArray.map((item) => mapItemToRow(item, responseCols));
  const asJsonLines = tryParseJsonLines(trimmed);
  if (asJsonLines) return asJsonLines.map((item) => mapItemToRow(item, responseCols));
  return parseWhitespaceColumns(trimmed, responseCols);
}

// ─────────────────────────────────────────────────────────────────────────────
// shHost: template fill ({col} -> raw splice into the command string; $col -> an
// exported environment variable for the child, left untouched in the template text so
// the child's own shell performs the expansion) + spawn (shell: true; the template IS
// a shell line by contract) + output-contract parse.
// ─────────────────────────────────────────────────────────────────────────────

function binDirectory(): string {
  return fileURLToPath(new URL("../node_modules/.bin", import.meta.url));
}

function fillTemplateSplice(template: string, row: Row, columns: readonly string[]): string {
  let filled = template;
  for (const column of columns) {
    filled = filled.split(`{${column}}`).join(valueToShellText(row[column] ?? null));
  }
  return filled;
}

function envForRow(row: Row, columns: readonly string[]): Record<string, string> {
  const env: Record<string, string> = {};
  for (const column of columns) env[column] = valueToShellText(row[column] ?? null);
  return env;
}

function runShellLine(commandLine: string, envOverrides: Record<string, string>): Promise<string> {
  return new Promise((resolve, reject) => {
    const child = spawn(commandLine, {
      shell: true,
      env: { ...process.env, ...envOverrides, PATH: `${binDirectory()}:${process.env.PATH ?? ""}` },
    });
    let stdout = "";
    let stderr = "";
    child.stdout?.on("data", (chunk: Buffer) => {
      stdout += chunk.toString();
    });
    child.stderr?.on("data", (chunk: Buffer) => {
      stderr += chunk.toString();
    });
    child.on("error", reject);
    child.on("close", (code) => {
      if (code !== 0) reject(new Error(`sh host exited ${code}: ${stderr.trim()}`));
      else resolve(stdout);
    });
  });
}

/** "Response rows INCLUDE the request cols (the join key): merge request values in" is
 *  implemented as a first-class step, not an incidental side effect: the generic
 *  stdout parse only ever targets the OUTPUT-only columns (responseCols minus
 *  inputCols) -- request columns are never re-parsed from a tool's own output, always
 *  taken verbatim from the request row. This is also what keeps the generic parser
 *  honestly generic: a tool whose own JSON nests its fields (ast-grep's `--json` does:
 *  byte offsets live at `range.byteOffset.start/end`, not top-level `start`/`end`) is
 *  expected to reshape its own output inside the template (a `| jq '...'` tail is a
 *  shell line like any other) rather than push tool-specific unwrapping into this
 *  parser -- see the sg-vs-sg_sh parity test for a worked example. */
export function shHost(decl: HostDecl): HostDef {
  const responseCols = decl.columns.map((column) => column.name);
  const outputOnlyCols = responseCols.filter((name) => !decl.inputCols.includes(name));
  return {
    name: decl.name,
    requestCols: decl.inputCols,
    responseCols,
    async *run(request: Row): AsyncIterable<Row> {
      const filledTemplate = fillTemplateSplice(decl.template, request, decl.inputCols);
      const envOverrides = envForRow(request, decl.inputCols);
      const stdout = await runShellLine(filledTemplate, envOverrides);
      for (const parsedRow of parseHostOutput(stdout, outputOnlyCols)) {
        const row: Record<string, Value> = {};
        for (const column of responseCols) {
          row[column] = decl.inputCols.includes(column) ? (request[column] ?? null) : (parsedRow[column] ?? null);
        }
        yield row as Row;
      }
    },
  };
}

// ─────────────────────────────────────────────────────────────────────────────
// builtinSg: ast-grep via node_modules/.bin/sg. Spawn WITHOUT shell, args array -- no
// quoting hazards, unlike the sh-decl path. VERIFIED on this machine (sg 0.39.9, this
// worktree's corpus fixture): `range.byteOffset` {start:147,end:167}, text
// "console.log(message)" for `sg run --pattern 'console.log($$$ARGS)' --json
// fixtures/corpus/bad.ts`.
// ─────────────────────────────────────────────────────────────────────────────

interface SgMatch {
  readonly text: string;
  readonly range: { readonly byteOffset: { readonly start: number; readonly end: number } };
}

function sgBinaryPath(): string {
  return fileURLToPath(new URL("../node_modules/.bin/sg", import.meta.url));
}

function runSgProcess(pattern: string, path: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const child = spawn(sgBinaryPath(), ["run", "--pattern", pattern, "--json", path], { shell: false });
    let stdout = "";
    let stderr = "";
    child.stdout?.on("data", (chunk: Buffer) => {
      stdout += chunk.toString();
    });
    child.stderr?.on("data", (chunk: Buffer) => {
      stderr += chunk.toString();
    });
    child.on("error", reject);
    child.on("close", (code) => {
      if (code !== 0) reject(new Error(`sg exited ${code}: ${stderr.trim()}`));
      else resolve(stdout);
    });
  });
}

export const builtinSg: HostDef = {
  name: "sg",
  requestCols: ["pattern", "path"],
  responseCols: ["pattern", "path", "start", "end", "text"],
  async *run(request: Row): AsyncIterable<Row> {
    const pattern = String(request.pattern ?? "");
    const path = String(request.path ?? "");
    const stdout = await runSgProcess(pattern, path);
    const matches = JSON.parse(stdout) as SgMatch[];
    for (const match of matches) {
      yield {
        pattern,
        path,
        start: match.range.byteOffset.start,
        end: match.range.byteOffset.end,
        text: match.text,
      };
    }
  },
};

// ─────────────────────────────────────────────────────────────────────────────
// builtinExtract: task 4.4, exposure of the extraction machinery as a demand-driven
// host (a program can `extract?(path)` the same binary file_changed's ingest path
// runs) -- NOT a second ingest path. MVP shape: each extract JSONL line lands as one
// response row with the raw line as record_json.
// ─────────────────────────────────────────────────────────────────────────────

const DEFAULT_EXTRACT_BIN: ExtractBinDefault =
  "/Users/chrishafley/projects/sprefa/.claude/worktrees/extract-golden-plan/v6/sprefa-extract/target/debug/extract";

function extractBinaryPath(): string {
  return process.env.DL_EXTRACT_BIN ?? DEFAULT_EXTRACT_BIN;
}

function runExtractProcess(path: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const child = spawn(extractBinaryPath(), [path], { shell: false });
    let stdout = "";
    let stderr = "";
    child.stdout?.on("data", (chunk: Buffer) => {
      stdout += chunk.toString();
    });
    child.stderr?.on("data", (chunk: Buffer) => {
      stderr += chunk.toString();
    });
    child.on("error", reject);
    child.on("close", (code) => {
      if (code !== 0) reject(new Error(`extract exited ${code}: ${stderr.trim()}`));
      else resolve(stdout);
    });
  });
}

export const builtinExtract: HostDef = {
  name: "extract",
  requestCols: ["path"],
  responseCols: ["path", "record_json"],
  async *run(request: Row): AsyncIterable<Row> {
    const path = String(request.path ?? "");
    const stdout = await runExtractProcess(path);
    const lines = stdout.split("\n").filter((line) => line.trim().length > 0);
    for (const line of lines) yield { path, record_json: line };
  },
};

// ─────────────────────────────────────────────────────────────────────────────
// HostRunner: subscribes deltas$ for __req_* inserts, digest-caches via effect_cache,
// commits __resp_* rows. cacheDb is a THIRD structural param (pinned resolution, per
// the escalation ruling): HostRunner receives the runtime only for deltas$/commit, and
// a libsql-shaped client for the effect_cache reads/writes it needs on its own -- the
// same db the runtime booted with (tests pass that same client, or a fresh
// createClient() on the same file path; both are documented as acceptable). CacheDb
// itself is declared in 0_types.ts (M7: it is a cross-file contract -- 6_http.ts and
// tests/2_helpers_hosts.ts both import it) and re-exported above.
// ─────────────────────────────────────────────────────────────────────────────

interface PendingEffect {
  readonly host: HostDef;
  readonly requestRow: Row;
  readonly tick: number;
}

export class HostRunner {
  private readonly hostsByRequestRel: ReadonlyMap<string, HostDef>;
  private subscription: Subscription | null = null;

  constructor(
    private readonly runtime: DlRuntime,
    hosts: readonly HostDef[],
    private readonly cacheDb: CacheDb,
  ) {
    const hostsByRequestRel = new Map<string, HostDef>();
    for (const host of hosts) hostsByRequestRel.set(`__req_${host.name}`, host);
    this.hostsByRequestRel = hostsByRequestRel;
  }

  /** Subscribes runtime.deltas$. concatMap is the serialization lock (one effect runs
   *  at a time this arc; the cache row is the dedupe, not a queue data structure). */
  start(): void {
    if (this.subscription) return;
    const requests$: Observable<void> = this.runtime.deltas$.pipe(
      filter((event) => event.rel.startsWith("__req_") && event.inserts.length > 0),
      mergeMap((event) => {
        const host = this.hostsByRequestRel.get(event.rel);
        if (!host) return EMPTY;
        return from(event.inserts.map((insertRow): PendingEffect => ({ host, requestRow: insertRow, tick: event.tick })));
      }),
      concatMap((pending) => from(this.runEffectOnce(pending))),
    );
    this.subscription = requests$.subscribe({
      error: (error: unknown) => {
        // runEffectOnce swallows every per-effect failure internally; reaching this
        // handler means a bug in the pipeline plumbing itself, not an effect run.
        throw error;
      },
    });
  }

  /** Unsubscribes; any run already past its `host.run()` drain settles harmlessly
   *  (its commit()/cache UPDATE simply lands after dispose, same as any in-flight
   *  promise on teardown -- no cancellation machinery this arc). */
  dispose(): void {
    this.subscription?.unsubscribe();
    this.subscription = null;
  }

  private async runEffectOnce(pending: PendingEffect): Promise<void> {
    const digest = effectDigest(pending.host.name, pending.requestRow, pending.host.requestCols);
    try {
      const existing = await this.cacheDb.execute({ sql: "SELECT digest FROM effect_cache WHERE digest = ?", args: [digest] });
      if (existing.rows.length > 0) return; // cache hit: the `?` law -- never re-run, never re-commit.

      await this.cacheDb.execute({
        sql: "INSERT INTO effect_cache(digest,host,state,requested_tick) VALUES (?,?,?,?) ON CONFLICT(digest) DO NOTHING",
        args: [digest, pending.host.name, "pending", pending.tick],
      });

      const responseRows: Row[] = [];
      for await (const responseRow of pending.host.run(pending.requestRow)) responseRows.push(responseRow);

      await this.runtime.commit({
        insert: new Map([[`__resp_${pending.host.name}`, responseRows]]),
        retract: new Map(),
      });
      await this.cacheDb.execute({ sql: "UPDATE effect_cache SET state = ? WHERE digest = ?", args: ["done", digest] });
    } catch {
      // Error DETAIL retrieval is a frontier item (documented, not this arc's scope):
      // the state column's 'error' value is all effect_cache records today. The
      // outer .catch keeps this defensive even if the UPDATE itself fails --
      // the stream must never die.
      await this.cacheDb
        .execute({ sql: "UPDATE effect_cache SET state = ? WHERE digest = ?", args: ["error", digest] })
        .catch(() => {});
    }
  }
}
