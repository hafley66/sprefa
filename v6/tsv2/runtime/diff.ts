/**
 * diff.ts — the multiset row diff every gen/*.ts tick uses to turn a
 * before/after table snapshot into boundary add/del deltas.
 *
 * One algorithm covers both rel kinds in engine.pl's r7 boundary rule: Set
 * and level rels never hold duplicate rows, so a multiset diff degenerates
 * to an ordinary set diff; Log rels are append-only (arrivals only ever add
 * a stamped row), so "count in next minus count in prev" for a given row
 * value is exactly "new stamps this tick" without any separate stamp
 * column — see gen/switch_as_keyed_replace.ts's route_change handling.
 *
 * Small pure leaf transform (no per-instance state): stays a bare function,
 * the same exemption the rxjs law gives `.map` callbacks.
 */

import type { IRow } from "./types.ts";

function rowKey(row: IRow): string {
  return JSON.stringify(row);
}

// A plain data shape returned by one pure function, not implemented by a
// class — a `type` alias, not an `interface`, so it sits outside the
// header-declaration / I-prefix law (0_types.ts's `Value`/`Row` precedent).
export type RowDiff = {
  readonly add: readonly IRow[];
  readonly del: readonly IRow[];
};

/** `prevRows` / `nextRows` are the FULL row lists (duplicates allowed) for
 *  one rel, read before and after one tick's writes. */
export function multisetDiff(prevRows: readonly IRow[], nextRows: readonly IRow[]): RowDiff {
  const prevCounts = new Map<string, { row: IRow; count: number }>();
  for (const row of prevRows) {
    const key = rowKey(row);
    const existing = prevCounts.get(key);
    if (existing) existing.count += 1;
    else prevCounts.set(key, { row, count: 1 });
  }
  const nextCounts = new Map<string, { row: IRow; count: number }>();
  for (const row of nextRows) {
    const key = rowKey(row);
    const existing = nextCounts.get(key);
    if (existing) existing.count += 1;
    else nextCounts.set(key, { row, count: 1 });
  }

  const add: IRow[] = [];
  for (const [key, entry] of nextCounts) {
    const before = prevCounts.get(key)?.count ?? 0;
    for (let extra = before; extra < entry.count; extra += 1) add.push(entry.row);
  }
  const del: IRow[] = [];
  for (const [key, entry] of prevCounts) {
    const after = nextCounts.get(key)?.count ?? 0;
    for (let extra = after; extra < entry.count; extra += 1) del.push(entry.row);
  }
  return { add, del };
}
