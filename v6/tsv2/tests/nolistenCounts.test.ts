/**
 * nolistenCounts.test.ts — RAIL C of the unread-rel skip contract (§6, §8
 * lane C): the standing COUNT receipt that the skip actually fires.
 *
 * The opposite failure mode from "a missed reader read a skipped rel's staging":
 * the skip silently NEVER fires, the driver keeps paying for the copies nobody
 * observes, and every tick runs unchanged. End-state equality alone cannot see
 * it — the skipped copies are the copies that would have been empty anyway. The
 * receipt is statements-per-tick, counted through a wrapped runner, dropping by
 * EXACTLY the copy set the skip removes, and it is FLAT as the source grows.
 *
 * The fixture is a two-relation program, bool_identity_comparison_filters:
 *   flag (the arrival source, observed — `ruleObservers: ["enabled_name/1"]`)
 *   enabled_name (derived, `ruleObservers: []` → skip-able)
 * Marking enabled_name via `seam.unobservedRels` makes `isUnobserved` true, and
 * one tick drops its staging copies:
 *   prepareTick clears          DELETE "__delta_enabled_name"
 *                               DELETE "__next_frontier_enabled_name"   (2)
 *   applyLevelStatement stage   INSERT "__delta_enabled_name"  (boundary)
 *                               INSERT "__frontier_enabled_name" (frontier) (2)
 *   readBoundary                boundarySql SELECT over "__delta_enabled_name"(1)
 *   promoteFrontiers            DELETE "__frontier_enabled_name"
 *                               INSERT "__frontier_enabled_name" FROM next
 *                               DELETE "__next_frontier_enabled_name"      (3)
 * = 8 copy statements per tick. The retraction-guard term and the merge term
 * are combined-SQL text changes, not statement-count changes, so they do not
 * move the count. This is the CONTRACT §4b skip set as it fires for this rel;
 * the contract's §6 prose said "ten", the measured per-tick count here is 8
 * because mergeNextIntoCurrent (this tick path never runs it) and retention
 * (none) do not fire. Deviations recorded in REPORT.md.
 *
 * SABOTAGE RECEIPT (RAIL A, fail-first, 2026-08-06): a scratch copy of
 * bool_identity_comparison_filters.ts with one line planted --
 * `const FAKE_DELTA_READ = \`SELECT "name" FROM "__delta_enabled_name"\`;`
 * -- where enabled_name has empty `ruleObservers`. Running the text audit
 * (scripts/nolisten-text-audit.mjs) against that one module returns:
 *
 *   [violation] module=bool_identity_comparison_filters.ts rel=enabled_name table=__delta_enabled_name
 *     SELECT "name" FROM "__delta_enabled_name"
 *   nolisten text audit: 1 modules, 1 unobserved rels (of 2), 12 staging refs, 1 violations
 *
 *   exit 2. The boundary SELECT is white-listed by its `"_sign" IN (-1, 1)`
 *   shape; any SELECT that names a skipped rel's staging without it is a find.
 *
 * FAIL-PRE-FIX RECEIPT (RAIL C, what this test exists to prove is NOT
 * happening): with the runtime skip absent, `isUnobserved`/`observedRels` were
 * never consulted, and one tick paid 20 statements whether or not enabled_name
 * sat in `unobservedRels`. The pin below asserts 12 with the skip active —
 * exactly 8 fewer than 20 — and that enabled_name's final head is byte-identical
 * in both runs.
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import { firstValueFrom, type Observable } from "rxjs";
import { SqlRunner } from "sprefa-store-engine/src/engine/sqlRunner.ts";
import type {
  ISqlRunner,
  SqliteDb,
  SqlStatement,
  TraceStatement,
} from "sprefa-store-engine/src/engine/types.ts";

import { program } from "../gen_emitted/bool_identity_comparison_filters.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import type { IArrivalRow, ISqlSeam } from "../runtime/types.ts";

/** The copy statements one tick drops for the unobserved rel; PINS against
 *  data size. Measured on the wrapper defined below. */
