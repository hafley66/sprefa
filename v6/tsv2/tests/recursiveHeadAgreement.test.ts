/**
 * recursiveHeadAgreement.test.ts — the REFEREE for in-place recursive-head
 * maintenance (IDredPlan): the emitted head must equal the transitive closure
 * of the surviving base rows after every tick of an insert/delete script.
 *
 * The referee is computed here in plain JS from `flow_edge`, so it agrees with
 * neither the assert walk nor the DRed walk by construction. That is what the
 * 420-fixture sweep cannot give: only four fixtures have a recursive head and
 * none of them retracts one, so the sweep grades the assert half alone.
 *
 * The cycle case is the one a refCount gets wrong. Cutting a cycle's only
 * anchor leaves every row of the cycle deriving every other, so counting
 * derivations keeps them all alive as phantoms; DRed over-deletes the cone and
 * finds no surviving anchor, so they die.
 *
 * SABOTAGE RECEIPT (2026-08-06, why the liveness probe in the delta seeds is
 * not decoration): with `dred_delta_select/4` reading `_sign = 1` alone, the
 * randomized leg failed at seeds 4 and 5 with `extra=n4|n4` — a row added and
 * retracted inside ONE tick stays in the cumulative delta table under both
 * signs, and the assert half rederived the head row from the dead fact. The
 * sweep stayed 420/wrong=0 through that defect.
 *
 * The last two legs grade the MID-WALK BAIL (`dredHalf`, cone > head/4) with
 * `flow_reach` in `unobservedRels`, which is the configuration analyze.pl's
 * self-read refinement created: this fixture's `flow_reach` renders
 * `ruleObservers: []`, so a caller that does not subscribe to it takes the
 * SKIPPED bail path, where `stageRetract` and the frontier copies are dropped
 * and only the head table carries the answer. Bail and no-bail are told apart
 * by the SQL that ran: `__support_next_flow_reach` appears only on the
 * recompute the bail falls back to, `headDeleteSql` only when the walk was
 * allowed to finish.
 *
 * SABOTAGE RECEIPT (2026-08-07, why the bail legs are not asserting a
 * constant): with `coneCap` forced to `Number.MAX_SAFE_INTEGER` in `dredHalf`
 * so no walk can ever bail, the bail leg failed on `expected the bail to fall
 * back to the refCount recompute` while the no-bail leg stayed green; with
 * `coneCap` forced to `-1` so every walk bails, the no-bail leg failed on `a
 * walk under the cap must not fall back to the recompute` while the bail leg
 * stayed green. Under BOTH sabotages every referee check passed, which is the
 * point and the reason each leg grades its head before it grades its branch:
 * the bail is a COST decision, and the head is right either way.
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import { firstValueFrom, tap } from "rxjs";

import { program } from "../gen_emitted/flagship_flow_reach_over_batched_resolved_edges.ts";

// The rel is authored `flow_reach`; the table carries this compilation
// unit's module prefix (compile.pl relation_storage_names/4).
const FLOW_REACH_TABLE = "flagship_flow_reach_over_batched_resolved_edges_flow_reach";
import { ScratchStore } from "../runtime/scratchStore.ts";
// ISqlRunner is not on runtime/types.ts's re-export list; this is the module
// runtime/types.ts itself sources it from.
import type { ISqlRunner } from "sprefa-store-engine/src/engine/types.ts";
import type { IArrivalRow, ISqlSeam, SqlStatement } from "../runtime/types.ts";

type Pair = readonly [string, string];

function call_edge(sign: "add" | "del", from: string, to: string): IArrivalRow {
  return { rel: "resolved_call_edge", sign, row: [from, "fn", to, "fn"] };
}

/** The referee: naive closure to fixpoint, no incrementality anywhere. */
function transitive_closure(edges: readonly Pair[]): Set<string> {
  const reached = new Set<string>(edges.map(([from, to]) => `${from}|${to}`));
  for (;;) {
    let grew = false;
    for (const key of [...reached]) {
      const [from, middle] = key.split("|");
      for (const [tail, head] of edges) {
        if (tail === middle && !reached.has(`${from}|${head}`)) {
          reached.add(`${from}|${head}`);
          grew = true;
        }
      }
    }
    if (!grew) break;
  }
  return reached;
}

async function head_of(seam: ISqlSeam): Promise<Set<string>> {
  const result = await firstValueFrom(
    seam.runner.execute(seam.db, program.final_select.flow_reach!),
  );
  return new Set(
    result.rows.map((row) => `${String(row.from_path)}|${String(row.to_path)}`),
  );
}

