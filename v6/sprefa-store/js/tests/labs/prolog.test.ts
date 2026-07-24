/**
 * prolog.test.ts -- the golden gate for prolog.ts: SLG (tabled, BFS) and SLD (DFS,
 * backtracking) cross-checked against fixpoint.ts's bottom-up datalog closure
 * (`closureDemand`), the from-scratch reference oracle for "what set of facts does
 * this query answer to". Style matches tests/lower.test.ts: node:test, tight asserts,
 * set-of-factKey comparisons (both engines and the oracle are order-independent).
 *
 * Five tests, each proving one distinct claim:
 *   1. SLG duality on an acyclic graph -- top-down tabled search === bottom-up closure.
 *   2. SLG terminates on a CYCLE (tabling is what makes that true).
 *   3. SLG's demand-driven search never touches a disconnected component the
 *      bottom-up closure computes anyway -- the top-down pruning advantage.
 *   4. SLD (real Prolog, DFS + backtracking, right-recursive so it terminates on an
 *      acyclic graph) finds the SAME answers as SLG/the oracle.
 *   5. SLD on the SAME cyclic graph SLG handled fine in test 2 does not terminate --
 *      it hits its step budget. The concrete DFS(stack)-vs-SLG(tabled queue) split.
 */

import { test } from "node:test";
import { strict as assert } from "node:assert";

import {
  type Atom,
  type Rule,
  type Fact,
  atom,
  vr,
  con,
  rule,
  fact,
  factKey,
  datalogWhile,
} from "../../src/labs/fixpoint.ts";
import {
  closureDemand,
  factKeySet,
  slgDeltas,
  queryAnswers,
  tabledSubgoals,
  sldAnswers,
} from "../../src/labs/prolog.ts";

// =============================================================================
// Programs
// =============================================================================

// LEFT-recursive transitive closure: path(X,Z) reads path(X,Y) first, then edge(Y,Z).
// SLG tables this fine (call-pattern tabling terminates regardless of recursion side);
// SLD (no tabling) would loop forever on ANY cyclic graph with this shape, which is
// exactly why the SLD tests below use the right-recursive form instead.
const leftRecursiveRules: Rule[] = [
  rule(atom("path", vr("X"), vr("Y")), atom("edge", vr("X"), vr("Y"))),
  rule(atom("path", vr("X"), vr("Z")), atom("path", vr("X"), vr("Y")), atom("edge", vr("Y"), vr("Z"))),
];

// RIGHT-recursive transitive closure: path(X,Z) reads edge(X,Y) first, then path(Y,Z).
// Chosen for the SLD tests so DFS terminates on an ACYCLIC graph (each recursive step
// consumes one edge before recursing, so depth is bounded by the graph's path length).
const rightRecursiveRules: Rule[] = [
  rule(atom("path", vr("X"), vr("Y")), atom("edge", vr("X"), vr("Y"))),
  rule(atom("path", vr("X"), vr("Z")), atom("edge", vr("X"), vr("Y")), atom("path", vr("Y"), vr("Z"))),
];

const edgeFacts = (pairs: readonly (readonly [string, string])[]): Fact[] =>
  pairs.map(([from, to]) => fact("edge", from, to));

// Acyclic graph: chain a->b->c->d, plus a branch b->e.
const acyclicEdges: readonly (readonly [string, string])[] = [
  ["a", "b"],
  ["b", "c"],
  ["c", "d"],
  ["b", "e"],
];

// Cyclic graph: a<->b.
const cyclicEdges: readonly (readonly [string, string])[] = [
  ["a", "b"],
  ["b", "a"],
];

const pathAY: Atom = atom("path", con("a"), vr("Y"));

// =============================================================================
// Test 1 -- SLG duality (acyclic): top-down tabled search === bottom-up closure.
// =============================================================================

test("SLG duality: acyclic path(a,Y) matches the bottom-up datalog closure", () => {
  const edb = edgeFacts(acyclicEdges);
  const deltas = slgDeltas(edb, leftRecursiveRules, pathAY);
  const slgAnswers = queryAnswers(deltas, pathAY);
  const oracle = closureDemand(edb, leftRecursiveRules, pathAY);

  const expected = factKeySet([
    fact("path", "a", "b"),
    fact("path", "a", "c"),
    fact("path", "a", "d"),
    fact("path", "a", "e"),
  ]);

  assert.deepStrictEqual(factKeySet(slgAnswers), expected, "SLG answer set !== expected");
  assert.deepStrictEqual(factKeySet(oracle), expected, "oracle set !== expected");
  assert.deepStrictEqual(factKeySet(slgAnswers), factKeySet(oracle), "SLG !== bottom-up closure");
});

// =============================================================================
// Test 2 -- SLG terminates on a cycle (tabling is what makes this possible).
// =============================================================================

test("SLG terminates on a cycle and matches the closure", () => {
  const edb = edgeFacts(cyclicEdges);
  const deltas = slgDeltas(edb, leftRecursiveRules, pathAY); // returns => it terminated
  const slgAnswers = queryAnswers(deltas, pathAY);
  const oracle = closureDemand(edb, leftRecursiveRules, pathAY);

  const expected = factKeySet([fact("path", "a", "b"), fact("path", "a", "a")]);

  assert.deepStrictEqual(factKeySet(slgAnswers), expected, "SLG answer set !== expected on the cycle");
  assert.deepStrictEqual(factKeySet(oracle), expected, "oracle set !== expected on the cycle");
});

