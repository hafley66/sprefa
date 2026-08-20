/**
 * graph_lowering lab (v6/labs/graph_lowering/CONTRACT.md): graph algorithms written as
 * ast.ts programs, lowered by evalProgramSql, checked against spelled-out TypeScript
 * oracles. GRAPH_LOWERING_BENCH=1 also writes STANDINGS.md.
 */

import { test } from "node:test";
import { strict as assert } from "node:assert";
import { writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { firstValueFrom } from "rxjs";
import { createClient } from "@libsql/client";

import { derivedRel, edbRel, headAgg, headVar, lit, relRef, v, type Program } from "../../src/lower/ast.ts";
import { evalProgramSql } from "../../src/lower/lowerSql.ts";
import type { RelTable, RelTables } from "../../src/lower/types.ts";

type Row = readonly unknown[];
type Edge = readonly [number, number];
type Fixture = { name: string; nodeCount: number; edges: Edge[]; roots: number[] };

// ── programs ─────────────────────────────────────────────────────────────────

const reachProgram: Program = {
  rels: [edbRel("root", ["node"]), edbRel("edge", ["parent", "child"]), derivedRel("reach", ["node"])],
  rules: [
    { head: "reach", headTerms: [headVar("node")], body: [relRef("root", v("node"))] },
    { head: "reach", headTerms: [headVar("child")], body: [relRef("edge", v("parent"), v("child")), relRef("reach", v("parent"))] },
  ],
};

/** level(node, depth) is a SET of every depth a node is seen at; tier takes the max after the recursion. */
const tiersProgram: Program = {
  rels: [
    edbRel("root", ["node"]),
    edbRel("edge", ["parent", "child"]),
    edbRel("succ", ["depth", "nextDepth"]),
    derivedRel("level", ["node", "depth"]),
    derivedRel("tier", ["node", "depth"]),
  ],
  rules: [
    { head: "level", headTerms: [headVar("node"), headVar("zero")], body: [relRef("root", v("node")), relRef("succ", v("zero"), lit(1))] },
    {
      head: "level",
      headTerms: [headVar("child"), headVar("nextDepth")],
      body: [relRef("level", v("parent"), v("depth")), relRef("edge", v("parent"), v("child")), relRef("succ", v("depth"), v("nextDepth"))],
    },
    { head: "tier", headTerms: [headVar("node"), headAgg("max", "depth")], body: [relRef("level", v("node"), v("depth"))] },
  ],
};

/** label(node, candidate) is every node id reachable over undirected links; component takes the min. */
const componentsProgram: Program = {
  rels: [
    edbRel("node", ["id"]),
    edbRel("link", ["source", "target"]),
    derivedRel("label", ["node", "candidate"]),
    derivedRel("component", ["node", "label"]),
  ],
  rules: [
    { head: "label", headTerms: [headVar("id"), headVar("id")], body: [relRef("node", v("id"))] },
    { head: "label", headTerms: [headVar("target"), headVar("candidate")], body: [relRef("label", v("source"), v("candidate")), relRef("link", v("source"), v("target"))] },
    { head: "component", headTerms: [headVar("node"), headAgg("min", "candidate")], body: [relRef("label", v("node"), v("candidate"))] },
  ],
};

/** hop(source, node, depth) is every depth at which node is reached from source; distance takes the min. */
const distanceProgram: Program = {
  rels: [
    edbRel("source", ["node"]),
    edbRel("edge", ["parent", "child"]),
    edbRel("succ", ["depth", "nextDepth"]),
    derivedRel("hop", ["source", "node", "depth"]),
    derivedRel("distance", ["source", "node", "depth"]),
  ],
  rules: [
    { head: "hop", headTerms: [headVar("node"), headVar("node"), headVar("zero")], body: [relRef("source", v("node")), relRef("succ", v("zero"), lit(1))] },
    {
      head: "hop",
      headTerms: [headVar("source"), headVar("child"), headVar("nextDepth")],
      body: [relRef("hop", v("source"), v("parent"), v("depth")), relRef("edge", v("parent"), v("child")), relRef("succ", v("depth"), v("nextDepth"))],
    },
    { head: "distance", headTerms: [headVar("source"), headVar("node"), headAgg("min", "depth")], body: [relRef("hop", v("source"), v("node"), v("depth"))] },
  ],
};

const trianglesProgram: Program = {
  rels: [edbRel("link", ["source", "target"]), derivedRel("triangle", ["a", "b", "c"]), derivedRel("triangles", ["count"])],
  rules: [
    { head: "triangle", headTerms: [headVar("a"), headVar("b"), headVar("c")], body: [relRef("link", v("a"), v("b")), relRef("link", v("b"), v("c")), relRef("link", v("c"), v("a"))] },
    { head: "triangles", headTerms: [headAgg("count", "a")], body: [relRef("triangle", v("a"), v("b"), v("c"))] },
  ],
};

// ── fixtures ─────────────────────────────────────────────────────────────────

function chain(n: number): Fixture {
  const edges: Edge[] = [];
  for (let i = 1; i < n; i++) edges.push([i - 1, i]);
  return { name: `chain-${n}`, nodeCount: n, edges, roots: [0] };
}

/** w x h grid, edges right and down, so it is a DAG with longest path w + h - 2 from node 0. */
function grid(w: number, h: number): Fixture {
  const edges: Edge[] = [];
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++) {
      const i = y * w + x;
      if (x + 1 < w) edges.push([i, i + 1]);
      if (y + 1 < h) edges.push([i, i + w]);
    }
  return { name: `grid-${w}x${h}`, nodeCount: w * h, edges, roots: [0] };
}

