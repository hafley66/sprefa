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
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import { firstValueFrom } from "rxjs";

import { program } from "../gen_emitted/flagship_flow_reach_over_batched_resolved_edges.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import type { IArrivalRow, ISqlSeam } from "../runtime/types.ts";

type Pair = readonly [string, string];

function callEdge(sign: "add" | "del", from: string, to: string): IArrivalRow {
  return { rel: "resolved_call_edge", sign, row: [from, "fn", to, "fn"] };
}

/** The referee: naive closure to fixpoint, no incrementality anywhere. */
function transitiveClosure(edges: readonly Pair[]): Set<string> {
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

async function headOf(seam: ISqlSeam): Promise<Set<string>> {
  const result = await firstValueFrom(
    seam.runner.execute(seam.db, program.finalSelect.flow_reach!),
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

async function openProgram(): Promise<ISqlSeam> {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, program.ddl));
  return seam;
}

test("scripted insert/delete ticks agree with the closure referee", async () => {
  const seam = await openProgram();
  const live: Pair[] = [];
  const script: { readonly name: string; readonly rows: readonly IArrivalRow[] }[] = [
    { name: "chain", rows: [callEdge("add", "a", "b"), callEdge("add", "b", "c"), callEdge("add", "c", "d")] },
    {
      name: "cycle behind one anchor",
      rows: [callEdge("add", "p", "q"), callEdge("add", "q", "r"), callEdge("add", "r", "s"), callEdge("add", "s", "q")],
    },
    { name: "cut the cycle's only anchor", rows: [callEdge("del", "p", "q")] },
    { name: "cut the chain mid-way", rows: [callEdge("del", "b", "c")] },
    { name: "put it back", rows: [callEdge("add", "b", "c")] },
    { name: "add and retract in one tick", rows: [callEdge("add", "z", "z"), callEdge("del", "z", "z")] },
    {
      name: "kill the cycle outright",
      rows: [callEdge("del", "q", "r"), callEdge("del", "r", "s"), callEdge("del", "s", "q")],
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
    const verdict = disagreement(await headOf(seam), transitiveClosure(live));
    assert.equal(verdict, "", `${step.name}: ${verdict}`);
  }
  seam.db.close();
});

test("randomized insert/delete ticks agree with the closure referee", async () => {
  const NODE_COUNT = 9;
  const TICKS = 40;
  for (const startSeed of [4, 5, 77, 303]) {
    let state = startSeed;
    const next = (): number => {
      state = (state * 1103515245 + 12345) & 0x7fffffff;
      return state / 0x7fffffff;
    };
    const seam = await openProgram();
    const live = new Set<string>();
    for (let tick = 0; tick < TICKS; tick += 1) {
      const rows: IArrivalRow[] = [];
      const batchSize = 1 + Math.floor(next() * 4);
      for (let index = 0; index < batchSize; index += 1) {
        if (live.size > 0 && next() < 0.45) {
          const keys = [...live];
          const key = keys[Math.floor(next() * keys.length)]!;
          const [from, to] = key.split("|");
          live.delete(key);
          rows.push(callEdge("del", from!, to!));
        } else {
          const from = `n${Math.floor(next() * NODE_COUNT)}`;
          const to = `n${Math.floor(next() * NODE_COUNT)}`;
          if (live.has(`${from}|${to}`)) continue;
          live.add(`${from}|${to}`);
          rows.push(callEdge("add", from, to));
        }
      }
      if (rows.length === 0) continue;
      await firstValueFrom(program.tick(seam, rows));
      const referee = transitiveClosure(
        [...live].map((key) => key.split("|") as unknown as Pair),
      );
      const verdict = disagreement(await headOf(seam), referee);
      assert.equal(verdict, "", `seed ${startSeed} tick ${tick}: ${verdict}`);
    }
    seam.db.close();
  }
});