// =============================================================================
// Test 3 -- SLG demand pruning: a disconnected component is never tabled, though
// the bottom-up closure computes it regardless (it has no query to prune against).
// =============================================================================

test("SLG never tables a subgoal in a disconnected component; datalogWhile computes it anyway", () => {
  const disconnectedEdges: readonly (readonly [string, string])[] = [...acyclicEdges, ["p", "q"], ["q", "r"]];
  const edb = edgeFacts(disconnectedEdges);

  const deltas = slgDeltas(edb, leftRecursiveRules, pathAY);
  const subgoals = tabledSubgoals(deltas);

  // Demand pruning: no subgoal key mentions the disconnected component's constants.
  // "c:p" / "c:q" / "c:r" is how subgoalKey renders a bound constant "p"/"q"/"r";
  // checking for the bare letters would false-positive on the "path" predicate name.
  for (const key of subgoals) {
    assert.ok(!key.includes("c:p"), `subgoal key ${key} mentions disconnected node p`);
    assert.ok(!key.includes("c:q"), `subgoal key ${key} mentions disconnected node q`);
    assert.ok(!key.includes("c:r"), `subgoal key ${key} mentions disconnected node r`);
  }
  assert.ok(subgoals.size > 0, "sanity: SLG did table some subgoals for the demanded query");

  // The bottom-up closure has no query to prune against: it computes path(p,q) etc regardless.
  const fullClosure = datalogWhile(edb, leftRecursiveRules);
  const fullClosureKeys = factKeySet(fullClosure);
  assert.ok(fullClosureKeys.has(factKey(fact("path", "p", "q"))), "datalogWhile should compute path(p,q) anyway");
  assert.ok(fullClosureKeys.has(factKey(fact("path", "q", "r"))), "datalogWhile should compute path(q,r) anyway");
  assert.ok(fullClosureKeys.has(factKey(fact("path", "p", "r"))), "datalogWhile should compute path(p,r) anyway");

  // Answers to the demanded query are unaffected by the extra disconnected component.
  const slgAnswers = queryAnswers(deltas, pathAY);
  const expected = factKeySet([
    fact("path", "a", "b"),
    fact("path", "a", "c"),
    fact("path", "a", "d"),
    fact("path", "a", "e"),
  ]);
  assert.deepStrictEqual(factKeySet(slgAnswers), expected, "demanded answers unaffected by the disconnected component");

  console.log(
    `[demand pruning] tabled ${subgoals.size} subgoal(s), 0 mentioning the disconnected component; ` +
      `datalogWhile's full closure has ${fullClosureKeys.size} facts (includes it)`,
  );
});

// =============================================================================
// Test 4 -- SLD (real Prolog: DFS + backtracking) finds the same answers as SLG /
// the oracle, on an acyclic graph with the right-recursive rule set.
// =============================================================================

test("SLD (right-recursive, acyclic) matches SLG and the oracle; uncapped", () => {
  const edb = edgeFacts(acyclicEdges);
  const { answers, capped } = sldAnswers(edb, rightRecursiveRules, pathAY, 1000);

  const expected = factKeySet([
    fact("path", "a", "b"),
    fact("path", "a", "c"),
    fact("path", "a", "d"),
    fact("path", "a", "e"),
  ]);

  assert.strictEqual(capped, false, "generous budget: SLD should complete without hitting it");
  assert.deepStrictEqual(factKeySet(answers), expected, "SLD answer set !== expected");

  // cross-check directly against SLG and the oracle computed the same way as test 1.
  const slgAnswers = queryAnswers(slgDeltas(edb, leftRecursiveRules, pathAY), pathAY);
  const oracle = closureDemand(edb, rightRecursiveRules, pathAY);
  assert.deepStrictEqual(factKeySet(answers), factKeySet(slgAnswers), "SLD !== SLG");
  assert.deepStrictEqual(factKeySet(answers), factKeySet(oracle), "SLD !== bottom-up closure");
});

// =============================================================================
// Test 5 -- SLD non-termination witness: the SAME cyclic graph SLG tabled fine in
// test 2 sends unbounded DFS around the a->b->a->b... branch until the budget trips.
// =============================================================================

test("SLD (right-recursive, cyclic) hits its step budget; SLG on the same graph does not", () => {
  const edb = edgeFacts(cyclicEdges);

  const { capped } = sldAnswers(edb, rightRecursiveRules, pathAY, 40);
  assert.strictEqual(capped, true, "DFS with no tabling should exhaust a small step budget on a cycle");

  // Re-confirm SLG's side of the contrast right here: same cyclic edges, terminates.
  const slgDone = slgDeltas(edb, leftRecursiveRules, pathAY); // must simply return
  assert.ok(slgDone.length > 0, "SLG terminated (returned a finite delta sequence) on the same cyclic graph");
});
