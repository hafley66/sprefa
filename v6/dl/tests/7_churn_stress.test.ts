/**
 * tests/7_churn_stress.test.ts — M8-beta churn stress (the owner's escalation gate,
 * tasks.d.ts's IdentityWitnessLaw). Drives N synthetic file identities through M
 * sequential content-witness edits each, simulating exactly the retract+insert shape
 * 4_ingest.ts's ingestFile produces per edit, over a counting fake host (deterministic,
 * no subprocess -- tests/2_helpers_hosts.ts's makeCountingHost). Proves the supersession
 * mechanics hold under volume, not just the single-edit cases 4_hosts.test.ts covers:
 * exact fire-once-per-witness run count, per-edit "only the live witness survives" at
 * every step, linear (not quadratic) delta-log growth with a formula derived and stated
 * below, and a loose RSS ceiling.
 *
 * The program is hand-built with sprefa-store-engine's ast.ts constructors (edbRel/
 * derivedRel/headVar/relRef/v), NOT bridged from .dl text: this test only needs the
 * SHAPE the bridge would have minted for a salted probe (`__req_counter(path, salt_0)`
 * derived from `file`, `__resp_counter(path, salt_0, value)` EDB, a `churn_hit` derived
 * rel joining `file` and `__resp_counter` on the shared witness var -- the exact analog
 * of sg-rail.dl's `console_hit`), wired via tests/1_helpers_db.ts's fakeBridgeOk so
 * bootHostRunnerFixture boots it exactly like any other fixture.
 */
import assert from "node:assert/strict";
import { test } from "node:test";

import { derivedRel, edbRel, headVar, relRef, v, type Program, type Rule } from "sprefa-store-engine/src/lower/ast.ts";

import type { Row } from "../tasks.d.ts";
import { deltaDump, edbBatch, fakeBridgeOk, rowsOf } from "./1_helpers_db.ts";
import { bootHostRunnerFixture, disposeHostFixture, effectCacheDump, makeCountingHost, waitUntil } from "./2_helpers_hosts.ts";

const IDENTITY_COUNT = 6;
const EDIT_COUNT = 5;

function identityPath(identityIndex: number): string {
  return `synthetic/file-${identityIndex}.ts`;
}

function witnessHash(identityIndex: number, editIndex: number): string {
  return `hash-${identityIndex}-w${editIndex}`;
}

/** file(path, content_hash) EDB + the salted-probe shape a bridge would mint for
 *  `counter?(path, content_hash, value)` against a `counter(path, value)` host whose
 *  template only refs `{path}` -- __req_counter derived (path, salt_0), __resp_counter
 *  EDB (path, salt_0, value), and churn_hit derived (path, value) as the console_hit
 *  analog: joins file and __resp_counter on the SAME shared witness variable, so it
 *  loses its match the instant file's content_hash flips (exactly sg-rail.dl's
 *  console_hit/diag mechanism -- curl-session.sh's finding #2). */
function buildChurnProgram(): Program {
  const fileDecl = edbRel("file", ["path", "content_hash"]);
  const reqCounterDecl = derivedRel("__req_counter", ["path", "salt_0"]);
  const respCounterDecl = edbRel("__resp_counter", ["path", "salt_0", "value"]);
  const churnHitDecl = derivedRel("churn_hit", ["path", "value"]);

  const reqCounterRule: Rule = {
    head: "__req_counter",
    headTerms: [headVar("path"), headVar("content_hash")],
    body: [relRef("file", v("path"), v("content_hash"))],
  };
  const churnHitRule: Rule = {
    head: "churn_hit",
    headTerms: [headVar("path"), headVar("value")],
    body: [
      relRef("file", v("path"), v("content_hash")),
      relRef("__resp_counter", v("path"), v("content_hash"), v("value")),
    ],
  };

  return {
    rels: [fileDecl, reqCounterDecl, respCounterDecl, churnHitDecl],
    rules: [reqCounterRule, churnHitRule],
  };
}

