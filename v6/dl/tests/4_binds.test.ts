/**
 * tests/4_binds.test.ts -- the bind seam's own gate, mirroring tests/4_hosts.test.ts's
 * shape for the input side: activation-by-rel-name, retention interplay, and the
 * teardown law (unsubscribe stops the timer, no bespoke dispose()). The full
 * boot -> advance -> reload-stops-timer receipt lives in tests/6_binds_http.test.ts
 * (through serveDl, the way the task's own acceptance criteria are worded); this file
 * is the unit-level proof reaching what that end-to-end golden can't isolate quickly.
 *
 * A one-second period (not the demo fixture's two-second period) keeps these tests
 * fast; fixtures/clock-swr-demo.dl's own period is exercised by the http-level test.
 *
 * SCHEDULER SEAM (0_types.ts's BindConfig.scheduler, plans/2026-07-28-cleanup-audit-
 * findings.md F3/F6): the second and third tests below inject rxjs's own
 * `TestScheduler` in place of the production default (`asyncScheduler`) and drive
 * `clockIntervalFor`'s cadence on virtual time via `advanceVirtualSeconds`
 * (tests/2_helpers_binds.ts) -- no real `setTimeout` sleep. The FIRST test stays on
 * the real default scheduler on purpose: its assertion depends on `bucketFor`
 * computing two DIFFERENT bucket values across two firings, and `bucketFor` reads
 * real `Date.now()` regardless of which scheduler drives the interval (1_binds.ts's
 * header: bucket VALUES stay real wall-clock, only the firing CADENCE is injected).
 * Two firings collapsed onto the same virtual flush would land inside the same real
 * wall-clock second and compute the SAME bucket, falsifying "must strictly increase"
 * for a reason that has nothing to do with the bind's correctness.
 *
 * SABOTAGE RECEIPT (F3, mandatory per the dispatch): 1_binds.ts's BindRunner
 * constructor was temporarily rewritten to hold an internal Subject-backed
 * subscription that never unsubscribes from the merged bind sources -- the exact
 * "held internal subscription keeping every interval alive past unsubscribe" shape
 * the audit found:
 *
 *   const bridge = new Subject<BindCommitDone>();
 *   merge(...).subscribe({ next: (value) => bridge.next(value) }); // never torn down
 *   this.commits$ = bridge.asObservable();
 *
 * Result: the THIRD test below (teardown) went RED at its first post-unsubscribe
 * assertion (`scheduler.actions.length` stayed 1, not 0) -- confirmed discriminating.
 * The SECOND test (inactive-bind) stayed GREEN under the same sabotage, as expected:
 * it is not a teardown test, and clock_bucket is never declared by its program, so
 * clockBind's own activation filter (BindRunner constructor, unrelated to the
 * sabotaged commits$ plumbing) still keeps it out of `activeBinds` entirely. The
 * sabotage was reverted (`git checkout -- src/1_binds.ts`) before this file was
 * finalized; the third test was re-run GREEN against the real implementation to
 * confirm the fix-then-restore round trip.
 */
import assert from "node:assert/strict";
import { test } from "node:test";
import { TestScheduler } from "rxjs/testing";

import { bridge } from "../src/0_ast_bridge.ts";
import { clockBind } from "../src/1_binds.ts";
import type { BindDef } from "../src/0_types.ts";
import { builtinRelsForTests } from "./0_helpers.ts";
import {
  advanceVirtualSeconds,
  bootBindRunnerFixture,
  disposeBindFixture,
  waitForScheduledAction,
} from "./2_helpers_binds.ts";
import { waitUntil } from "./2_helpers_hosts.ts";

const ONE_SECOND_CLOCK_PROGRAM = `rel clock_period(period_secs: int).
clock_period(1).

rel(1) clock_bucket(period: int, bucket: int).

rel poll_due(period: int, bucket: int).
poll_due(period, bucket) <- clock_bucket(period, bucket).
`;

const NO_CLOCK_BUCKET_PROGRAM = `rel clock_period(period_secs: int).
clock_period(1).

rel unrelated_thing(name: text).
`;

function bridgeOrThrow(text: string) {
  const result = bridge(text, builtinRelsForTests());
  if (result.kind === "err") throw new Error(result.diags.map((diag) => diag.message).join("; "));
  return result;
}

/** No-op assertDeepEqual: this file never calls `TestScheduler.run()`/`expectObservable`
 *  (those are the marble-diagram API, which this file has no use for), only the raw
 *  `VirtualTimeScheduler` surface (`frame`/`maxFrames`/`flush`/`actions`) TestScheduler
 *  inherits -- but the constructor still requires the callback positionally. */
function newTestScheduler(): TestScheduler {
  return new TestScheduler(() => {});
}