function disagreement(head: Set<string>, referee: Set<string>): string {
  const missing = [...referee].filter((key) => !head.has(key));
  const extra = [...head].filter((key) => !referee.has(key));
  return missing.length === 0 && extra.length === 0
    ? ""
    : `missing=[${missing.join(" ")}] extra=[${extra.join(" ")}]`;
}

async function open_program(): Promise<ISqlSeam> {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, program.ddl));
  return seam;
}

/** Every SQL text the tick ran, so a leg can name the branch it took rather
 *  than infer it from the head it produced. */
function recording_runner(inner: ISqlRunner, executed: string[]): ISqlRunner {
  const note = (statement: SqlStatement | string): void => {
    executed.push(typeof statement === "string" ? statement : statement.sql);
  };
  return {
    execute(db, statement, trace) {
      return inner.execute(db, statement, trace).pipe(tap(() => note(statement)));
    },
    scalar(db, statement, trace) {
      return inner.scalar(db, statement, trace).pipe(tap(() => note(statement)));
    },
    executeMultiple(db, sql, trace) {
      return inner.executeMultiple(db, sql, trace).pipe(tap(() => note(sql)));
    },
    batch(db, statements, trace) {
      return inner.batch(db, statements, trace).pipe(
        tap(() => statements.forEach((statement) => note(statement))),
      );
    },
    inTransaction(db, body) {
      return inner.inTransaction(db, body);
    },
  };
}

/** `flow_reach` renders `ruleObservers: []`, so naming it here is what makes
 *  the runtime drop its event copies and take the skipped path. */
async function open_unobserved_program(
  executed: string[],
): Promise<ISqlSeam> {
  const opened = ScratchStore.open(":memory:");
  const seam: ISqlSeam = {
    ...opened,
    runner: recording_runner(opened.runner, executed),
    unobserved_rels: new Set(["flow_reach"]),
  };
  await firstValueFrom(ScratchStore.boot(seam, program.ddl));
  return seam;
}

/** A chain of `length` hops closes to `length*(length+1)/2` head rows, and
 *  cutting hop `at` orphans `(at+1)*(length-at)` of them -- past the cone cap
 *  of a quarter of the head when the cut is near the middle. */
function chain(length: number): IArrivalRow[] {
  return Array.from({ length }, (_unused, hop) =>
    call_edge("add", `n${hop}`, `n${hop + 1}`),
  );
}

function chain_pairs(length: number): Pair[] {
  return Array.from({ length }, (_unused, hop): Pair => [`n${hop}`, `n${hop + 1}`]);
}

test("scripted insert/delete ticks agree with the closure referee", async () => {
  const seam = await open_program();
  const live: Pair[] = [];
  const script: { readonly name: string; readonly rows: readonly IArrivalRow[] }[] = [
    { name: "chain", rows: [call_edge("add", "a", "b"), call_edge("add", "b", "c"), call_edge("add", "c", "d")] },
    {
      name: "cycle behind one anchor",
      rows: [call_edge("add", "p", "q"), call_edge("add", "q", "r"), call_edge("add", "r", "s"), call_edge("add", "s", "q")],
    },
    { name: "cut the cycle's only anchor", rows: [call_edge("del", "p", "q")] },
    { name: "cut the chain mid-way", rows: [call_edge("del", "b", "c")] },
    { name: "put it back", rows: [call_edge("add", "b", "c")] },
    { name: "add and retract in one tick", rows: [call_edge("add", "z", "z"), call_edge("del", "z", "z")] },
    {
      name: "kill the cycle outright",
      rows: [call_edge("del", "q", "r"), call_edge("del", "r", "s"), call_edge("del", "s", "q")],
    },
  ];

  for (const step of script) {
    for (const row of step.rows) {
      const pair: Pair = [String(row.row[0]), String(row.row[2])];
      if (row.sign === "add") live.push(pair);
      else {
        const at = live.findIndex(([from, to]) => from === pair[0] && to === pair[1]);
        if (at >= 0) live.splice(at, 1);
      }
    }
    await firstValueFrom(program.tick(seam, step.rows));
    const verdict = disagreement(await head_of(seam), transitive_closure(live));
    assert.equal(verdict, "", `${step.name}: ${verdict}`);
  }
  seam.db.close();
});