test(
  "churn stress: N identities x M witness edits -- fire-once-per-witness, linear delta growth, bounded RSS",
  { timeout: 30_000 },
  async () => {
    const bridgeOk = fakeBridgeOk(buildChurnProgram());
    const counting = makeCountingHost("counter");
    const fixture = await bootHostRunnerFixture(bridgeOk, [counting.host]);

    const commitMsSamples: number[] = [];
    const rssBefore = process.memoryUsage().rss;
    const wallStart = performance.now();

    try {
      for (let identityIndex = 0; identityIndex < IDENTITY_COUNT; identityIndex++) {
        const path = identityPath(identityIndex);
        for (let editIndex = 0; editIndex < EDIT_COUNT; editIndex++) {
          const newHash = witnessHash(identityIndex, editIndex);
          // Exactly ingestFile's shape (4_ingest.ts): one commit, retract the prior
          // witness's file row (if any) + insert the new one -- never two commits.
          const insert: Readonly<Record<string, readonly Row[]>> = { file: [{ path, content_hash: newHash }] };
          const retract: Readonly<Record<string, readonly Row[]>> =
            editIndex === 0 ? {} : { file: [{ path, content_hash: witnessHash(identityIndex, editIndex - 1) }] };

          const commitStart = performance.now();
          await fixture.rt.commit(edbBatch(insert, retract));
          commitMsSamples.push(performance.now() - commitStart);

          // Bounded poll: HostRunner's supersession commit lands on a LATER tick than
          // this file edit's own commit (the same async gap curl-session.sh's finding
          // #2 documents) -- wait for THIS identity to have EXACTLY one live
          // __resp_counter row, and for it to be the NEW witness, before driving the
          // next edit. This poll IS assertion (b) ("after each edit, rows(__resp) for
          // that identity reflect ONLY the live witness"), checked at every step, not
          // just at the end.
          await waitUntil(async () => {
            const respRowsForPath = (await fixture.rt.rows("__resp_counter")).filter((row) => row.path === path);
            return respRowsForPath.length === 1 && respRowsForPath[0]?.salt_0 === newHash ? respRowsForPath : undefined;
          });
        }
      }

      const wallMs = performance.now() - wallStart;
      const rssAfter = process.memoryUsage().rss;

      // (a) executor run count === distinct (identity, witness) pairs === N*M: every
      // edit is a genuinely new witness, so every edit is a cache MISS, fires once.
      assert.equal(counting.runCount, IDENTITY_COUNT * EDIT_COUNT);

      // Final full-table check closing the per-edit poll above: exactly one live
      // witness row per identity, matching each identity's LAST edit.
      const finalResp = await rowsOf(fixture.rt, "__resp_counter");
      assert.equal(finalResp.length, IDENTITY_COUNT);
      for (let identityIndex = 0; identityIndex < IDENTITY_COUNT; identityIndex++) {
        const path = identityPath(identityIndex);
        const liveRow = finalResp.find((row) => row.path === path);
        assert.ok(liveRow, `expected a live __resp_counter row for ${path}`);
        assert.equal(liveRow?.salt_0, witnessHash(identityIndex, EDIT_COUNT - 1));
      }

      // The orchestrator's latest-wins interpretation (1_hosts.ts's runEffectOnce,
      // flagged there, not owner-ruled) at scale: effect_cache holds exactly one row
      // per identity -- the delete keeps pace with N identities x M edits, not
      // growing unbounded with the edit count.
      assert.equal((await effectCacheDump(fixture.dbPath)).length, IDENTITY_COUNT);

      // (c) delta log growth is linear in (identity, edit) pairs, not quadratic.
      // Per identity, the FIRST edit is insert-only (no prior witness to retract) and
      // produces 4 delta rows across two ticks:
      //   tick A (the file insert commit itself, one transaction): file +1,
      //     __req_counter +1 (both derived synchronously from the SAME commit --
      //     __req_counter's rule body is just `file(path, content_hash)`).
      //   tick B (HostRunner's async run; no superseded rows since this identity has
      //     no prior witness): __resp_counter +1, churn_hit +1 (churn_hit's join
      //     against `file` only NOW finds a partner in __resp_counter).
      // Every SUBSEQUENT edit (M-1 of them per identity) retracts a prior witness and
      // produces 8 delta rows across two ticks:
      //   tick A (the file retract+insert commit): file +1/-1 (2 rows), __req_counter
      //     +1/-1 (2 rows, the derived rel recomputes to the new (path,salt_0) pair),
      //     AND churn_hit -1 (1 row: its match against the STILL-stale __resp_counter
      //     row breaks the instant file's content_hash flips -- the exact mechanism
      //     curl-session.sh's finding #2 documents, no special retraction code).
      //   tick B (HostRunner's async supersession commit): __resp_counter +1/-1 (2
      //     rows, the new witness lands, the prior one retracts in the SAME commit),
      //     churn_hit +1 (1 row, the new witness's row now matches file's current
      //     content_hash).
      //   2+2+1 (tick A) + 2+1 (tick B) = 8.
      // total = N * (4 + 8*(M-1)); for N=6, M=5: 6 * (4 + 32) = 216.
      const expectedDeltaRows = IDENTITY_COUNT * (4 + 8 * (EDIT_COUNT - 1));
      const deltaRows = await deltaDump(fixture.dbPath);
      assert.equal(deltaRows.length, expectedDeltaRows);

      // (d) RSS growth bound. Deliberately loose: libsql's own connection/WAL
      // machinery has documented, accepted noise on a run this size (see the M8 perf
      // baseline note pinned alongside IdentityWitnessLaw in tasks.d.ts -- store
      // stress large-golden rx/sql peak RSS in the hundreds of MB with double-digit
      // slopes) that would make a tight bound dishonest for a 30-effect run; 75MB is a
      // "nothing is leaking wildly" ceiling, not a tuned budget.
      const rssGrowthMb = (rssAfter - rssBefore) / (1024 * 1024);
      assert.ok(rssGrowthMb < 75, `rss grew ${rssGrowthMb.toFixed(2)}MB across the run, expected < 75MB`);

      // Timing receipts (arrays + sort, no deps -- plain JS, nothing new to justify
      // under build-vs-buy for a sort of 30 numbers).
      const sortedCommitMs = [...commitMsSamples].sort((left, right) => left - right);
      const percentile = (p: number): number => {
        const index = Math.min(sortedCommitMs.length - 1, Math.floor((p / 100) * sortedCommitMs.length));
        return sortedCommitMs[index] ?? 0;
      };
      console.log(
        `[churn-stress] total_wall_ms=${wallMs.toFixed(1)} commits=${commitMsSamples.length} ` +
          `ms_per_commit_p50=${percentile(50).toFixed(2)} ms_per_commit_p95=${percentile(95).toFixed(2)} ` +
          `rss_growth_mb=${rssGrowthMb.toFixed(2)} delta_rows=${deltaRows.length}`,
      );
    } finally {
      await disposeHostFixture(fixture);
    }
  },
);