test("clockBind fires on its declared period and rel(1) keeps only the latest bucket", async () => {
  const bridgeOk = bridgeOrThrow(ONE_SECOND_CLOCK_PROGRAM);
  // Real (default) scheduler, deliberately -- see this file's header.
  const fixture = await bootBindRunnerFixture(bridgeOk, [clockBind]);
  try {
    const firstBucket = await waitUntil(async () => {
      const rows = await fixture.rt.rows("clock_bucket");
      return rows.length === 1 ? rows[0] : undefined;
    });

    const secondBucket = await waitUntil(
      async () => {
        const rows = await fixture.rt.rows("clock_bucket");
        return rows.length === 1 && (rows[0]!.bucket as number) > (firstBucket!.bucket as number) ? rows[0] : undefined;
      },
      { attempts: 200, intervalMs: 25 },
    );

    // retention-1 (a GLOBAL "keep newest" sweep, 3_runtime.ts's applyRelWrite): with
    // exactly one active period, clock_bucket never accumulates more than one row.
    assert.equal((await fixture.rt.rows("clock_bucket")).length, 1);
    assert.equal(secondBucket!.period, 1);
    assert.ok((secondBucket!.bucket as number) > (firstBucket!.bucket as number));

    // poll_due (derived from clock_bucket) mirrors the same current value -- proof
    // that a bucket advance is a real retract+insert delta reaching a derived rule,
    // not just a private state change inside the bind.
    const pollDueRows = await fixture.rt.rows("poll_due");
    assert.deepEqual(pollDueRows, [{ period: 1, bucket: secondBucket!.bucket }]);
  } finally {
    await disposeBindFixture(fixture);
  }
});

test("clockBind stays inactive when the loaded program never declares clock_bucket", async () => {
  const bridgeOk = bridgeOrThrow(NO_CLOCK_BUCKET_PROGRAM);
  const scheduler = newTestScheduler();
  const fixture = await bootBindRunnerFixture(bridgeOk, [clockBind], scheduler);
  try {
    let commitCount = 0;
    const sub = fixture.runner.commits$.subscribe(() => {
      commitCount += 1;
    });
    try {
      // Virtual time, not a real sleep (F6): several periods' worth advanced
      // instantly. If the bind were (wrongly) active despite the program never
      // declaring `clock_bucket`, this would fire its interval and increment
      // commitCount -- the same discrimination the old 1500ms real wait bought,
      // at zero wall-clock cost.
      advanceVirtualSeconds(scheduler, 3);
      assert.equal(commitCount, 0);
    } finally {
      sub.unsubscribe();
    }
  } finally {
    await disposeBindFixture(fixture);
  }
});

test("unsubscribing BindRunner.commits$ stops the timer -- no bespoke dispose() needed", async () => {
  const bridgeOk = bridgeOrThrow(ONE_SECOND_CLOCK_PROGRAM);
  const scheduler = newTestScheduler();
  const fixture = await bootBindRunnerFixture(bridgeOk, [clockBind], scheduler);
  try {
    await waitForScheduledAction(scheduler, (actionCount) => actionCount > 0);
    advanceVirtualSeconds(scheduler, 1);
    await waitUntil(async () => {
      const rows = await fixture.rt.rows("clock_bucket");
      return rows.length === 1 ? rows : undefined;
    });
    assert.ok(
      scheduler.actions.length > 0,
      "the clock's interval action must still be scheduled while the bind is running",
    );

    // Tear down only the bind side; the runtime + its delta stream stay alive so a
    // stray commit reaching clock_bucket after teardown is still observable.
    fixture.running.unsubscribe();

    // F3: row equality alone does NOT discriminate a leaked timer here, because two
    // firings landing inside the same real wall-clock second compute the SAME
    // bucket value (bucketFor reads real Date.now(); this seam deliberately does not
    // virtualize it, see this file's header). A leaked interval re-committing the
    // identical (period, bucket) row under retention-1 would leave clock_bucket
    // looking unchanged even though it kept firing. Assert on the SCHEDULER's own
    // action queue instead -- this is the direction F3 itself named.
    assert.equal(
      scheduler.actions.length,
      0,
      "unsubscribing BindRunner.commits$ must remove the interval's scheduled action -- no leaked timer",
    );

    // Advance virtual time well past several more periods: a leaked subscription
    // would reschedule a new action here (confirmed: this line goes RED under the
    // held-internal-subscription sabotage described in this file's header).
    advanceVirtualSeconds(scheduler, 5);
    assert.equal(scheduler.actions.length, 0, "no new scheduled action may appear after teardown");

    // Corroborating (weaker on its own, per the note above, but still a real check
    // once the scheduler assertion has already discriminated the leak): the DB
    // itself never gained a row after teardown.
    const rowsAtTeardown = await fixture.rt.rows("clock_bucket");
    assert.equal(rowsAtTeardown.length, 1);
  } finally {
    // fixture.running is already unsubscribed above; Subscription.unsubscribe() is
    // idempotent, so calling it again here (inside disposeBindFixture) is harmless.
    await disposeBindFixture(fixture);
  }
});

// ---- contract proof ----------------------------------------------------------
const clockBindDefTypeHolds: BindDef = clockBind;
void clockBindDefTypeHolds;
