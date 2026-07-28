/**
 * 4_ingest.ts — extract subprocess stdout -> spine EDB rels -> per-file re-ingest DIFF.
 *
 * Contract (plan M3, tasks.d.ts): `extractFile(path)` spawns DL_EXTRACT_BIN and yields
 * ExtractRecords; `toFactLines(recs, path)` is the pure-ish F2 mapping (record=node|edge|
 * sig|site|const -> spine rel rows, plus computed span_line(path, start, line, col)
 * rows from file bytes, plus one file(path, content_hash) row); `ingestFile` diffs the
 * new per-path row set against the current tables and rides ONE rt.commit (retraction is
 * the diff, no diag-specific code). extract is CONSUME-ONLY: never touch its crate or
 * worktree.
 *
 * Spine rel schemas (ORCHESTRATOR PIN, tasks.d.ts): file(path, content_hash); content_hash
 * is the sha-256 hex digest of the file's raw bytes (M8-alpha, IdentityWitnessLaw,
 * tasks.d.ts: the witness half of identity-vs-witness). A file row retracts+inserts on
 * ANY content edit, even a same-mtime rewrite, so a downstream salted probe (e.g. sg?
 * joined against content_hash) re-fires instead of reusing a stale cached response);
 * node(path, family, start, end, kind, name); edge(path, family, kind, from_start,
 * from_end, to_start, to_end); sig(path, owner_start, owner_end, slot, pos, ty);
 * site(path, start, end, callee, callee_path); const(path, owner_start, owner_end, field,
 * text, kind); span_line(path, start, line, col) — one row per DISTINCT span offset
 * (start AND end) seen in any node/site record of the file, 0-based line/col, columns
 * counted in BYTES (not UTF-16 code units), computed by scanning the raw file buffer for
 * the ingested file exactly once per ingestFile() call (content_hash reuses that SAME
 * read: one IO, no second pass over the file).
 *
 * `spineDeclsLocal` below is THIS PACKAGE's own decl builder, used only by its tests
 * (fakeBridgeOk needs a Program built from RelDecl[]). Integration wiring point: package
 * M5 (src/5_diag.ts, owned by a parallel arc) exports `spineDecls`/`builtinDecls` built
 * the same way (edbRel(name, columns) per the schema above); once that lands, the http
 * layer (6_http.ts) should boot DlRuntime against THAT export, not this local one, so
 * there is a single source of truth for the spine schema across packages.
 */
import { spawn } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import readline from "node:readline";

import { differenceWith, isEqual } from "lodash-es";

import { edbRel, type RelDecl } from "sprefa-store-engine/src/lower/ast.ts";
import type { FactLine } from "sprefa-store-engine/src/engine/ingest.ts";

import { PerfTrace } from "./0_trace.ts";
import type {
  AssertTrue,
  ExtractRecord,
  IDlRuntime,
  IngestFile,
  Row,
  TickReport,
  ToFactLines,
  Value,
} from "./0_types.ts";
import type { ExtractBinDefault, SpineRelName } from "../tasks.d.ts";

// ─────────────────────────────────────────────────────────────────────────────
// The spine rel schema: one source of truth for both spineDeclsLocal (RelDecl[], for
// this package's own test fixtures) and the row-column mapping toFactLines/ingestFile
// use to turn a positional FactLine into a named Row.
// ─────────────────────────────────────────────────────────────────────────────

const SPINE_REL_SCHEMA: readonly { readonly name: SpineRelName; readonly columns: readonly string[] }[] = [
  { name: "file", columns: ["path", "content_hash"] },
  { name: "node", columns: ["path", "family", "start", "end", "kind", "name"] },
  { name: "edge", columns: ["path", "family", "kind", "from_start", "from_end", "to_start", "to_end"] },
  { name: "sig", columns: ["path", "owner_start", "owner_end", "slot", "pos", "ty"] },
  { name: "site", columns: ["path", "start", "end", "callee", "callee_path"] },
  { name: "const", columns: ["path", "owner_start", "owner_end", "field", "text", "kind"] },
  { name: "span_line", columns: ["path", "start", "line", "col"] },
];

const SPINE_COLUMNS_BY_NAME: ReadonlyMap<string, readonly string[]> = new Map(
  SPINE_REL_SCHEMA.map((entry) => [entry.name, entry.columns] as const),
);

/** The server's working directory is the root used by the stored path surface. */
export const DL_ROOT = process.cwd();

function rootRelativePath(filePath: string): string {
  return path.relative(DL_ROOT, path.resolve(DL_ROOT, filePath));
}

/** This package's own spineDecls builder (see file header: integration replaces this
 *  with 5_diag.ts's `spineDecls`/`builtinDecls` export). */
