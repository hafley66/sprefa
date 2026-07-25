/**
 * sequence.ts — run observables one after another and collect what they produced.
 *
 * `concat` and `concatMap` already sequence; what these add is the single terminal
 * emission, so a following `concatMap` fires once rather than once per step. That is
 * `toArray`, not `count`: the results stay on the value channel. Erasing them to `void`
 * is what turns a pipeline into a task runner, and it costs real work downstream, since
 * a caller that threw away `rowsAffected` has to go re-read it with another query.
 */

import { Observable, concat, concatMap, from, of, toArray } from "rxjs";

/** Run every step in order; emit once, with all their results in order. */
export function inSequence<Result>(steps: readonly Observable<Result>[]): Observable<Result[]> {
  return steps.length === 0 ? of([]) : concat(...steps).pipe(toArray());
}

/** Map each item to a step, run the steps in order; emit once, with all their results. */
export function forEachInSequence<Item, Result>(
  items: readonly Item[],
  step: (item: Item) => Observable<Result>,
): Observable<Result[]> {
  return from(items).pipe(concatMap(step), toArray());
}
