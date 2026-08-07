/**
 * engineBetweenSubscribers.test.ts — an engine with no ticks$ reader is still
 * an engine.
 *
 * THE DEFECT. `LiveEngine.ticks$` ended in a bare `share()`, whose default
 * `resetOnRefCountZero: true` unsubscribes the concatMap lane the moment the
 * last reader drops. `tap({ finalize })` then flipped `running` false, so the
 * next `submit` was refused with "tsv2 engine is not running: nothing
 * subscribes ticks$" -- an arrival lost because of who happened to be WATCHING,
 * not because of anything the engine could not do. The reader count is a
 * display concern; the tick loop is the engine's own. Ruling receipt:
 * prolog/conformance/rulings.pl subscribed_reset_pole ("Decoupling running from
 * the ticks$ refcount is a defect fix independent of this pole").
 *
 * Engine level, no http, because the claim is about `ticks$` itself: over http
 * the server's own `run_program$` subscription never drops while a program is
 * loaded, so the http leg cannot see this.
 *
 * RED FIRST, verbatim, before the fix (`node --test
 * --experimental-transform-types tests/engineBetweenSubscribers.test.ts`):
 *
 *   ✖ a submit with no ticks$ reader still ticks, and a late reader sees the state it left (5.63725ms)
 *     Error: tsv2 engine is not running: nothing subscribes ticks$
 *         at Observable._subscribe (v6/tsv2/serve/3_engine.ts:119:26)
 *   ✖ tick numbering does not restart when the readers leave and come back (7.267833ms)
 *     Error: tsv2 engine is not running: nothing subscribes ticks$
 *         at Observable._subscribe (v6/tsv2/serve/3_engine.ts:119:26)
 *   ℹ pass 0
 *   ℹ fail 2
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import { firstValueFrom, toArray } from "rxjs";

import { program as switch_program } from "../gen_emitted/switch_as_keyed_replace.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import type { IArrivalBatch, IServedProgram } from "../runtime/types.ts";

import { LiveEngine, boot_served_program } from "../serve/3_engine.ts";

function route_change(route_id: string): IArrivalBatch {
  return [{ rel: "route_change", sign: "add", row: ["session_one", route_id] }];
}

test("a submit with no ticks$ reader still ticks, and a late reader sees the state it left", async () => {
  const seam = ScratchStore.open(":memory:");
  const engine = new LiveEngine(switch_program as unknown as IServedProgram, seam);
  try {
    await firstValueFrom(boot_served_program(seam, switch_program as unknown as IServedProgram));

    const early = engine.ticks$.subscribe();
    early.unsubscribe();

    const unwatched = await firstValueFrom(engine.submit(route_change("settings")).pipe(toArray()));
    assert.ok(unwatched.length > 0, "the submitter is owed its own ticks with nobody watching ticks$");

    // The tick really ran against the seam, not just against the submitter.
    assert.deepEqual(await firstValueFrom(engine.rows("open_scope")), [["session_one", "route_data(settings)"]]);

    const late_ticks: number[] = [];
    const late = engine.ticks$.subscribe((outcome) => late_ticks.push(outcome.tick));
    try {
      await firstValueFrom(engine.submit(route_change("profile")).pipe(toArray()));
      assert.ok(late_ticks.length > 0, "a reader that arrives after the gap still sees later ticks");
      assert.deepEqual(await firstValueFrom(engine.rows("open_scope")), [["session_one", "route_data(profile)"]]);
      // Both route_changes are in the log: the unwatched one was not replayed
      // into the late reader's view of the world either.
      assert.deepEqual(await firstValueFrom(engine.rows("route_change")), [
        ["session_one", "settings"],
        ["session_one", "profile"],
      ]);
    } finally {
      late.unsubscribe();
    }
  } finally {
    seam.db.close();
  }
});

test("tick numbering does not restart when the readers leave and come back", async () => {
  const seam = ScratchStore.open(":memory:");
  const engine = new LiveEngine(switch_program as unknown as IServedProgram, seam);
  try {
    await firstValueFrom(boot_served_program(seam, switch_program as unknown as IServedProgram));

    const first = engine.ticks$.subscribe();
    const watched = await firstValueFrom(engine.submit(route_change("settings")).pipe(toArray()));
    first.unsubscribe();

    const unwatched = await firstValueFrom(engine.submit(route_change("profile")).pipe(toArray()));

    const late_ticks: number[] = [];
    const late = engine.ticks$.subscribe((outcome) => late_ticks.push(outcome.tick));
    try {
      const rewatched = await firstValueFrom(engine.submit(route_change("settings")).pipe(toArray()));
      const numbers = [...watched, ...unwatched, ...rewatched].map((outcome) => outcome.tick);
      // The oracle grades the tick log by number; a gap or a restart across a
      // reader change would break that comparison.
      assert.deepEqual(numbers, [...Array(numbers.length).keys()].map((index) => index + 1));
      assert.deepEqual(late_ticks, rewatched.map((outcome) => outcome.tick));
    } finally {
      late.unsubscribe();
    }
  } finally {
    seam.db.close();
  }
});
