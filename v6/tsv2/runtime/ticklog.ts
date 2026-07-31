/**
 * Format one tick's `ITickDeltas` into the shared log envelope
 * log envelope (plan header item 9, mirrored by
 * v6/prolog/conformance/ticklog.pl on the oracle side):
 *
 *   {"tick":N,"deltas":{"relName":{"add":[[..],...],"del":[[..],...]}}}
 *
 * Rel names ascending; only rels with a nonempty add or del; rows are JSON
 * arrays of column values in declared order. Integers are JSON numbers; a
 * `json`-typed column is canonicalized as JSON with sorted object keys and no
 * whitespace; everything else is a JSON string. Add/del are sorted
 * lexicographically by their own JSON text; no spaces, no trailing newline
 * (the caller adds LF).
 *
 * JSON encoding follows the declared column type, including scalar JSON
 * values.
 */

import type {
  IRelDelta,
  IRow,
  IRowColumnType,
  IRowValue,
  ITickDeltas,
  ITickLogEmitter,
  ITickLogLine,
} from "./types.ts";

function canonicalizeJson(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(canonicalizeJson);
  if (value !== null && typeof value === "object") {
    const record = value as Record<string, unknown>;
    return Object.fromEntries(Object.keys(record).sort().map((key) => [key, canonicalizeJson(record[key])]));
  }
  return value;
}

/** The stored text of a `json` column, canonicalized. Every json VALUE is
 *  accepted, not only objects and arrays: RFC 8259 §2 makes a bare scalar a
 *  complete document and json1's `json_valid` agrees. A column declared
 *  `json` whose stored text somehow does not parse falls back to the string
 *  rendering rather than throwing -- the arrival gate's
 *  `CHECK (json_valid(...))` is what makes that unreachable, and a log line is
 *  the wrong place to discover it. */
function canonicalJsonText(value: string): string | null {
  try {
    const parsed: unknown = JSON.parse(value);
    return JSON.stringify(canonicalizeJson(parsed));
  } catch {
    return null;
  }
}

function encodeValue(value: IRowValue, type?: IRowColumnType): string {
  if (typeof value === "number") {
    if (!Number.isFinite(value)) throw new Error(`non-finite float at tick boundary: ${String(value)}`);
    return JSON.stringify(value);
  }
  if (typeof value === "boolean") return value ? "true" : "false";
  // `ref` joins `json` here, and the measurement that put it there is worth
  // keeping: a ref column's boundary value is the dictionary's memoized
  // `__rendered` text, and that text is NOT key-sorted -- it comes out in
  // DECLARED column order (`{"start":3,"end":9}` for a `span(start, end)`).
  // The old first-character sniff happened to catch it and re-sorted it on
  // the way out, so this encoder has been silently repairing the intern-time
  // rendering all along. Nineteen fixtures went red the moment `ref` was left
  // out. The struct-as-rows header's claim that intern-time text is already
  // canonical is therefore false; that is a named card, and until it moves,
  // canonicalizing here is what the byte contract actually rests on.
  if (type === "json" || type === "ref") return canonicalJsonText(value) ?? JSON.stringify(value);
  return JSON.stringify(value);
}

function encodeRow(row: IRow, types: readonly IRowColumnType[] | undefined): string {
  return `[${row.map((value, index) => encodeValue(value, types?.[index])).join(",")}]`;
}

function encodeRel(delta: IRelDelta, types: readonly IRowColumnType[] | undefined): string {
  const add = delta.add.map((row) => encodeRow(row, types)).sort();
  const del = delta.del.map((row) => encodeRow(row, types)).sort();
  return `${JSON.stringify(delta.rel)}:{"add":[${add.join(",")}],"del":[${del.join(",")}]}`;
}

function byRelNameAscending(left: IRelDelta, right: IRelDelta): number {
  if (left.rel < right.rel) return -1;
  if (left.rel > right.rel) return 1;
  return 0;
}

export const TickLogEmitter: ITickLogEmitter = {
  valueText: encodeValue,

  line(
    tick: number,
    deltas: ITickDeltas,
    relColumnTypes?: Readonly<Record<string, readonly IRowColumnType[]>>,
  ): ITickLogLine {
    const nonEmpty = deltas.rels.filter((delta) => delta.add.length > 0 || delta.del.length > 0);
    const relsText = [...nonEmpty]
      .sort(byRelNameAscending)
      .map((delta) => encodeRel(delta, relColumnTypes?.[delta.rel]))
      .join(",");
    return `{"tick":${tick},"deltas":{${relsText}}}`;
  },
};
