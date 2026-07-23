/**
 * fixpoint.test.ts — the golden gate for the datalog fixpoint lab (src/fixpoint.ts).
 *
 * Proves the THREE expressions of the datalog least-fixpoint agree, against a fourth
 * INDEPENDENT reference (naive datalog: re-apply all rules over the full set to
 * quiescence — a different algorithm from the semi-naive rounds ways 1/2/2b run, and
 * from the SQL delta loop way 3 runs). Then it measures the differences the three-way
 * contrast exists to expose: delta-disjointness (set-vs-element), and peak RSS.
 *
 * Sections:
 *   1. reference — naive datalog closure (the from-scratch oracle).
 *   2. agreement — while === expand === expandAsync === sql === naive (path facts).
 *   3. set-vs-element — expand's emitted batches are DISJOINT deltas, union = closure.
 *   4. cycle — all ways terminate + agree on a cyclic graph.
 *   5. measure — peak RSS per way on a denser graph (logged; asserted under budget).
 *
 * Pure; rxjs + better-sqlite3 only. Style matches tests/lower.test.ts / golden.test.ts.
 */

import { test } from "node:test";
import { strict as assert } from "node:assert";
import { firstValueFrom, toArray } from "rxjs";

import {
  type Fact,
  type Rule,
  type Value,
  atom,
  vr,
  con,
  rule,
  fact,
  factKey,
  matchFact,
  instantiate,
  datalogWhile,
  datalogExpand,
  datalogExpandAsync,
  datalogExpandClosure,
  datalogExpandDeltas,
  datalogSqlClosure,
} from "../src/fixpoint.ts";
import { memcap } from "../src/measure.ts";

// =============================================================================
// Helpers
// =============================================================================

/** The two-rule transitive closure program (LEFT-recursive, semi-naive friendly). */
const TC_RULES: Rule[] = [
  rule(atom("path", vr("X"), vr("Y")), atom("edge", vr("X"), vr("Y"))),
  rule(atom("path", vr("X"), vr("Z")), atom("path", vr("X"), vr("Y")), atom("edge", vr("Y"), vr("Z"))),
];

function edgeFacts(edges: readonly (readonly [Value, Value])[]): Fact[] {
  return edges.map(([x, y]) => fact("edge", x, y));
}

/** A stable set of factKeys, filtered to one predicate (path). */
function pathKeySet(facts: readonly Fact[]): Set<string> {
  const out = new Set<string>();
  for (const f of facts) if (f.pred === "path") out.add(factKey(f));
  return out;
}

function assertSameSet(a: Set<string>, b: Set<string>, msg: string): void {
  assert.deepEqual([...a].sort(), [...b].sort(), msg);
}

// =============================================================================
// 1. Reference oracle — NAIVE datalog (independent algorithm).
// =============================================================================
// Repeatedly apply EVERY rule over the ENTIRE current fact set until a full pass adds
// nothing. No delta, no semi-naive: the textbook naive T_P^omega. Different code path
// from every way under test, so agreement is a real cross-check.

function naiveDatalog(edb: readonly Fact[], rules: readonly Rule[]): Fact[] {
  const all = new Map<string, Fact>();
  for (const f of edb) all.set(factKey(f), f);
  let changed = true;
  while (changed) {
    changed = false;
    const byPred = new Map<string, Fact[]>();
    for (const f of all.values()) {
      const list = byPred.get(f.pred);
      if (list) list.push(f);
      else byPred.set(f.pred, [f]);
    }
    for (const r of rules) {
      let substs = [new Map<string, Value>()] as Map<string, Value>[];
      for (const bodyAtom of r.body) {
        const src = byPred.get(bodyAtom.pred) ?? [];
        const nextS: Map<string, Value>[] = [];
        for (const s of substs) {
          for (const groundFact of src) {
            const m = matchFact(bodyAtom, groundFact, s);
            if (m) nextS.push(m as Map<string, Value>);
          }
        }
        substs = nextS;
      }
      for (const s of substs) {
        const derived = instantiate(r.head, s);
        const k = factKey(derived);
        if (!all.has(k)) {
          all.set(k, derived);
          changed = true;
        }
      }
    }
  }
  return [...all.values()];
}

// =============================================================================
// 2. Agreement — all four ways === the naive reference (path facts).
// =============================================================================

test("agreement: while / expand / expandAsync / sql === naive reference", async () => {
  // a->b->c->d, with a branch b->e and a fork c->b (creates diamonds, not a cycle).
  const edges: [Value, Value][] = [
    ["a", "b"],
    ["b", "c"],
    ["c", "d"],
    ["b", "e"],
    ["a", "e"],
  ];
  const edb = edgeFacts(edges);

  const reference = pathKeySet(naiveDatalog(edb, TC_RULES));

  assertSameSet(pathKeySet(datalogWhile(edb, TC_RULES)), reference, "while vs naive");
  assertSameSet(pathKeySet(datalogExpandClosure(edb, TC_RULES)), reference, "expand vs naive");
  assertSameSet(pathKeySet(datalogSqlClosure(edges)), reference, "sql vs naive");

  // expandAsync: async hop → collect via toArray, flatten, filter path.
  const asyncBatches = await firstValueFrom(datalogExpandAsync(edb, TC_RULES).pipe(toArray()));
  assertSameSet(pathKeySet(asyncBatches.flat()), reference, "expandAsync vs naive");

  // Sanity: the closure is non-trivial and includes a two-hop and three-hop fact.
  assert.ok(reference.has(factKey(fact("path", "a", "c"))), "two-hop a->c present");
  assert.ok(reference.has(factKey(fact("path", "a", "d"))), "three-hop a->d present");
});