export const spineDeclsLocal: readonly RelDecl[] = SPINE_REL_SCHEMA.map((entry) => edbRel(entry.name, entry.columns));

// ─────────────────────────────────────────────────────────────────────────────
// extractFile: spawn DL_EXTRACT_BIN, JSONL over stdout -> ExtractRecord per line.
// ─────────────────────────────────────────────────────────────────────────────

const DEFAULT_EXTRACT_BIN: ExtractBinDefault =
  "/Users/chrishafley/projects/sprefa/.claude/worktrees/extract-golden-plan/v6/sprefa-extract/target/debug/extract";

function extractBinPath(): string {
  return process.env.DL_EXTRACT_BIN ?? DEFAULT_EXTRACT_BIN;
}

export async function* extractFile(filePath: string): AsyncIterable<ExtractRecord> {
  const child = spawn(extractBinPath(), [filePath]);
  child.stdin.end();

  let stderrText = "";
  child.stderr.on("data", (chunk: Buffer) => {
    stderrText += chunk.toString("utf8");
  });

  const exitCode = new Promise<number>((resolve, reject) => {
    child.on("error", reject);
    child.on("close", (code) => resolve(code ?? 0));
  });

  const readlineInterface = readline.createInterface({ input: child.stdout, crlfDelay: Infinity });
  for await (const line of readlineInterface) {
    if (line.trim().length === 0) continue;
    yield JSON.parse(line) as ExtractRecord;
  }

  const code = await exitCode;
  if (code !== 0) {
    throw new Error(`extract exited ${code} for '${filePath}': ${stderrText.trim()}`);
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// Byte-offset -> {line, col} index, scanned once over the raw file buffer. Both 0-based;
// col is a BYTE count (extract's spans are byte offsets, not UTF-16 code units).
// ─────────────────────────────────────────────────────────────────────────────

function buildLineStarts(buffer: Buffer): readonly number[] {
  const starts = [0];
  for (let byteIndex = 0; byteIndex < buffer.length; byteIndex++) {
    if (buffer[byteIndex] === 0x0a) starts.push(byteIndex + 1);
  }
  return starts;
}

/** Largest lineStarts[i] <= offset, via binary search (lineStarts is sorted ascending). */
function offsetToLineCol(lineStarts: readonly number[], offset: number): { readonly line: number; readonly col: number } {
  let low = 0;
  let high = lineStarts.length - 1;
  while (low < high) {
    const middle = Math.ceil((low + high) / 2);
    if (lineStarts[middle]! <= offset) low = middle;
    else high = middle - 1;
  }
  return { line: low, col: offset - lineStarts[low]! };
}

// ─────────────────────────────────────────────────────────────────────────────
// toFactLines: the F2 mapping. Pure over its inputs in the sense that matters here (same
// records + same on-disk file bytes -> same output, every time); it does one synchronous
// read of `filePath` to build the span_line byte-offset index (the offsets are ONLY
// knowable from the file bytes, per the plan's "computed from the FILE BYTES" pin) AND
// to hash the file's content (M8-alpha, IdentityWitnessLaw): one read, both derived
// from the SAME bytes, no second IO.
// ─────────────────────────────────────────────────────────────────────────────

function relLine(name: string, row: readonly unknown[]): FactLine {
  return { t: "rel", name, row };
}

/** sha-256 hex digest of a file's raw bytes: the `file` spine row's witness column
 *  (content_hash). Any content edit flips this even when mtime/size coincidentally
 *  match, so a downstream salted probe joined against it re-fires on that edit. */
function contentHashOf(buffer: Buffer): string {
  return crypto.createHash("sha256").update(buffer).digest("hex");
}

export function toFactLines(records: readonly ExtractRecord[], filePath: string): readonly FactLine[] {
  const absPath = path.resolve(DL_ROOT, filePath);
  const relPath = rootRelativePath(absPath);
  const buffer = fs.readFileSync(absPath);
  const contentHash = contentHashOf(buffer);
  const lineStarts = buildLineStarts(buffer);
  const offsets = new Set<number>();
  const recordLines: FactLine[] = [];

  for (const record of records) {
    switch (record.record) {
      case "node":
        recordLines.push(
          relLine("node", [relPath, record.family, record.span.start, record.span.end, record.kind, record.name]),
        );
        offsets.add(record.span.start);
        offsets.add(record.span.end);
        break;
      case "edge":
        recordLines.push(
          relLine("edge", [
            relPath,
            record.family,
            record.kind,
            record.from.start,
            record.from.end,
            record.to.start,
            record.to.end,
          ]),
        );
        break;
      case "sig":
        recordLines.push(relLine("sig", [relPath, record.owner.start, record.owner.end, record.slot, record.pos, record.ty]));
        break;
      case "site":
        recordLines.push(
          relLine("site", [relPath, record.span.start, record.span.end, record.callee, record.callee_path]),
        );
        offsets.add(record.span.start);
        offsets.add(record.span.end);
        break;
      case "const":
        recordLines.push(relLine("const", [relPath, record.owner.start, record.owner.end, record.field, record.text, record.kind]));
        break;
    }
  }

  const spanLineLines: FactLine[] = [...offsets]
    .sort((aOffset, bOffset) => aOffset - bOffset)
    .map((offset) => {
      const { line, col } = offsetToLineCol(lineStarts, offset);
      return relLine("span_line", [relPath, offset, line, col]);
    });

  return [relLine("file", [relPath, contentHash]), ...recordLines, ...spanLineLines];
}

// ─────────────────────────────────────────────────────────────────────────────
// ingestFile: drain extractFile -> toFactLines -> group into named Rows per spine rel ->
// diff against the current per-path rows -> ONE rt.commit. N+1 law: each spine rel's
// current rows are read exactly once (rt.rows(relName)), never per row.
// ─────────────────────────────────────────────────────────────────────────────

function namedRowFromFactLine(factLine: Extract<FactLine, { t: "rel" }>): Row {
  const columns = SPINE_COLUMNS_BY_NAME.get(factLine.name);
  if (!columns) throw new Error(`toFactLines: unknown spine rel '${factLine.name}'`);
  const row: Record<string, Value> = {};
  columns.forEach((column, index) => {
    row[column] = (factLine.row[index] ?? null) as Value;
  });
  return row as Row;
}

function groupFactLinesByRel(factLines: readonly FactLine[]): ReadonlyMap<string, readonly Row[]> {
  const grouped = new Map<string, Row[]>();
  for (const factLine of factLines) {
    if (factLine.t !== "rel") continue; // toFactLines only ever emits the named-rel shape.
    const rows = grouped.get(factLine.name) ?? [];
    rows.push(namedRowFromFactLine(factLine));
    grouped.set(factLine.name, rows);
  }
  return grouped;
}

/** true if `filePath` exists on disk (of any kind); false only for ENOENT. Any other
 *  stat failure (e.g. a permission error) is a real error and is rethrown — only ENOENT
 *  means "this is the delete-file case." */
function fileExistsOnDisk(filePath: string): boolean {
  try {
    fs.statSync(filePath);
    return true;
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code === "ENOENT") return false;
    throw err;
  }
}

export async function ingestFile(rt: IDlRuntime, filePath: string): Promise<TickReport> {
  let newRowsByRel: ReadonlyMap<string, readonly Row[]> = new Map();
  const absPath = path.resolve(DL_ROOT, filePath);
  const relPath = rootRelativePath(absPath);

  // Perf trace (0_trace.ts, seam 3): "extract wall" per the header, deliberately
  // scoped to extractFile + toFactLines only, NOT the commit below.
  const extractStartedAt = performance.now();
  let factLineCount = 0;

  if (fileExistsOnDisk(absPath)) {
    const records: ExtractRecord[] = [];
    for await (const record of extractFile(absPath)) records.push(record);
    const factLines = toFactLines(records, absPath);
    factLineCount = factLines.length;
    newRowsByRel = groupFactLinesByRel(factLines);
  }
  // else: DELETE-FILE case (task 3.3) — newRowsByRel stays empty, so every current row
  // scoped to this path retracts below (empty newSet, non-empty oldSet -> full retract).
  const extractMs = performance.now() - extractStartedAt;

  const insert = new Map<string, readonly Row[]>();
  const retract = new Map<string, readonly Row[]>();
  let diffSize = 0;

  for (const { name: relName } of SPINE_REL_SCHEMA) {
    const newRows = (newRowsByRel.get(relName) ?? []) as Row[];
    const oldRows = (await rt.rows(relName)).filter((row) => row.path === relPath);

    const toInsert = differenceWith(newRows, oldRows, isEqual);
    const toRetract = differenceWith(oldRows, newRows, isEqual);
    if (toInsert.length > 0) insert.set(relName, toInsert);
    if (toRetract.length > 0) retract.set(relName, toRetract);
    diffSize += toInsert.length + toRetract.length;
  }

  const report = await rt.commit({ insert, retract });
  // No further `await` between commit() resolving and this publish, per 0_trace.ts's
  // FLUSH TIMING note -- see 1_hosts.ts's runEffectOnce for the same rule applied to
  // the effect seam.
  PerfTrace.ingestDone(report.tick, relPath, extractMs, factLineCount, diffSize);
  return report;
}

// ---- dataflow proof (src/0_types.ts) -----------------------------------------
export type IngestFileHolds = AssertTrue<typeof ingestFile extends IngestFile ? true : false>;
export type ToFactLinesHolds = AssertTrue<typeof toFactLines extends ToFactLines ? true : false>;
