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
 * NOT A LEAF-HELPER EXEMPTION, and this header used to claim it was. The old
 * text read "small pure leaf transform (no per-instance state): stays a bare
 * function, the same exemption the rxjs law gives `.map` callbacks". Review
 * finding 7 disagreed and is right: purity and statelessness are not the test.
 * The test is whether the function carries a contract worth checking, and this
 * one is THE boundary-diff algorithm, imported by name from 137 emitted
 * modules. It is now annotated against `IMultisetDiff` in the header, which is
 * what buys the compiler check. The exported NAME is unchanged, because the
 * emitted imports are written by the prolog emitter and moving them is another
 * lane's change.
 *
 * `RowDiff` moved to the header too, as `IRowDiff`. It had no reader outside
 * this file, so nothing depended on the old name.
 */

import type { IMultisetDiff, IRow, IRowDiff } from "./types.ts";

function rowKey(row: IRow): string {
  return JSON.stringify(row);
}

/** `prevRows` / `nextRows` are the FULL row lists (duplicates allowed) for
 *  one rel, read before and after one tick's writes. */
export const multisetDiff: IMultisetDiff = (prevRows: readonly IRow[], nextRows: readonly IRow[]): IRowDiff => {
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
};
