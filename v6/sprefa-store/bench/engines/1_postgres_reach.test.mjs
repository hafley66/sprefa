import assert from "node:assert/strict";
import test from "node:test";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { checksumSeen, graphFixture, graphOracle, validateRows } from "./1_postgres_reach.mjs";

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

test("actual SWI exact inputs and outputs match the shared DAG and cycle BFS", () => {
  const source = fileURLToPath(new URL("../swi_reach.pl", import.meta.url));
  for (const [layers, width, stride] of [[1, 1, 0], [3, 4, 0], [3, 4, 7], [8, 20, 7]]) {
    const goal = `use_module(library(http/json)),build(${layers},${width}),
      forall((between(${2+width},${1+layers*width},N),${stride}>0,N mod max(1,${stride})=:=0),
             (P is N-${width},assertz(edge(N,P)))),
      findall(P-C,edge(P,C),Es),msort(Es,Sorted),findall([P,C],member(P-C,Sorted),Edges),
      setof(N,alive(N),Before),retract(root(0)),setof(N,alive(N),After),
      json_write_dict(current_output,_{edges:Edges,before:Before,after:After}),halt`;
    const result = spawnSync("swipl", ["-q", "-l", source, "-g", goal], {
      encoding: "utf8", timeout: 10_000,
    });
    assert.equal(result.status, 0, result.stderr);
    const actual = JSON.parse(result.stdout);
    const expected = graphFixture(layers, width, stride);
    assert.deepEqual(actual, { edges: expected.edges, before: expected.before, after: expected.after });
  }
});