/** two disjoint grids plus one triangle, for components and triangle counting. */
function twoGridsWithTriangle(w: number, h: number): Fixture {
  const a = grid(w, h);
  const offset = a.nodeCount;
  const edges: Edge[] = [...a.edges, ...a.edges.map(([p, c]) => [p + offset, c + offset] as const)];
  const t = offset * 2;
  edges.push([t, t + 1], [t + 1, t + 2], [t + 2, t]);
  return { name: `two-grids-${w}x${h}-plus-triangle`, nodeCount: t + 3, edges, roots: [0, offset] };
}

// ── oracles, spelled out ─────────────────────────────────────────────────────

function breadthFirstDepths(fixture: Fixture, sources: number[]): Map<number, number> {
  const out = new Map<number, Map<number, number>>();
  const adjacency = new Map<number, number[]>();
  for (const [p, c] of fixture.edges) (adjacency.get(p) ?? adjacency.set(p, []).get(p)!).push(c);
  const depths = new Map<number, number>();
  for (const s of sources) {
    const queue = [s];
    const seen = new Map<number, number>([[s, 0]]);
    while (queue.length) {
      const x = queue.shift()!;
      for (const y of adjacency.get(x) ?? []) {
        if (!seen.has(y)) {
          seen.set(y, seen.get(x)! + 1);
          queue.push(y);
        }
      }
    }
    out.set(s, seen);
    for (const [node, d] of seen) depths.set(node, Math.min(depths.get(node) ?? Infinity, d));
  }
  return depths;
}

/** longest path from any root on a DAG, by topological relaxation. */
function longestPathTiers(fixture: Fixture): Map<number, number> {
  const indegree = new Map<number, number>();
  const adjacency = new Map<number, number[]>();
  for (let i = 0; i < fixture.nodeCount; i++) indegree.set(i, 0);
  for (const [p, c] of fixture.edges) {
    (adjacency.get(p) ?? adjacency.set(p, []).get(p)!).push(c);
    indegree.set(c, (indegree.get(c) ?? 0) + 1);
  }
  const tier = new Map<number, number>();
  const queue: number[] = [];
  for (const [node, degree] of indegree) if (degree === 0) queue.push(node);
  for (const r of fixture.roots) tier.set(r, 0);
  while (queue.length) {
    const x = queue.shift()!;
    for (const y of adjacency.get(x) ?? []) {
      if (tier.has(x)) tier.set(y, Math.max(tier.get(y) ?? -1, tier.get(x)! + 1));
      indegree.set(y, indegree.get(y)! - 1);
      if (indegree.get(y) === 0) queue.push(y);
    }
  }
  return tier;
}