test("randomized insert/delete ticks agree with the closure referee", async () => {
  const NODE_COUNT = 9;
  const TICKS = 40;
  for (const start_seed of [4, 5, 77, 303]) {
    let state = start_seed;
    const next = (): number => {
      state = (state * 1103515245 + 12345) & 0x7fffffff;
      return state / 0x7fffffff;
    };
    const seam = await open_program();
    const live = new Set<string>();
    for (let tick = 0; tick < TICKS; tick += 1) {
      const rows: IArrivalRow[] = [];
      const batch_size = 1 + Math.floor(next() * 4);
      for (let index = 0; index < batch_size; index += 1) {
        if (live.size > 0 && next() < 0.45) {
          const keys = [...live];
          const key = keys[Math.floor(next() * keys.length)]!;
          const [from, to] = key.split("|");
          live.delete(key);
          rows.push(call_edge("del", from!, to!));
        } else {
          const from = `n${Math.floor(next() * NODE_COUNT)}`;
          const to = `n${Math.floor(next() * NODE_COUNT)}`;
          if (live.has(`${from}|${to}`)) continue;
          live.add(`${from}|${to}`);
          rows.push(call_edge("add", from, to));
        }
      }
      if (rows.length === 0) continue;
      await firstValueFrom(program.tick(seam, rows));
      const referee = transitive_closure(
        [...live].map((key) => key.split("|") as unknown as Pair),
      );
      const verdict = disagreement(await head_of(seam), referee);
      assert.equal(verdict, "", `seed ${start_seed} tick ${tick}: ${verdict}`);
    }
    seam.db.close();
  }
});

const CHAIN_HOPS = 12;

test("an unobserved head bails past cone > head/4 and still agrees with the referee", async () => {
  const executed: string[] = [];
  const seam = await open_unobserved_program(executed);
  await firstValueFrom(program.tick(seam, chain(CHAIN_HOPS)));

  const live = chain_pairs(CHAIN_HOPS);
  assert.equal(
    disagreement(await head_of(seam), transitive_closure(live)).length,
    0,
    "the chain must close before the cut is graded",
  );
  const head_rows = (await head_of(seam)).size;

  executed.length = 0;
  const cut_at = CHAIN_HOPS / 2;
  await firstValueFrom(program.tick(seam, [call_edge("del", `n${cut_at}`, `n${cut_at + 1}`)]));
  const survived = live.filter(([from]) => from !== `n${cut_at}`);

  const cone = (cut_at + 1) * (CHAIN_HOPS - cut_at);
  assert.ok(
    cone > Math.floor(head_rows / 4),
    `the cut must outgrow the cone cap: cone ${cone} vs cap ${Math.floor(head_rows / 4)}`,
  );
  // Correctness before cost: the head must be right whichever branch ran, so
  // this is graded first and the branch assertion below only prices it.
  assert.equal(
    disagreement(await head_of(seam), transitive_closure(survived)),
    "",
    "the bailed tick must leave the same head an in-place walk would",
  );
  // The skip is only sound if it actually skipped: a staged event here would
  // mean the unobserved path wrote copies nobody reads.
  assert.ok(
    !executed.some((sql) => sql.includes(`__delta_${FLOW_REACH_TABLE}`)),
    "an unobserved head must stage no delta rows on a bail tick",
  );
  assert.ok(
    executed.some((sql) => sql.includes(`__support_next_${FLOW_REACH_TABLE}`)),
    "expected the bail to fall back to the refCount recompute",
  );
  seam.db.close();
});

test("an unobserved head under the cone cap finishes the walk in place", async () => {
  const executed: string[] = [];
  const seam = await open_unobserved_program(executed);
  await firstValueFrom(program.tick(seam, chain(CHAIN_HOPS)));

  const live = chain_pairs(CHAIN_HOPS);
  executed.length = 0;
  // The last hop orphans one head row per prefix start, which is the smallest
  // cone this shape can produce.
  await firstValueFrom(
    program.tick(seam, [call_edge("del", `n${CHAIN_HOPS - 1}`, `n${CHAIN_HOPS}`)]),
  );
  const survived = live.filter(([from]) => from !== `n${CHAIN_HOPS - 1}`);

  assert.equal(
    disagreement(await head_of(seam), transitive_closure(survived)),
    "",
    "the in-place tick must agree with the referee",
  );
  assert.ok(
    !executed.some((sql) => sql.includes(`__support_next_${FLOW_REACH_TABLE}`)),
    "a walk under the cap must not fall back to the recompute",
  );
  assert.ok(
    executed.some((sql) => sql.includes(`DELETE FROM "${FLOW_REACH_TABLE}" WHERE`)),
    "expected the in-place walk to reach headDelete",
  );
  seam.db.close();
});
