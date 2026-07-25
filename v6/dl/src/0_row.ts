/**
 * 0_row.ts — decoding driver rows into `Value`.
 *
 * One copy. The two that existed had drifted: 1_hosts.ts had no `bigint` branch, so a
 * bigint fell through to `String(raw)` and landed in a `Value` as text. libsql opens with
 * `intMode:"bigint"`, so every INTEGER column arrives as one.
 */

import type { IRowCodec, Row, Value } from "./0_types.ts";

export const RowCodec: IRowCodec = {
  normalizeValue(raw: unknown): Value {
    if (raw === undefined || raw === null) return null;
    if (typeof raw === "bigint") return Number(raw);
    if (typeof raw === "number" || typeof raw === "string" || typeof raw === "boolean") return raw;
    return String(raw);
  },

  rowFromRaw(rawRow: unknown, columns: readonly string[]): Row {
    const raw = rawRow as Record<string, unknown>;
    const row: Record<string, Value> = {};
    for (const column of columns) row[column] = RowCodec.normalizeValue(raw[column]);
    return row as Row;
  },
};