function unionFindComponents(fixture: Fixture): Map<number, number> {
  const parent = Array.from({ length: fixture.nodeCount }, (_, i) => i);
  const find = (x: number): number => (parent[x] === x ? x : (parent[x] = find(parent[x])));
  for (const [p, c] of fixture.edges) {
    const a = find(p);
    const b = find(c);
    if (a !== b) parent[Math.max(a, b)] = Math.min(a, b);
  }
  const out = new Map<number, number>();
  for (let i = 0; i < fixture.nodeCount; i++) out.set(i, find(i));
  return out;
}

function triangleCountTimesSix(fixture: Fixture): number {
  const neighbours = new Map<number, Set<number>>();
  const add = (a: number, b: number) => (neighbours.get(a) ?? neighbours.set(a, new Set()).get(a)!).add(b);
  for (const [p, c] of fixture.edges) {
    add(p, c);
    add(c, p);
  }
  let count = 0;
  for (const [a, nbrs] of neighbours) for (const b of nbrs) for (const c of neighbours.get(b) ?? []) if (neighbours.get(c)?.has(a)) count++;
  return count;
}

// ── evaluator harness ────────────────────────────────────────────────────────

type RunResult = { rows: Map<string, Row[]>; statements: number; ms: number };

async function runSql(program: Program, edb: Record<string, Row[]>): Promise<RunResult> {
  const db = createClient({ url: ":memory:" });
  try {
    const tables: Map<string, RelTable> = new Map();
    for (const decl of program.rels) {
      tables.set(decl.name, { table: `t_${decl.name}`, columns: decl.columns });
      await db.executeMultiple(`CREATE TABLE t_${decl.name}(${decl.columns.join(", ")}, PRIMARY KEY (${decl.columns.join(", ")})) WITHOUT ROWID`);
    }
    for (const [relName, rows] of Object.entries(edb)) {
      const t = tables.get(relName)!;
      const chunks: string[] = [];
      for (const row of rows) chunks.push(`(${row.map((x) => (typeof x === "number" ? String(x) : `'${String(x)}'`)).join(", ")})`);
      for (let i = 0; i < chunks.length; i += 500) {
        await db.executeMultiple(`INSERT OR IGNORE INTO ${t.table}(${t.columns.join(", ")}) VALUES ${chunks.slice(i, i + 500).join(", ")}`);
      }
    }
    let statements = 0;
    const started = performance.now();
    await firstValueFrom(evalProgramSql(db, program, tables as RelTables, undefined, () => statements++));
    const ms = performance.now() - started;
    const rows = new Map<string, Row[]>();
    for (const decl of program.rels) {
      if (decl.origin !== "IDB") continue;
      const result = await db.execute(`SELECT ${decl.columns.join(", ")} FROM t_${decl.name}`);
      rows.set(decl.name, result.rows.map((r) => decl.columns.map((c) => Number(r[c]))));
    }
    return { rows, statements, ms };
  } finally {
    db.close();
  }
}

const succRows = (n: number): Row[] => Array.from({ length: n }, (_, d) => [d, d + 1]);
const edgeRows = (f: Fixture): Row[] => f.edges.map(([p, c]) => [p, c]);
const linkRows = (f: Fixture): Row[] => f.edges.flatMap(([p, c]) => [[p, c], [c, p]]);
const nodeRows = (f: Fixture): Row[] => Array.from({ length: f.nodeCount }, (_, i) => [i]);
const asMap = (rows: Row[]): Map<number, number> => new Map(rows.map((r) => [r[0] as number, r[1] as number]));

// ── oracle tests ─────────────────────────────────────────────────────────────

