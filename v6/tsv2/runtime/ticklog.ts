/**
 * ticklog.ts — formats one tick's `ITickDeltas` into the shared oracle/tsv2
 * log envelope (plan header item 9, mirrored by
 * v6/prolog/conformance/ticklog.pl on the oracle side):
 *
 *   {"tick":N,"deltas":{"relName":{"add":[[..],...],"del":[[..],...]}}}
 *
 * Rel names ascending; only rels with a nonempty add or del; rows are JSON
 * arrays of column values in declared order, integers as JSON numbers,
 * everything else as a JSON string; add/del each sorted lexicographically by
 * their own JSON text; no spaces, no trailing newline (the caller adds LF).
 */

import type { IRelDelta, IRow, IRowValue, ITickDeltas, ITickLogEmitter, ITickLogLine } from "./types.ts";

function encodeValue(value: IRowValue): string {
  return typeof value === "number" ? String(value) : JSON.stringify(value);
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
