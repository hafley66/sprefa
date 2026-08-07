/** Two set-based statements per non-empty batch, flat in the DISTINCT arriving
 *  value count; `tests/textIntern.test.ts` is the count rail. Runs BEFORE
 *  StructPlane, whose target tables carry text columns inside their own key. */

import { concatMap, defer, map, of, type Observable } from "rxjs";

import type {
  IArrivalBatch,
  IArrivalRow,
  IRow,
  ISqlSeam,
  ITextInternPlan,
  ITextPlane,
} from "./types.ts";

/** A number reaching a text column interns as its rendering, which is what the
 *  TEXT column stored before the flip. */
function collectValues(plan: ITextInternPlan, arrivals: IArrivalBatch): readonly string[] {
  const values = new Set<string>();
  for (const arrival of arrivals) {
    const flags = plan.relColumns[arrival.rel];
    if (flags === undefined) continue;
    arrival.row.forEach((value, index) => {
      if (flags[index] !== true) return;
      if (value === null || value === undefined) {
        throw new Error(`text_intern_null(${arrival.rel}, ${index})`);
      }
      values.add(String(value));
    });
  }
  return [...values];
}

function rewriteRow(row: IRow, flags: readonly boolean[], ids: ReadonlyMap<string, number>): IRow {
  return row.map((value, index) => {
    if (flags[index] !== true) return value;
    const content = String(value);
    const id = ids.get(content);
    if (id === undefined) {
      throw new Error(`text intern lost the id for ${JSON.stringify(content)}`);
    }
    return id;
  });
}

export const TextPlane: ITextPlane = {
  intern(seam: ISqlSeam, plan: ITextInternPlan, arrivals: IArrivalBatch): Observable<IArrivalBatch> {
    // defer, so a malformed row's refusal reaches the tick's error channel
    // rather than the caller's stack frame.
    return defer(() => {
      if (arrivals.length === 0) return of(arrivals);
      const values = collectValues(plan, arrivals);
      if (values.length === 0) return of(arrivals);
      const encoded = JSON.stringify(values);
      return internValues(seam, plan, arrivals, encoded);
    });
  },
};

function internValues(
  seam: ISqlSeam,
  plan: ITextInternPlan,
  arrivals: IArrivalBatch,
  encoded: string,
): Observable<IArrivalBatch> {
  return seam.runner.execute(seam.db, { sql: plan.internSql, args: [encoded] }).pipe(
    concatMap(() => seam.runner.execute(seam.db, { sql: plan.lookupSql, args: [encoded] })),
    map((result) => {
      const ids = new Map<string, number>();
      for (const row of result.rows) ids.set(String(row["__lookup"]), Number(row["__id"]));
      return arrivals.map((arrival): IArrivalRow => {
        const flags = plan.relColumns[arrival.rel];
        if (flags === undefined) return arrival;
        return { rel: arrival.rel, sign: arrival.sign, row: rewriteRow(arrival.row, flags, ids) };
      });
    }),
  );
}
