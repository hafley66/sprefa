import assert from "node:assert/strict";
import test from "node:test";
import { checksumSeen, graphOracle, validateRows } from "./1_postgres_reach.mjs";

test("graph oracle mirrors the one-layer fixture exactly", () => {
  const graph = graphOracle(1, 4);
  assert.deepEqual({
    nodes: graph.nodes,
    edges: graph.edges,
    before: graph.before.count,
    after: graph.after.count,
    killed: graph.before.count - graph.after.count,
  }, { nodes: 6, edges: 6, before: 6, after: 3, killed: 3 });
});

test("graph oracle propagates root-1 survivors through deeper layers", () => {
  const graph = graphOracle(3, 4);
  assert.deepEqual({
    nodes: graph.nodes,
    edges: graph.edges,
    before: graph.before.count,
    after: graph.after.count,
    killed: graph.before.count - graph.after.count,
  }, { nodes: 14, edges: 22, before: 14, after: 10, killed: 4 });
});

test("ordered row validation checks every survivor and its checksum", () => {
  const expected = graphOracle(2, 5).after;
  const rows = [];
  for (let node = 0; node < expected.seen.length; node += 1) {
    if (expected.seen[node]) rows.push({ node });
  }
  assert.equal(validateRows(rows, expected, "test"), checksumSeen(expected.seen));
  assert.throws(() => validateRows(rows.slice(1), expected, "test"), /count mismatch/);
  assert.throws(
    () => validateRows(rows.map((row, index) => index === 1 ? { node: 999 } : row), expected, "test"),
    /node 1/,
  );
});