const SKIPPED_COPY_STATEMENTS = 8;
const SKIP_REL = "enabled_name";

/** A statement-shaped arrival batch for `source` rows: distinct names, all
 *  enabled, so the derived head actually grows with the source. */
function arrivals(value_count: number): readonly IArrivalRow[] {
  return Array.from({ length: value_count }, (_, index) => ({
    rel: "flag",
    sign: "add",
    row: [`name_${index}`, true],
  }));
}

/** Counts SQL per runner call, the way SqlRunner increments stmt_counter:
 *  execute/scalar one each, executeMultiple one per `;`-split part, batch one
 *  per statement. The seam's real runner is wrapped without changing it. */
function counting_runner(counter: { n: number }): ISqlRunner {
  const inner = SqlRunner;
  const split = (sql: string): number =>
    sql
      .split(";")
      .map((part) => part.trim())
      .filter((part) => part.length > 0).length;
  return {
    execute(db: SqliteDb, statement: SqlStatement, trace?: TraceStatement) {
      counter.n += 1;
      return inner.execute(db, statement, trace);
    },
    scalar(db: SqliteDb, statement: SqlStatement, trace?: TraceStatement) {
      counter.n += 1;
      return inner.scalar(db, statement, trace);
    },
    executeMultiple(db: SqliteDb, sql: string, trace?: TraceStatement) {
      counter.n += split(String(sql));
      return inner.executeMultiple(db, sql, trace);
    },
    batch(db: SqliteDb, statements: readonly SqlStatement[], trace?: TraceStatement) {
      counter.n += statements.length;
      return inner.batch(db, statements, trace);
    },
    inTransaction<Value>(db: SqliteDb, body: () => Observable<Value>): Observable<Value> {
      return inner.inTransaction(db, body);
    },
  };
}

function seam_with(unobserved: boolean): { seam: ISqlSeam; counter: { n: number } } {
  const base = ScratchStore.open(":memory:");
  const counter = { n: 0 };
  const seam = unobserved
    ? { ...base, runner: counting_runner(counter), unobserved_rels: new Set([SKIP_REL]) }
    : { ...base, runner: counting_runner(counter) };
  return { seam, counter };
}

async function run_one_tick(value_count: number, unobserved: boolean) {
  const { seam, counter } = seam_with(unobserved);
  await firstValueFrom(ScratchStore.boot(seam, program.ddl));
  counter.n = 0;
  await firstValueFrom(program.tick(seam, arrivals(value_count)));
  const statements = counter.n;
  const final = await firstValueFrom(
    seam.runner.execute(seam.db, program.final_select[SKIP_REL]!),
  );
  const head = final.rows
    .map((row) => String((row as Record<string, unknown>).name))
    .sort();
  seam.db.close();
  return { statements, head };
}

test("the unobserved-rel skip cuts exactly the copy statements, flat in source rows", async () => {
  const small = { active: await run_one_tick(5, true), inactive: await run_one_tick(5, false) };
  const large = { active: await run_one_tick(100, true), inactive: await run_one_tick(100, false) };

  for (const [label, pair] of [
    ["5 rows", small],
    ["100 rows", large],
  ] as const) {
    assert.equal(
      pair.inactive.statements - pair.active.statements,
      SKIPPED_COPY_STATEMENTS,
      `${label}: skip active ran ${pair.active.statements} statements, inactive ran ${pair.inactive.statements}; expected exactly ${SKIPPED_COPY_STATEMENTS} copy statements dropped`,
    );
    assert.deepEqual(
      pair.active.head,
      pair.inactive.head,
      `${label}: skipping the unobserved rel must not change the derived head`,
    );
  }

  // Non-vacuity: the source really does scale, so a flat delta is flat over
  // real derivation, not over an empty fixture.
  assert.equal(small.active.head.length, 5, "5 enabled names derived");
  assert.equal(large.active.head.length, 100, "100 enabled names derived");
});