// =============================================================================
// 3. Set-vs-element — the emitted batches are DISJOINT deltas; union = closure.
// =============================================================================
// This is the resolved fork: a fixpoint's stream grain is the DELTA, never the whole
// re-emitted set. If ways emitted the full set each round (SET semantics), batches would
// OVERLAP and grow monotonically. They do not: each batch is exactly the round's NEW
// facts, and their disjoint union is the closure. So recursive rels lower delta/element.

test("set-vs-element: expand emits disjoint deltas whose union is the closure", () => {
  const edges: [Value, Value][] = [
    ["a", "b"],
    ["b", "c"],
    ["c", "d"],
    ["d", "e"],
  ];
  const edb = edgeFacts(edges);

  const batches = datalogExpandDeltas(edb, TC_RULES);

  // Every batch disjoint from the union of the prior batches.
  const seen = new Set<string>();
  for (let i = 0; i < batches.length; i++) {
    for (const f of batches[i]!) {
      const k = factKey(f);
      assert.ok(!seen.has(k), `batch ${i} re-emits already-seen ${k} (would be SET semantics)`);
      seen.add(k);
    }
  }

  // Union of the deltas === the closure (as computed by the while form).
  assertSameSet(
    new Set([...seen].filter((k) => k.startsWith("path("))),
    pathKeySet(datalogWhile(edb, TC_RULES)),
    "union of deltas vs while closure",
  );

  // Batch 0 is the EDB seed; later batches are the fixpoint rounds. A 5-node chain needs
  // several rounds, so there is genuine delta structure (not one big batch).
  assert.ok(batches.length >= 3, `expected multiple delta rounds, got ${batches.length}`);
});

// =============================================================================
// 4. Cycle — every way terminates and agrees on a cyclic graph.
// =============================================================================

test("cycle: all ways terminate and agree (a<->b)", async () => {
  const edges: [Value, Value][] = [
    ["a", "b"],
    ["b", "a"],
  ];
  const edb = edgeFacts(edges);

  // closure of {a->b, b->a} under TC = {a->b, b->a, a->a, b->b}.
  const reference = pathKeySet(naiveDatalog(edb, TC_RULES));
  assert.equal(reference.size, 4, "cyclic closure has 4 path facts");

  assertSameSet(pathKeySet(datalogWhile(edb, TC_RULES)), reference, "while (cycle)");
  assertSameSet(pathKeySet(datalogExpandClosure(edb, TC_RULES)), reference, "expand (cycle)");
  assertSameSet(pathKeySet(datalogSqlClosure(edges)), reference, "sql (cycle)");

  const asyncBatches = await firstValueFrom(datalogExpandAsync(edb, TC_RULES).pipe(toArray()));
  assertSameSet(pathKeySet(asyncBatches.flat()), reference, "expandAsync (cycle)");
});

// =============================================================================
// 5. Measure — peak RSS per way on a denser graph. Logged; asserted under budget.
// =============================================================================
// A layered DAG: L layers of W nodes, fully connected layer-to-layer. Path count grows
// ~ (W^2) * (L choose 2), so the closure is dense enough to move RSS. The contrast the
// numbers expose: ways 1/2 hold the whole closure in JS heap; way 3 holds it in SQLite
// and returns only the final read. (The dramatic divergence needs a much bigger graph;
// this is the in-budget smoke that the delegate path is real.)

function layeredEdges(layers: number, width: number): [Value, Value][] {
  const edges: [Value, Value][] = [];
  for (let l = 0; l < layers - 1; l++) {
    for (let i = 0; i < width; i++) {
      for (let j = 0; j < width; j++) {
        edges.push([`n${l}_${i}`, `n${l + 1}_${j}`]);
      }
    }
  }
  return edges;
}

test("measure: peak RSS per way (logged, under budget)", () => {
  const edges = layeredEdges(6, 6); // 5*36 = 180 edges; a few thousand path facts
  const edb = edgeFacts(edges);
  const BUDGET_MB = 512;

  const measure = (label: string, run: () => Fact[]): number => {
    memcap.reset_peak();
    const out = run();
    const peakKb = memcap.peak_rss_kb();
    // eslint-disable-next-line no-console
    console.log(`  [measure] ${label.padEnd(16)} facts=${out.length} peakRSS=${(peakKb / 1024).toFixed(1)}MiB`);
    assert.ok(peakKb / 1024 < BUDGET_MB, `${label} peak RSS over ${BUDGET_MB}MiB`);
    return out.length;
  };

  const whileN = measure("while", () => datalogWhile(edb, TC_RULES));
  const expandN = measure("expand", () => datalogExpandClosure(edb, TC_RULES));
  const sqlN = measure("sql", () => datalogSqlClosure(edges));

  // Same closure size across ways (path facts only for sql; while/expand include edges).
  const pathCount = pathKeySet(datalogWhile(edb, TC_RULES)).size;
  assert.equal(sqlN, pathCount, "sql path-fact count matches while");
  assert.ok(whileN === expandN, "while and expand produce the same fact count");
});
