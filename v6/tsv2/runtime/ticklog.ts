/**
 * ticklog.ts — formats one tick's `ITickDeltas` into the shared oracle/tsv2
 * log envelope (plan header item 9, mirrored by
 * v6/prolog/conformance/ticklog.pl on the oracle side):
 *
 *   {"tick":N,"deltas":{"relName":{"add":[[..],...],"del":[[..],...]}}}
 *
 * Rel names ascending; only rels with a nonempty add or del; rows are JSON
 * arrays of column values in declared order. Integers are JSON numbers;
 * JSON object/array text crossing the SQLite seam is canonicalized as JSON
 * with sorted object keys and no whitespace; everything else is a JSON
 * string. Add/del are sorted lexicographically by their own JSON text; no
 * spaces, no trailing newline (the caller adds LF).
 */

import type { IRelDelta, IRow, IRowValue, ITickDeltas, ITickLogEmitter, ITickLogLine } from "./types.ts";

function canonicalizeJson(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(canonicalizeJson);
  if (value !== null && typeof value === "object") {
    const record = value as Record<string, unknown>;
    return Object.fromEntries(Object.keys(record).sort().map((key) => [key, canonicalizeJson(record[key])]));
  }
  return value;
}

function canonicalJsonText(value: string): string | null {
  if (value[0] !== "{" && value[0] !== "[") return null;
  try {
    const parsed: unknown = JSON.parse(value);
    if (parsed === null || typeof parsed !== "object") return null;
    return JSON.stringify(canonicalizeJson(parsed));
  } catch {
    return null;
  }
}

function encodeValue(value: IRowValue): string {
  if (typeof value === "number") return String(value);
  return canonicalJsonText(value) ?? JSON.stringify(value);
}

function encodeRow(row: IRow): string {
  return `[${row.map(encodeValue).join(",")}]`;
}

function encodeRel(delta: IRelDelta): string {
  const add = delta.add.map(encodeRow).sort();
  const del = delta.del.map(encodeRow).sort();
  return `${JSON.stringify(delta.rel)}:{"add":[${add.join(",")}],"del":[${del.join(",")}]}`;
}

function byRelNameAscending(left: IRelDelta, right: IRelDelta): number {
  if (left.rel < right.rel) return -1;
  if (left.rel > right.rel) return 1;
  return 0;
}

export const TickLogEmitter: ITickLogEmitter = {
  line(tick: number, deltas: ITickDeltas): ITickLogLine {
    const nonEmpty = deltas.rels.filter((delta) => delta.add.length > 0 || delta.del.length > 0);
    const relsText = [...nonEmpty].sort(byRelNameAscending).map(encodeRel).join(",");
    return `{"tick":${tick},"deltas":{${relsText}}}`;
  },
};