test("graph_lowering: reach === breadth-first reachable set", async () => {
  const f = grid(6, 5);
  const { rows } = await runSql(reachProgram, { root: [[0]], edge: edgeRows(f) });
  const reached = new Set(rows.get("reach")!.map((r) => r[0]));
  assert.deepEqual([...reached].sort((a, b) => (a as number) - (b as number)), [...breadthFirstDepths(f, [0]).keys()].sort((a, b) => a - b));
});

test("graph_lowering: tiers via succ + post-recursion max === longest path on a DAG", async () => {
  const f = grid(6, 5);
  const { rows } = await runSql(tiersProgram, { root: [[0]], edge: edgeRows(f), succ: succRows(f.nodeCount) });
  assert.deepEqual(asMap(rows.get("tier")!), longestPathTiers(f));
});

test("graph_lowering: components via label set + post-recursion min === union-find", async () => {
  const f = twoGridsWithTriangle(4, 3);
  const { rows } = await runSql(componentsProgram, { node: nodeRows(f), link: linkRows(f) });
  assert.deepEqual(asMap(rows.get("component")!), unionFindComponents(f));
});

test("graph_lowering: distance via hop set + post-recursion min === breadth-first depth", async () => {
  const f = grid(5, 4);
  const { rows } = await runSql(distanceProgram, { source: [[0]], edge: edgeRows(f), succ: succRows(f.nodeCount) });
  const got = new Map(rows.get("distance")!.map((r) => [r[1] as number, r[2] as number]));
  assert.deepEqual(got, breadthFirstDepths(f, [0]));
});

test("graph_lowering: triangle count over links === six times the triangle count", async () => {
  const f = twoGridsWithTriangle(3, 3);
  const { rows } = await runSql(trianglesProgram, { link: linkRows(f) });
  assert.equal(rows.get("triangles")![0]![0], triangleCountTimesSix(f));
  assert.equal(rows.get("triangles")![0]![0], 6);
});

// ── bench -> STANDINGS.md ────────────────────────────────────────────────────

test("graph_lowering: bench rows, statements, ms per program per fixture", { skip: process.env.GRAPH_LOWERING_BENCH !== "1" }, async () => {
  const fixtures = [chain(200), chain(1000), grid(16, 16), grid(32, 32), twoGridsWithTriangle(16, 16)];
  const lines = ["# graph_lowering STANDINGS", "", `measured ${new Date().toISOString()}, libsql :memory:, node ${process.version}`, "",
    "| program | fixture | nodes | edges | materialised rows | statements | ms |", "|---|---|---|---|---|---|---|"];
  for (const f of fixtures) {
    const runs: [string, Program, Record<string, Row[]>, string][] = [
      ["reach", reachProgram, { root: [[0]], edge: edgeRows(f) }, "reach"],
      ["tiers", tiersProgram, { root: [[0]], edge: edgeRows(f), succ: succRows(f.nodeCount) }, "level"],
      ["components", componentsProgram, { node: nodeRows(f), link: linkRows(f) }, "label"],
      ["distance", distanceProgram, { source: [[0]], edge: edgeRows(f), succ: succRows(f.nodeCount) }, "hop"],
      ["triangles", trianglesProgram, { link: linkRows(f) }, "triangle"],
    ];
    for (const [name, program, edb, materialisedRel] of runs) {
      const r = await runSql(program, edb);
      lines.push(`| ${name} | ${f.name} | ${f.nodeCount} | ${f.edges.length} | ${r.rows.get(materialisedRel)!.length} | ${r.statements} | ${r.ms.toFixed(0)} |`);
    }
  }
  lines.push("", "`materialised rows` is the set the post-recursion aggregate reads: level/hop carry every depth a node is seen at, label every candidate id. A monotone aggregate inside the recursion would keep one row per key.");
  writeFileSync(resolve(import.meta.dirname, "../../../../labs/graph_lowering/STANDINGS.md"), lines.join("\n") + "\n");
  assert.ok(lines.length > 8);
});
