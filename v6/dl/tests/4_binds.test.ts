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
 */
import assert from "node:assert/strict";
import { test } from "node:test";

import { bridge } from "../src/0_ast_bridge.ts";
import { clockBind } from "../src/1_binds.ts";
import type { BindDef } from "../src/0_types.ts";
import { builtinRelsForTests } from "./0_helpers.ts";
import { bootBindRunnerFixture, disposeBindFixture } from "./2_helpers_binds.ts";
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

test("clockBind fires on its declared period and rel(1) keeps only the latest bucket", async () => {
  const bridgeOk = bridgeOrThrow(ONE_SECOND_CLOCK_PROGRAM);
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
  const fixture = await bootBindRunnerFixture(bridgeOk, [clockBind]);
  try {
    let commitCount = 0;
    const sub = fixture.runner.commits$.subscribe(() => {
      commitCount += 1;
    });
    try {
      // Longer than the declared period: if the bind were (wrongly) active despite
      // the program never declaring `clock_bucket`, this window would catch a firing.
      await new Promise((resolve) => setTimeout(resolve, 1500));
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
  const fixture = await bootBindRunnerFixture(bridgeOk, [clockBind]);
  try {
    await waitUntil(async () => {
      const rows = await fixture.rt.rows("clock_bucket");
      return rows.length === 1 ? rows : undefined;
    });

    // Tear down only the bind side; the runtime + its delta stream stay alive so a
    // stray commit reaching clock_bucket after teardown is still observable.
    fixture.running.unsubscribe();
    const rowsAtTeardown = await fixture.rt.rows("clock_bucket");

    await new Promise((resolve) => setTimeout(resolve, 2500));
    const rowsAfterWaiting = await fixture.rt.rows("clock_bucket");
    assert.deepEqual(rowsAfterWaiting, rowsAtTeardown);
  } finally {
    // fixture.running is already unsubscribed above; Subscription.unsubscribe() is
    // idempotent, so calling it again here (inside disposeBindFixture) is harmless.
    await disposeBindFixture(fixture);
  }
});

// ---- contract proof ----------------------------------------------------------
const clockBindDefTypeHolds: BindDef = clockBind;
void clockBindDefTypeHolds;
