/**
 * tests/2_helpers_binds.ts -- shared bind-test setup, mirroring tests/2_helpers_hosts.ts
 * exactly (same numbering reasoning: imports tests/1_helpers_db.ts + src/1_binds.ts,
 * sits below tests/4_binds.test.ts which also exercises the runtime).
 *
 * Exports: bridgeBindsFixture (bridge() a fixture .dl against the standard test
 * builtins), bootBindRunnerFixture/disposeBindFixture (boot a DlRuntime + a BindRunner
 * over it, subscribed the same way main.ts composes the real app graph).
 */
import { merge, type Subscription } from "rxjs";

import { bridge } from "../src/0_ast_bridge.ts";
import { BindRunner, type BindDef } from "../src/1_binds.ts";
import { DlRuntime } from "../src/3_runtime.ts";
import type { BridgeOk } from "../tasks.d.ts";
import { builtinRelsForTests, readFixture } from "./0_helpers.ts";
import { cleanupDbFile, freshDbPath } from "./1_helpers_db.ts";

/** bridge() a fixture .dl file against the standard test builtins; throws with the
 *  joined diag messages on a load error, same convention as bridgeHostsFixture. */
export function bridgeBindsFixture(name: string): BridgeOk {
  const result = bridge(readFixture(name), builtinRelsForTests());
  if (result.kind === "err") {
    throw new Error(`bridgeBindsFixture(${name}): ${result.diags.map((diag) => diag.message).join("; ")}`);
  }
  return result;
}

export interface BindFixture {
  readonly rt: DlRuntime;
  readonly runner: BindRunner;
  readonly dbPath: string;
  /** The test's stand-in for main.ts's terminal subscription: it runs the tick loop
   *  and the runner's commits for the life of the fixture. Merged in that order, same
   *  as bootHostRunnerFixture, so deltas$ is live before the runner's own source$
   *  observables start firing. */
  readonly running: Subscription;
}

export async function bootBindRunnerFixture(bridgeOk: BridgeOk, binds: readonly BindDef[]): Promise<BindFixture> {
  const dbPath = freshDbPath();
  const rt = await DlRuntime.boot({ dbPath, bridge: bridgeOk });
  const runner = new BindRunner(rt, binds, bridgeOk.program);
  const running = merge(rt.deltas$, runner.commits$).subscribe({
    error: (failure: unknown) => {
      throw failure;
    },
  });
  return { rt, runner, dbPath, running };
}

export async function disposeBindFixture(fixture: BindFixture): Promise<void> {
  fixture.running.unsubscribe();
  await fixture.rt.dispose();
  cleanupDbFile(fixture.dbPath);
}
