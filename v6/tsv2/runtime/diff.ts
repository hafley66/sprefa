/**
 * Multiset row diff for generated tick boundaries. It turns a
 * before/after table snapshot into boundary add/del deltas.
 *
 * One algorithm covers both rel kinds in engine.pl's r7 boundary rule: Set
 * and level rels never hold duplicate rows, so a multiset diff degenerates
 * to an ordinary set diff; Log rels are append-only (arrivals only ever add
 * a stamped row), so "count in next minus count in prev" for a given row
 * value is exactly "new stamps this tick" without any separate stamp
 * column — see gen/switch_as_keyed_replace.ts's route_change handling.
 *
 * The exported name and call signature are fixed by emitted imports.
 */

import type { IMultisetDiff, IRow, IRowDiff } from "./types.ts";

function row_key(row: IRow): string {
  return JSON.stringify(row);
}

/** `prevRows` / `nextRows` are the FULL row lists (duplicates allowed) for
 *  one rel, read before and after one tick's writes. */
export const multiset_diff: IMultisetDiff = (prev_rows: readonly IRow[], next_rows: readonly IRow[]): IRowDiff => {
  const prev_counts = new Map<string, { row: IRow; count: number }>();
  for (const row of prev_rows) {
    const key = row_key(row);
    const existing = prev_counts.get(key);
    if (existing) existing.count += 1;
    else prev_counts.set(key, { row, count: 1 });
  }
  const next_counts = new Map<string, { row: IRow; count: number }>();
  for (const row of next_rows) {
    const key = row_key(row);
    const existing = next_counts.get(key);
    if (existing) existing.count += 1;
    else next_counts.set(key, { row, count: 1 });
  }

  const add: IRow[] = [];
  for (const [key, entry] of next_counts) {
    const before = prev_counts.get(key)?.count ?? 0;
    for (let extra = before; extra < entry.count; extra += 1) add.push(entry.row);
  }
  const del: IRow[] = [];
  for (const [key, entry] of prev_counts) {
    const after = next_counts.get(key)?.count ?? 0;
    for (let extra = after; extra < entry.count; extra += 1) del.push(entry.row);
  }
  return { add, del };
};
