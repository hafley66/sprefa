/**
 * tests/2_helpers_hosts.ts — shared host-test setup (M4). Numbered 2_ because it
 * imports tests/1_helpers_db.ts (scratch-db-file helpers) + src/1_hosts.ts (HostRunner,
 * HostDef, CacheDb); tests/4_hosts.test.ts sits above it at 4_ because it also
 * exercises the runtime (src/3_runtime.ts, numbered 3_) alongside the hosts machinery.
 *
 * Exports: bridgeHostsFixture (bridge() a fixture .dl against the standard test
 * builtins, throwing on a load error so test bodies read as straight-line happy path),
 * bootHostRunnerFixture/disposeHostFixture (boot a DlRuntime + a HostRunner wired to a
 * fresh libsql connection over the same db file, per the pinned cacheDb resolution),
 * effectCacheDump (deterministic effect_cache read for snapshot comparison), waitUntil
 * (bounded poll -- no arbitrary sleeps), makeCountingHost (M8-beta: a deterministic,
 * no-subprocess HostDef stand-in used by the supersession/ABA tests and by
 * tests/7_churn_stress.test.ts, so neither has to spawn a real process to prove the
 * identity/witness mechanics).
 */
import { createClient, type Client } from "@libsql/client";

import { bridge } from "../src/0_ast_bridge.ts";
import { HostRunner, type CacheDb, type HostDef } from "../src/1_hosts.ts";
import { DlRuntime } from "../src/3_runtime.ts";
import type { BridgeOk, Row } from "../tasks.d.ts";
import { builtinRelsForTests, readFixture } from "./0_helpers.ts";
import { cleanupDbFile, freshDbPath } from "./1_helpers_db.ts";

/** bridge() a fixture .dl file against the standard test builtins; throws with the
 *  joined diag messages on a load error (every M4 fixture is expected to bridge
 *  cleanly, so a throw here reads as a setup bug, not a test assertion). */
export function bridgeHostsFixture(name: string): BridgeOk {
  const result = bridge(readFixture(name), builtinRelsForTests());
  if (result.kind === "err") {
    throw new Error(`bridgeHostsFixture(${name}): ${result.diags.map((diag) => diag.message).join("; ")}`);
  }
  return result;
}

export interface HostFixture {
  readonly rt: DlRuntime;
  readonly runner: HostRunner;
  readonly dbPath: string;
  readonly cacheClient: Client;
}

/** Boots a DlRuntime against a fresh scratch db, then a HostRunner over the SAME file
 *  via a second libsql connection (the pinned cacheDb resolution: HostRunner receives
 *  the runtime only for deltas$/commit; a libsql-shaped client for effect_cache reads/
 *  writes is a separate connection to the same on-disk file). runner.start() is called
 *  before returning -- tests never forget to wire the subscription. */
export async function bootHostRunnerFixture(bridgeOk: BridgeOk, hosts: readonly HostDef[]): Promise<HostFixture> {
  const dbPath = freshDbPath();
  const rt = await DlRuntime.boot({ dbPath, bridge: bridgeOk });
  const cacheClient = createClient({ url: `file:${dbPath}` });
  const runner = new HostRunner(rt, hosts, cacheClient as unknown as CacheDb);
  runner.start();
  return { rt, runner, dbPath, cacheClient };
}

export async function disposeHostFixture(fixture: HostFixture): Promise<void> {
  fixture.runner.dispose();
  await fixture.rt.dispose();
  fixture.cacheClient.close();
  cleanupDbFile(fixture.dbPath);
}

/** M8-beta reshape (IdentityWitnessLaw, tasks.d.ts): full_digest is the fire-once PK,
 *  identity_digest is the supersession group key. */
export interface EffectCacheEntry {
  readonly full_digest: number;
  readonly identity_digest: number;
  readonly host: string;
  readonly state: string;
  readonly requested_tick: number;
}

/** Reads the whole effect_cache table directly (mirrors 1_helpers_db.ts's deltaDump):
 *  a fresh short-lived connection, closed before returning, sorted for determinism. */
export async function effectCacheDump(dbPath: string): Promise<EffectCacheEntry[]> {
  const db = createClient({ url: `file:${dbPath}` });
  try {
    const res = await db.execute(
      "SELECT full_digest, identity_digest, host, state, requested_tick FROM effect_cache ORDER BY full_digest",
    );
    return res.rows
      .map((row) => ({
        full_digest: Number(row.full_digest),
        identity_digest: Number(row.identity_digest),
        host: String(row.host),
        state: String(row.state),
        requested_tick: Number(row.requested_tick),
      }))
      .sort((a, b) => a.full_digest - b.full_digest);
  } finally {
    db.close();
  }
}

/** Bounded poll: no arbitrary sleeps. Retries `check` on a short interval until it
 *  returns a truthy value or the attempt budget is exhausted, in which case it throws
 *  (a timed-out wait reads as a distinct failure from a wrong-value assertion). */
export async function waitUntil<T>(
  check: () => Promise<T | undefined | false | null | 0 | "">,
  options: { readonly attempts?: number; readonly intervalMs?: number } = {},
): Promise<T> {
  const attempts = options.attempts ?? 100;
  const intervalMs = options.intervalMs ?? 20;
  for (let attempt = 0; attempt < attempts; attempt++) {
    const result = await check();
    if (result) return result;
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
  throw new Error(`waitUntil: condition not met after ${attempts} attempts (${attempts * intervalMs}ms)`);
}

// ─────────────────────────────────────────────────────────────────────────────
// makeCountingHost: a deterministic, no-subprocess HostDef stand-in (M8-beta). Used
// wherever a test needs to prove supersession/ABA/churn mechanics without spawning a
// real process -- requestCols ["path"] (the identity column any bridged `sh <name>`
// decl with a `{path}` template ref infers), responseCols ["path","value"]. Each
// run() call increments a shared counter and echoes the request's path, so a test can
// assert the EXACT number of executor runs (== distinct (identity,witness) pairs).
// ─────────────────────────────────────────────────────────────────────────────

export class CountingHost {
  runCount = 0;
  readonly calls: Row[] = [];
  readonly host: HostDef;

  constructor(name: string) {
    this.host = {
      name,
      requestCols: ["path"],
      responseCols: ["path", "value"],
      run: (request: Row) => this.run(request),
    };
  }

  private async *run(request: Row): AsyncIterable<Row> {
    this.runCount += 1;
    this.calls.push(request);
    yield { path: request.path ?? null, value: this.runCount };
  }
}

export function makeCountingHost(name: string): CountingHost {
  return new CountingHost(name);
}
