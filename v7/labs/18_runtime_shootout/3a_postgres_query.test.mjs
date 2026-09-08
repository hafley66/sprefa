import assert from "node:assert/strict";
import test from "node:test";
import { closureOracle, validateClosure } from "./3a_postgres_query.mjs";

test("chain oracle contains the exact strict upper triangle", () => {
  const oracle = closureOracle("chain", 4);
  assert.equal(oracle.edges, 3);
  assert.deepEqual(oracle.pairs, [
    [0, 1], [0, 2], [0, 3], [1, 2], [1, 3], [2, 3],
  ]);
});

test("ring oracle contains all ordered pairs including reflexive paths", () => {
  const oracle = closureOracle("ring", 3);
  assert.equal(oracle.edges, 3);
  assert.deepEqual(oracle.pairs, [
    [0, 0], [0, 1], [0, 2],
    [1, 0], [1, 1], [1, 2],
    [2, 0], [2, 1], [2, 2],
  ]);
});

test("closure validation rejects a wrong pair with the same cardinality", () => {
  const expected = closureOracle("chain", 3).pairs;
  const rows = expected.map(([source, target]) => ({ source, target }));
  assert.match(validateClosure(rows, expected), /^[0-9a-f]{64}$/);
  rows[1] = { source: 2, target: 0 };
  assert.throws(() => validateClosure(rows, expected), /closure pair 1/);
});
