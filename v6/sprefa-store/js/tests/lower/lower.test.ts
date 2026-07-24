/**
 * lower.test.ts — the golden gate for the dl->rxjs lowering core (ast + rulegraph + lower).
 *
 * Style matches tests/golden.test.ts: node:test, from-scratch reference oracles, tight
 * assertions. Four sections:
 *   1. rulegraph — pure-graph SCC + stratify vs an INDEPENDENT transitive-closure
 *      reference (Tarjan vs Floyd-Warshal reachability — different algorithms), plus
 *      dl Program rule-graph cases (chain, diamond, cycle, self-loop).
 *   2. lowering — hand-built programs (2-fact join, 3-rel join, join+selection,
 *      latest-by-gen aggregation, multi-rule union, live update) injected with
 *      in-memory ReplaySubject sources; derived emissions === a from-scratch TS reference.
 *   3. recursive — lazy recursive strata lower via the in-stratum fixpoint (closure vs a
 *      Floyd-Warshall reference; mutual recursion; live update); a materialized member or
 *      an aggregate head still defers (RecursiveStratumDeferred).
 *   4. stratified negation (v5 parity) — anti-join vs from-scratch set-difference
 *      oracles, exercised against the actual v5 example shapes (anim-deck's
 *      leaf_round wildcard, arch-conformance's all-bound !allowed, an ax_ord-style
 *      "first only" rule); the non-stratifiable diagnostic (v5's exact message
 *      wording); and the stratify-order guarantee (the negated rel is complete
 *      before its reader runs).
 *
 * Pure; no SQLite; no impure effects. rxjs (ReplaySubject) is the only dependency.
 */

import { test } from "node:test";
import { strict as assert } from "node:assert";
import { ReplaySubject } from "rxjs";
import type { Observable } from "rxjs";

import {
  edbRel,
  derivedRel,
  relRef,
  notRel,
  v,
  lit,
  wild,
  compare,
  headVar,
  headAgg,
  type Program,
  type RelDecl,
} from "../../src/lower/ast.ts";
import { buildRuleGraph, scc, stratify, NonStratifiableError, type Graph } from "../../src/lower/rulegraph.ts";
import { lowerProgram, RecursiveStratumDeferred, type Row, type Sources } from "../../src/lower/lower.ts";

// =============================================================================
// Helpers
// =============================================================================

/** Subscribe, drain synchronous emissions, unsubscribe (no dangling hot handles). */
function collectSync<T>(obs: Observable<T>): T[] {
  const out: T[] = [];
  const sub = obs.subscribe((x) => out.push(x));
  sub.unsubscribe();
  return out;
}

/** A derived rel emits its current row set as ONE emission (Row[]). Take the latest. */
function currentRows(obs: Observable<Row[]>): Row[] {
  const emissions = collectSync(obs);
  return emissions[emissions.length - 1] ?? [];
}

/** Build a dense-indexed Graph from an explicit edge list (nodes = sorted endpoints). */
function graphFromEdges(edges: readonly (readonly [number, number])[]): Graph {
  const nodeset = new Set<number>();
  for (const [a, b] of edges) {
    nodeset.add(a);
    nodeset.add(b);
  }
  const sorted = [...nodeset].sort((a, b) => a - b);
  const idx = new Map<number, number>(sorted.map((n, i) => [n, i] as const));
  const n = sorted.length;
  const adjSets: Set<number>[] = Array.from({ length: n }, () => new Set<number>());
  for (const [a, b] of edges) adjSets[idx.get(a)!]!.add(idx.get(b)!);
  return {
    nodes: sorted.map((n) => String(n)),
    adj: adjSets.map((s) => [...s].sort((a, b) => a - b)),
  };
}

/** Sort rows by JSON for order-independent comparison (the lowerer emits sorted sets). */
function sortRows(rows: readonly Row[]): Row[] {
  return rows
    .map((r) => [...r])
    .sort((a, b) => (JSON.stringify(a) < JSON.stringify(b) ? -1 : 1));
}

// =============================================================================
// SECTION 1 — rulegraph: SCC + stratify vs an independent reference
// =============================================================================
// The reference is transitive-closure-based (Floyd-Warshall reachability + mutual
// reachability grouping) — a deliberately different algorithm from the iterative
// Tarjan in rulegraph.scc, so it cross-checks rather than paraphrases.
// =============================================================================
namespace ref_graph {
  /** SCC groups (dense indices) via mutual reachability. Independent of Tarjan. */
  export function sccGroups(adj: readonly (readonly number[])[]): number[][] {
    const n = adj.length;
    const reach: boolean[][] = Array.from({ length: n }, () => new Array<boolean>(n).fill(false));
    for (let u = 0; u < n; u++) {
      reach[u]![u] = true;
      for (const w of adj[u]!) reach[u]![w] = true;
    }
    for (let k = 0; k < n; k++) {
      for (let i = 0; i < n; i++) {
        if (reach[i]![k]) for (let j = 0; j < n; j++) if (reach[k]![j]) reach[i]![j] = true;
      }
    }
    const comp = new Array<number>(n).fill(-1);
    const groups: number[][] = [];
    for (let u = 0; u < n; u++) {
      if (comp[u] !== -1) continue;
      const g: number[] = [];
      for (let w = 0; w < n; w++) {
        if (reach[u]![w] && reach[w]![u]) {
          g.push(w);
          comp[w] = groups.length;
        }
      }
      groups.push(g);
    }
    return groups;
  }

  /** A group is recursive iff it has >1 member OR a self-loop on a member. */
  export function isRecursive(adj: readonly (readonly number[])[], group: readonly number[]): boolean {
    if (group.length > 1) return true;
    const node = group[0]!;
    return adj[node]!.includes(node);
  }
}

/** Convert my scc() output to a set of frozenset-like group signatures (sorted dense ids). */
function myGroups(comp: readonly number[], ncomp: number): string[] {
  const members: number[][] = Array.from({ length: ncomp }, () => []);
  for (let node = 0; node < comp.length; node++) members[comp[node]!]!.push(node);
  return members
    .map((m) => [...m].sort((a, b) => a - b).join(","))
    .sort();
}

// v5-style shapes (echoes tests/golden.test.ts SHAPES): chain, diamond, cycles, self-loop, figure8.
const SHAPES: [string, (readonly [number, number])[]][] = [
  ["chain5", [[0, 1], [1, 2], [2, 3], [3, 4]]],
  ["diamond", [[0, 1], [0, 2], [1, 3], [2, 3]]],
  ["cycle3", [[1, 2], [2, 3], [3, 1]]],
  ["cycle3+tail", [[0, 1], [1, 2], [2, 3], [3, 1], [2, 4]]],
  ["two_cycles", [[0, 1], [1, 0], [2, 3], [3, 2], [1, 2]]],
  ["self_loop", [[0, 0], [0, 1], [1, 2]]],
  ["figure8", [[0, 1], [1, 2], [2, 0], [2, 3], [3, 4], [4, 2]]],
];

test("rulegraph: scc partition agrees with the independent transitive-closure reference", () => {
  for (const [name, edges] of SHAPES) {
    const g = graphFromEdges(edges);
    const mine = scc(g);
    const ref = ref_graph.sccGroups(g.adj);
    const want = ref.map((grp) => grp.slice().sort((a, b) => a - b).join(",")).sort();
    assert.deepStrictEqual(
      myGroups(mine.comp, mine.ncomp),
      want,
      `${name}: SCC partition disagreed with the reference`,
    );
  }
});

test("rulegraph: stratify is dependency-first, exhaustive, and flags recursive strata", () => {
  for (const [name, edges] of SHAPES) {
    const g = graphFromEdges(edges);
    const mine = scc(g);
    const strata = stratify(g, mine);
    const ref = ref_graph.sccGroups(g.adj);

    // (a) one stratum per SCC, in count.
    assert.strictEqual(strata.length, ref.length, `${name}: stratum count !== SCC count`);

    // (b) each stratum is exactly one reference SCC (as a set of node names).
    const refByName = ref.map((grp) => new Set(grp.map((i) => g.nodes[i]!)));
    for (const s of strata) {
      const sset = new Set(s.rels);
      const matched = refByName.some((rs) => rs.size === sset.size && [...sset].every((x) => rs.has(x)));
      assert.ok(matched, `${name}: stratum {${[...sset].join(",")}} is not a reference SCC`);
    }

    // (c) recursive flag matches (size>1 or self-loop).
    for (const s of strata) {
      // remap name->dense via the graph's node list
      const dense = s.rels.map((nm) => g.nodes.indexOf(nm));
      const refGroup = ref.find((grp) => grp.length === dense.length && dense.every((d) => grp.includes(d)));
      assert.ok(refGroup, `${name}: could not map stratum back to a reference group`);
      assert.strictEqual(
        s.recursive,
        ref_graph.isRecursive(g.adj, refGroup),
        `${name}: recursive flag wrong for {${s.rels.join(",")}}`,
      );
    }

    // (d) topo validity: for every edge u->v, v's stratum order <= u's (deps never come after).
    const nameToOrder = new Map<string, number>();
    for (const s of strata) for (const nm of s.rels) nameToOrder.set(nm, s.order);
    for (let u = 0; u < g.adj.length; u++) {
      for (const w of g.adj[u]!) {
        const un = g.nodes[u]!;
        const wn = g.nodes[w]!;
        assert.ok(
          nameToOrder.get(wn)! <= nameToOrder.get(un)!,
          `${name}: edge ${un}->${wn} violates dependency-first order`,
        );
      }
    }
  }
});

test("rulegraph: a self-loop is one recursive singleton stratum", () => {
  // node 0 has a self-loop and an edge to 1; node 1 -> 2. Only {0} is recursive.
  const g = graphFromEdges([[0, 0], [0, 1], [1, 2]]);
  const strata = stratify(g, scc(g));
  const selfStratum = strata.find((s) => s.rels.includes("0"));
  assert.ok(selfStratum, "self-loop node 0 has a stratum");
  assert.strictEqual(selfStratum!.recursive, true, "a self-loop singleton is recursive");
  assert.strictEqual(selfStratum!.rels.length, 1, "self-loop SCC has exactly one member");
});

// ---- dl Program rule-graph cases (buildRuleGraph + stratify on hand-built programs) ----

test("rulegraph(dl): chain a<-b<-c stratifies deps-first [c, b, a]", () => {
  const prog: Program = {
    rels: [derivedRel("a", ["x"]), derivedRel("b", ["x"]), derivedRel("c", ["x"])],
    rules: [
      { head: "a", headTerms: [headVar("x")], body: [relRef("b", v("x"))] },
      { head: "b", headTerms: [headVar("x")], body: [relRef("c", v("x"))] },
    ],
  };
  const strata = stratify(buildRuleGraph(prog), scc(buildRuleGraph(prog)));
  assert.deepStrictEqual(
    strata.map((s) => s.rels[0]),
    ["c", "b", "a"],
  );
  assert.ok(strata.every((s) => !s.recursive), "no recursive strata in a chain");
});

test("rulegraph(dl): diamond stratifies [d, b, c, a]", () => {
  // a depends on b and c; b and c both depend on d.
  const prog: Program = {
    rels: [
      derivedRel("a", ["x"]),
      derivedRel("b", ["x"]),
      derivedRel("c", ["x"]),
      derivedRel("d", ["x"]),
    ],
    rules: [
      { head: "a", headTerms: [headVar("x")], body: [relRef("b", v("x")), relRef("c", v("x"))] },
      { head: "b", headTerms: [headVar("x")], body: [relRef("d", v("x"))] },
      { head: "c", headTerms: [headVar("x")], body: [relRef("d", v("x"))] },
    ],
  };
  const strata = stratify(buildRuleGraph(prog), scc(buildRuleGraph(prog)));
  assert.deepStrictEqual(
    strata.map((s) => s.rels[0]),
    ["d", "b", "c", "a"],
  );
});

// =============================================================================
// SECTION 2 — lowering: derived emissions === from-scratch TS reference
// =============================================================================

test("lower: 2-fact join (watch x label on ep) matches the reference", () => {
  const prog: Program = {
    rels: [
      edbRel("watch", ["ep"]),
      edbRel("label", ["ep", "name"]),
      derivedRel("watch_label", ["ep", "name"]),
    ],
    rules: [
      {
        head: "watch_label",
        headTerms: [headVar("ep"), headVar("name")],
        body: [relRef("watch", v("ep")), relRef("label", v("ep"), v("name"))],
      },
    ],
  };
  const watch = [["repos/cli/cli"], ["repos/x/y"]];
  const label = [
    ["repos/cli/cli", "CLI"],
    ["repos/x/y", "X"],
    ["repos/none", "None"],
  ];

  // from-scratch reference: nested-loop equi-join on ep.
  const expected: Row[] = [];
  for (const [ep] of watch) {
    for (const [lep, name] of label) {
      if (lep === ep) expected.push([ep, name]);
    }
  }

  const sources: Sources = new Map([
    ["watch", makeReplay(watch)],
    ["label", makeReplay(label)],
  ]);
  const { rels, deferred } = lowerProgram(prog, sources);
  assert.deepStrictEqual(deferred, [], "no recursive strata");
  const got = currentRows(rels.get("watch_label")!);
  assert.deepStrictEqual(got, sortRows(expected));
});

test("lower: 3-rel join (a x b x c) matches the reference", () => {
  const prog: Program = {
    rels: [
      edbRel("a", ["x"]),
      edbRel("b", ["x", "y"]),
      edbRel("c", ["y", "z"]),
      derivedRel("abc", ["x", "y", "z"]),
    ],
    rules: [
      {
        head: "abc",
        headTerms: [headVar("x"), headVar("y"), headVar("z")],
        body: [
          relRef("a", v("x")),
          relRef("b", v("x"), v("y")),
          relRef("c", v("y"), v("z")),
        ],
      },
    ],
  };
  const aRows: Row[] = [["x1"], ["x2"]];
  const bRows: Row[] = [
    ["x1", "y1"],
    ["x1", "y2"],
    ["x2", "y1"],
  ];
  const cRows: Row[] = [
    ["y1", "z1"],
    ["y2", "z2"],
  ];

  const expected: Row[] = [];
  for (const [x] of aRows) {
    for (const [bx, y] of bRows) {
      if (bx !== x) continue;
      for (const [cy, z] of cRows) {
        if (cy !== y) continue;
        expected.push([x, y, z]);
      }
    }
  }

  const sources: Sources = new Map([
    ["a", makeReplay(aRows)],
    ["b", makeReplay(bRows)],
    ["c", makeReplay(cRows)],
  ]);
  const { rels } = lowerProgram(prog, sources);
  const got = currentRows(rels.get("abc")!);
  assert.deepStrictEqual(got, sortRows(expected));
});

test("lower: join + selection (Compare predicates) drops non-matching rows", () => {
  // high(ep, b) <- resp(ep, b, status), status == 200 AND b >= 10.
  const prog: Program = {
    rels: [
      edbRel("resp", ["ep", "b", "status"]),
      derivedRel("high", ["ep", "b"]),
    ],
    rules: [
      {
        head: "high",
        headTerms: [headVar("ep"), headVar("b")],
        body: [
          relRef("resp", v("ep"), v("b"), v("status")),
          compare("eq", "status", 200),
          compare("ge", "b", 10),
        ],
      },
    ],
  };
  const resp: Row[] = [
    ["a", 5, 200],
    ["a", 20, 200],
    ["b", 10, 200],
    ["c", 3, 404],
    ["a", 99, 404],
  ];
  const expected: Row[] = [
    ["a", 20],
    ["b", 10],
  ];

  const sources: Sources = new Map([["resp", makeReplay(resp)]]);
  const { rels } = lowerProgram(prog, sources);
  const got = currentRows(rels.get("high")!);
  assert.deepStrictEqual(got, sortRows(expected));
});

test("lower: latest-by-gen aggregation max(b) group-by ep (literal 200 selection)", () => {
  // latest(ep, maxB) <- resp(ep, b, 200).   (the 200 is a literal arg selecting status)
  const prog: Program = {
    rels: [
      edbRel("resp", ["ep", "b", "status"]),
      derivedRel("latest", ["ep", "maxB"]),
    ],
    rules: [
      {
        head: "latest",
        headTerms: [headVar("ep"), headAgg("max", "b")],
        body: [relRef("resp", v("ep"), v("b"), lit(200))],
      },
    ],
  };
  const resp: Row[] = [
    ["a", 1, 200],
    ["a", 5, 200],
    ["a", 9, 200],
    ["b", 2, 200],
    ["a", 3, 404], // filtered: status != 200
    ["b", 7, 404], // filtered
  ];
  const expected: Row[] = [
    ["a", 9],
    ["b", 2],
  ];

  const sources: Sources = new Map([["resp", makeReplay(resp)]]);
  const { rels } = lowerProgram(prog, sources);
  const got = currentRows(rels.get("latest")!);
  assert.deepStrictEqual(got, sortRows(expected));
});

test("lower: min / sum / count aggregates", () => {
  const prog: Program = {
    rels: [edbRel("r", ["k", "n"]), derivedRel("agg", ["k", "mn", "sm", "cnt"])],
    rules: [
      {
        head: "agg",
        headTerms: [
          headVar("k"),
          headAgg("min", "n"),
          headAgg("sum", "n"),
          headAgg("count", "n"),
        ],
        body: [relRef("r", v("k"), v("n"))],
      },
    ],
  };
  const r: Row[] = [
    ["x", 3],
    ["x", 1],
    ["x", 2],
    ["y", 10],
  ];
  // group x: min 1, sum 6, count 3 ; group y: min 10, sum 10, count 1
  const expected: Row[] = [
    ["x", 1, 6, 3],
    ["y", 10, 10, 1],
  ];
  const sources: Sources = new Map([["r", makeReplay(r)]]);
  const { rels } = lowerProgram(prog, sources);
  const got = currentRows(rels.get("agg")!);
  assert.deepStrictEqual(got, sortRows(expected));
});

test("lower: multi-rule head UNIONs + dedups", () => {
  // reachable(x,y) <- a(x,y) ; reachable(x,y) <- b(x,y).  Two rules, one head = UNION.
  // (The AST's HeadTerm is Var|Agg only; a constant head column like gh-cache's
  //  poll(w,"") is a parser/head-literal concern, out of scope. This exercises the
  //  combineLatest-over-rules union + dedup path with what the AST expresses.)
  const prog: Program = {
    rels: [
      edbRel("a", ["x", "y"]),
      edbRel("b", ["x", "y"]),
      derivedRel("reachable", ["x", "y"]),
    ],
    rules: [
      {
        head: "reachable",
        headTerms: [headVar("x"), headVar("y")],
        body: [relRef("a", v("x"), v("y"))],
      },
      {
        head: "reachable",
        headTerms: [headVar("x"), headVar("y")],
        body: [relRef("b", v("x"), v("y"))],
      },
    ],
  };
  const a: Row[] = [
    [1, 2],
    [3, 4],
  ];
  const b: Row[] = [
    [3, 4],
    [5, 6],
  ];
  // union, deduped: (3,4) appears in both a and b but lands once.
  const expected: Row[] = [
    [1, 2],
    [3, 4],
    [5, 6],
  ];
  const sources: Sources = new Map([
    ["a", makeReplay(a)],
    ["b", makeReplay(b)],
  ]);
  const { rels } = lowerProgram(prog, sources);
  const got = currentRows(rels.get("reachable")!);
  assert.deepStrictEqual(got, sortRows(expected));
});

test("lower: live update — a re-emitted source re-derives downstream", () => {
  // watch_label <- watch, label. Prime, subscribe, then push a new label row.
  const prog: Program = {
    rels: [
      edbRel("watch", ["ep"]),
      edbRel("label", ["ep", "name"]),
      derivedRel("watch_label", ["ep", "name"]),
    ],
    rules: [
      {
        head: "watch_label",
        headTerms: [headVar("ep"), headVar("name")],
        body: [relRef("watch", v("ep")), relRef("label", v("ep"), v("name"))],
      },
    ],
  };
  const watch$ = new ReplaySubject<Row[]>(1);
  const label$ = new ReplaySubject<Row[]>(1);
  watch$.next([["a"]]);
  label$.next([["a", "first"]]);
  const sources: Sources = new Map([
    ["watch", watch$.asObservable()],
    ["label", label$.asObservable()],
  ]);
  const { rels } = lowerProgram(prog, sources);
  const out: Row[][] = [];
  const sub = rels.get("watch_label")!.subscribe((r) => out.push(r));
  // initial emission present
  assert.strictEqual(out.length, 1, "initial derivation emitted");
  assert.deepStrictEqual(out[0], [["a", "first"]]);
  // push a second label row -> re-derive
  label$.next([
    ["a", "first"],
    ["a", "second"],
  ]);
  sub.unsubscribe();
  assert.strictEqual(out.length, 2, "a source re-emission re-derived the rel");
  assert.deepStrictEqual(out[1], [
    ["a", "first"],
    ["a", "second"],
  ]);
});

test("lower: an EDB rel with no injected source lowers to empty", () => {
  const prog: Program = {
    rels: [edbRel("solo", ["x"]), derivedRel("echo", ["x"])],
    rules: [{ head: "echo", headTerms: [headVar("x")], body: [relRef("solo", v("x"))] }],
  };
  const sources: Sources = new Map(); // no source for 'solo'
  const { rels } = lowerProgram(prog, sources);
  assert.deepStrictEqual(collectSync(rels.get("solo")!), [[]], "EDB with no source is empty");
  assert.deepStrictEqual(collectSync(rels.get("echo")!), [[]], "derived over empty source is empty");
});

// =============================================================================
// SECTION 3 — recursive strata: the in-stratum fixpoint backend + the defer paths
// =============================================================================

/** From-scratch reference: transitive closure (length >= 1) via Floyd-Warshall — a
 *  deliberately different algorithm from the lowerer's naive bottom-up fixpoint. */
function refClosure(edges: readonly (readonly [string, string])[]): Row[] {
  const nodes = [...new Set(edges.flat())];
  const idx = new Map(nodes.map((n, i) => [n, i] as const));
  const n = nodes.length;
  const reach: boolean[][] = Array.from({ length: n }, () => new Array<boolean>(n).fill(false));
  for (const [a, b] of edges) reach[idx.get(a)!]![idx.get(b)!] = true;
  for (let k = 0; k < n; k++) {
    for (let i = 0; i < n; i++) {
      if (reach[i]![k]) for (let j = 0; j < n; j++) if (reach[k]![j]) reach[i]![j] = true;
    }
  }
  const out: Row[] = [];
  for (let i = 0; i < n; i++) {
    for (let j = 0; j < n; j++) if (reach[i]![j]) out.push([nodes[i]!, nodes[j]!]);
  }
  return out;
}

const PATH_RULES: Program["rules"] = [
  {
    head: "path",
    headTerms: [headVar("a"), headVar("b")],
    body: [relRef("edge", v("a"), v("b"))],
  },
  {
    head: "path",
    headTerms: [headVar("a"), headVar("b")],
    body: [relRef("edge", v("a"), v("c")), relRef("path", v("c"), v("b"))],
  },
];

test("recursive: transitive closure over a cycle matches the Floyd-Warshall reference", () => {
  // path(a,b) <- edge(a,b). ; path(a,b) <- edge(a,c), path(c,b).  Cycle 0->1->2->0 + 2->3:
  // the fixpoint must converge (dedup), and the closure includes the (x,x) self-pairs.
  const prog: Program = {
    rels: [edbRel("edge", ["a", "b"]), derivedRel("path", ["a", "b"])],
    rules: PATH_RULES,
  };
  const edges: (readonly [string, string])[] = [
    ["0", "1"],
    ["1", "2"],
    ["2", "0"],
    ["2", "3"],
  ];
  const sources: Sources = new Map([["edge", makeReplay(edges.map((e) => [...e]))]]);
  const { rels, deferred } = lowerProgram(prog, sources);
  assert.deepStrictEqual(deferred, [], "lazy members, no aggregates: nothing deferred");
  const got = currentRows(rels.get("path")!);
  assert.deepStrictEqual(got, sortRows(refClosure(edges)));
});

test("recursive: mutual recursion (even/odd over succ) converges with a base rule", () => {
  // even(n) <- zero(n). ; even(n) <- succ(m,n), odd(m). ; odd(n) <- succ(m,n), even(m).
  const prog: Program = {
    rels: [
      edbRel("zero", ["n"]),
      edbRel("succ", ["m", "n"]),
      derivedRel("even", ["n"]),
      derivedRel("odd", ["n"]),
    ],
    rules: [
      { head: "even", headTerms: [headVar("n")], body: [relRef("zero", v("n"))] },
      {
        head: "even",
        headTerms: [headVar("n")],
        body: [relRef("succ", v("m"), v("n")), relRef("odd", v("m"))],
      },
      {
        head: "odd",
        headTerms: [headVar("n")],
        body: [relRef("succ", v("m"), v("n")), relRef("even", v("m"))],
      },
    ],
  };
  const sources: Sources = new Map([
    ["zero", makeReplay([[0]])],
    ["succ", makeReplay([[0, 1], [1, 2], [2, 3], [3, 4]])],
  ]);
  const { rels, deferred } = lowerProgram(prog, sources);
  assert.deepStrictEqual(deferred, []);
  assert.deepStrictEqual(currentRows(rels.get("even")!), sortRows([[0], [2], [4]]));
  assert.deepStrictEqual(currentRows(rels.get("odd")!), sortRows([[1], [3]]));
});

test("recursive: a re-emitted edge source re-runs the fixpoint", () => {
  const prog: Program = {
    rels: [edbRel("edge", ["a", "b"]), derivedRel("path", ["a", "b"])],
    rules: PATH_RULES,
  };
  const edge$ = new ReplaySubject<Row[]>(1);
  edge$.next([["a", "b"]]);
  const sources: Sources = new Map([["edge", edge$.asObservable()]]);
  const { rels } = lowerProgram(prog, sources);
  const out: Row[][] = [];
  const sub = rels.get("path")!.subscribe((r) => out.push(r));
  assert.deepStrictEqual(out[0], [["a", "b"]], "initial fixpoint emitted");
  edge$.next([
    ["a", "b"],
    ["b", "c"],
  ]);
  sub.unsubscribe();
  assert.strictEqual(out.length, 2, "edge re-emission re-ran the fixpoint");
  assert.deepStrictEqual(
    out[1],
    sortRows([
      ["a", "b"],
      ["b", "c"],
      ["a", "c"],
    ]),
  );
});

test("recursive: a materialized member defers the stratum (the cascade-delegate seam)", () => {
  // Same closure program, but path is materialized: the heavy path belongs to the SQLite
  // cascade backend (engine wiring), so the in-memory backend declines it.
  const materializedPath: RelDecl = {
    name: "path",
    columns: ["a", "b"],
    kind: {
      shape: "pipe",
      temperature: "cold",
      buffer: { replay: 0, onFull: "block" },
      origin: "IDB",
      materialization: "materialized",
    },
    origin: "IDB",
  };
  const prog: Program = {
    rels: [edbRel("edge", ["a", "b"]), materializedPath],
    rules: PATH_RULES,
  };
  const sources: Sources = new Map([["edge", makeReplay([["0", "1"]])]]);
  const { rels, deferred } = lowerProgram(prog, sources);
  assert.strictEqual(deferred.length, 1, "materialized recursive member defers");
  assert.ok(deferred[0]! instanceof RecursiveStratumDeferred, "marker is the typed class");
  assert.deepStrictEqual([...deferred[0]!.stratum.rels], ["path"]);
  assert.ok(!rels.has("path"), "materialized path not lowered");
  assert.ok(rels.has("edge"), "the acyclic EDB edge still lowers");
  assert.ok(/path/.test(deferred[0]!.message), "the marker names the rels");
});

test("recursive: an aggregate head inside a cycle defers the stratum", () => {
  // acc(k, max(n)) <- acc(k, n).  Aggregation under recursion is non-monotone: declined.
  const prog: Program = {
    rels: [derivedRel("acc", ["k", "n"])],
    rules: [
      {
        head: "acc",
        headTerms: [headVar("k"), headAgg("max", "n")],
        body: [relRef("acc", v("k"), v("n"))],
      },
    ],
  };
  const { rels, deferred } = lowerProgram(prog, new Map());
  assert.strictEqual(deferred.length, 1, "aggregate-in-cycle defers");
  assert.ok(deferred[0]! instanceof RecursiveStratumDeferred);
  assert.ok(!rels.has("acc"));
});

// =============================================================================
// SECTION 4 — E4 language completion: negation, @next, @async.
// =============================================================================

test("negation: missing(x) <- all(x), !seen(x) matches a from-scratch set difference", () => {
  const prog: Program = {
    rels: [
      edbRel("all", ["x"]),
      edbRel("seen", ["x"]),
      derivedRel("missing", ["x"]),
    ],
    rules: [
      {
        head: "missing",
        headTerms: [headVar("x")],
        body: [relRef("all", v("x")), notRel("seen", v("x"))],
      },
    ],
  };
  const allRows: Row[] = [["a"], ["b"], ["c"], ["d"]];
  const seenRows: Row[] = [["b"], ["d"]];
  const seenSet = new Set(seenRows.map(([x]) => x));
  const expected: Row[] = allRows.filter(([x]) => !seenSet.has(x)); // {a, c}: from-scratch set difference

  const sources: Sources = new Map([
    ["all", makeReplay(allRows)],
    ["seen", makeReplay(seenRows)],
  ]);
  const { rels, deferred } = lowerProgram(prog, sources);
  assert.deepStrictEqual(deferred, []);
  assert.deepStrictEqual(currentRows(rels.get("missing")!), sortRows(expected));
});

// ---- (a) the v5 anim-deck leaf_round shape: a negated atom with a wildcard --------
// examples/anim-deck.dl:41-49 —
//   rel round(f: text, r: int).       round(f, 0) <- fan_out(f, n), n >= 6.  (etc.)
//   rel tgt_round(t: text, r: int).   tgt_round(t, min(r)) <- round(f, r), edge0(f, t).
//   rel leaf_round(t: text, r: int).  leaf_round(t, r) <- tgt_round(t, r), !round(t, _).
// `!round(t, _)`: the wildcard means "does ANY row of round have this t", regardless
// of its r — exactly the existential-quantification `Wild` gives inside a NegRelRef.

test("negation (v5 a): leaf_round(t, r) <- tgt_round(t, r), !round(t, _) — wildcard in a negated atom", () => {
  const prog: Program = {
    rels: [
      edbRel("round", ["f", "r"]),
      edbRel("tgt_round", ["t", "r"]),
      derivedRel("leaf_round", ["t", "r"]),
    ],
    rules: [
      {
        head: "leaf_round",
        headTerms: [headVar("t"), headVar("r")],
        body: [relRef("tgt_round", v("t"), v("r")), notRel("round", v("t"), wild())],
      },
    ],
  };
  const round: Row[] = [
    ["hub1", 0],
    ["hub2", 1],
  ];
  const tgtRound: Row[] = [
    ["hub1", 0], // hub1 is itself a round: excluded regardless of its tgt_round r
    ["leafA", 2],
    ["leafB", 2],
    ["hub2", 1], // hub2 is itself a round too
  ];
  const roundFs = new Set(round.map(([f]) => f));
  const expected: Row[] = tgtRound.filter(([t]) => !roundFs.has(t)); // {leafA, leafB}

  const sources: Sources = new Map([["round", makeReplay(round)], ["tgt_round", makeReplay(tgtRound)]]);
  const { rels } = lowerProgram(prog, sources);
  assert.deepStrictEqual(currentRows(rels.get("leaf_round")!), sortRows(expected));
});

// ---- (b) the v5 arch-conformance !allowed shape: all-bound vars + a Compare -------
// examples/arch-conformance.dl:39-40 —
//   violation(a, b, ta, tb) <-
//     module_edge(a, b), tier(a, ta), tier(b, tb), ta != tb, !allowed(ta, tb).
// `ta != tb` there is a var-vs-var comparison; ast.ts's `Compare` is Var-vs-Lit only
// (out of scope for this arc, same boundary as the existing agg/join tests) — this
// exercises the same SHAPE (a negated atom with every arg already bound, alongside an
// independent Compare) with a Var-vs-Lit predicate instead.

test("negation (v5 b): violation(...) <- ..., cmp, !allowed(ta, tb) — all-bound negated atom + Compare", () => {
  const prog: Program = {
    rels: [
      edbRel("module_edge", ["a", "b"]),
      edbRel("tier", ["module", "tier"]),
      edbRel("allowed", ["from", "to"]),
      derivedRel("violation", ["a", "b", "ta", "tb"]),
    ],
    rules: [
      {
        head: "violation",
        headTerms: [headVar("a"), headVar("b"), headVar("ta"), headVar("tb")],
        body: [
          relRef("module_edge", v("a"), v("b")),
          relRef("tier", v("a"), v("ta")),
          relRef("tier", v("b"), v("tb")),
          compare("ne", "ta", "presentation"), // Var-vs-Lit stand-in for v5's ta != tb
          notRel("allowed", v("ta"), v("tb")),
        ],
      },
    ],
  };
  const moduleEdge: Row[] = [
    ["ui", "core"],
    ["core", "db"],
    ["db", "core"],
  ];
  const tier: Row[] = [
    ["ui", "presentation"],
    ["core", "domain"],
    ["db", "infra"],
  ];
  const allowed: Row[] = [["domain", "infra"]];
  const allowedSet = new Set(allowed.map(([from, to]) => `${from} ${to}`));
  const tierOf = new Map(tier.map(([m, t]) => [m, t] as const));
  const expected: Row[] = moduleEdge
    .map(([a, b]) => [a, b, tierOf.get(a as string)!, tierOf.get(b as string)!] as Row)
    .filter(([, , ta, tb]) => ta !== "presentation" && !allowedSet.has(`${ta} ${tb}`));

  const sources: Sources = new Map([
    ["module_edge", makeReplay(moduleEdge)],
    ["tier", makeReplay(tier)],
    ["allowed", makeReplay(allowed)],
  ]);
  const { rels } = lowerProgram(prog, sources);
  assert.deepStrictEqual(currentRows(rels.get("violation")!), sortRows(expected));
});

// ---- (c) an ax_ord-style "first only" shape ---------------------------------------
// examples/arch-expr.dl:38-44 —
//   rel ax_has_earlier(url: text).  ax_has_earlier(url) <- ax_parent(url, p), ax_parent(sib, p), sib < url.
//   rel ax_ord(url: text, ord: int).  ax_ord(url, 0) <- ax_parent(url, _), !ax_has_earlier(url).
// The head literal `0` and the positive-ref wildcard `ax_parent(url, _)` are both out
// of scope here (a constant head column, and a positive-ref wildcard — see ast.ts's
// deferrals note); this keeps the negation shape (`!ax_has_earlier(url)`, a single
// bound var) and substitutes a plain bound Var for the positive ref's `_`.

test("negation (v5 c): ax_first(url) <- ax_parent(url, parent), !ax_has_earlier(url) — first-only via negation", () => {
  const prog: Program = {
    rels: [
      edbRel("ax_parent", ["url", "parent"]),
      edbRel("ax_has_earlier", ["url"]),
      derivedRel("ax_first", ["url"]),
    ],
    rules: [
      {
        head: "ax_first",
        headTerms: [headVar("url")],
        body: [relRef("ax_parent", v("url"), v("parent")), notRel("ax_has_earlier", v("url"))],
      },
    ],
  };
  const axParent: Row[] = [["a", "root"], ["b", "root"], ["c", "root"]];
  const axHasEarlier: Row[] = [["b"], ["c"]]; // only 'a' is first under 'root'
  const hasEarlierSet = new Set(axHasEarlier.map(([url]) => url));
  const expected: Row[] = [...new Set(axParent.map(([url]) => url))]
    .filter((url) => !hasEarlierSet.has(url))
    .map((url) => [url] as Row);

  const sources: Sources = new Map([
    ["ax_parent", makeReplay(axParent)],
    ["ax_has_earlier", makeReplay(axHasEarlier)],
  ]);
  const { rels } = lowerProgram(prog, sources);
  assert.deepStrictEqual(currentRows(rels.get("ax_first")!), sortRows(expected));
});

test("negation: a re-emitted negated rel re-derives downstream (a newly-seen row drops out)", () => {
  const prog: Program = {
    rels: [edbRel("all", ["x"]), edbRel("seen", ["x"]), derivedRel("missing", ["x"])],
    rules: [
      {
        head: "missing",
        headTerms: [headVar("x")],
        body: [relRef("all", v("x")), notRel("seen", v("x"))],
      },
    ],
  };
  const all$ = new ReplaySubject<Row[]>(1);
  const seen$ = new ReplaySubject<Row[]>(1);
  all$.next([["a"], ["b"]]);
  seen$.next([]);
  const sources: Sources = new Map([["all", all$.asObservable()], ["seen", seen$.asObservable()]]);
  const { rels } = lowerProgram(prog, sources);
  const out: Row[][] = [];
  const sub = rels.get("missing")!.subscribe((r) => out.push(r));
  assert.deepStrictEqual(out[0], sortRows([["a"], ["b"]]), "nothing seen yet: both missing");
  seen$.next([["a"]]); // 'a' becomes seen -> drops out of missing
  sub.unsubscribe();
  assert.strictEqual(out.length, 2, "the negated rel's re-emission re-derived missing");
  assert.deepStrictEqual(out[1], [["b"]]);
});

// ---- (d) the non-stratifiable diagnostic, v5's exact message wording -------------
// src/typecheck.rs:1201 (`not-stratified`):
//   format!("relation `{}` is aggregated or negated inside a recursive cycle with `{}`",
//           names[b], names[h])
// where b = the negated (body-side) rel, h = the head rule reading it.

test("negation (v5 d): p <- !p (self-loop) throws NonStratifiableError with v5's exact wording", () => {
  const prog: Program = {
    rels: [derivedRel("p", ["x"])],
    rules: [{ head: "p", headTerms: [headVar("x")], body: [notRel("p", v("x"))] }],
  };
  assert.throws(
    () => lowerProgram(prog, new Map()),
    (err: unknown) => {
      assert.ok(err instanceof NonStratifiableError, "typed diagnostic class");
      assert.strictEqual(err.rel, "p");
      assert.strictEqual(err.cycleWith, "p");
      assert.strictEqual(
        err.message,
        "relation `p` is aggregated or negated inside a recursive cycle with `p`",
      );
      return true;
    },
  );
});

test("negation: a genuine 2-node cycle with a negative edge on one leg is NonStratifiableError", () => {
  // a(x) <- !b(x).  b(x) <- a(x).   a and b co-recurse; the a<-!b edge is negative and
  // both endpoints share the {a,b} SCC. v5 naming: b = the negated rel, a = the head.
  const prog: Program = {
    rels: [derivedRel("a", ["x"]), derivedRel("b", ["x"])],
    rules: [
      { head: "a", headTerms: [headVar("x")], body: [notRel("b", v("x"))] },
      { head: "b", headTerms: [headVar("x")], body: [relRef("a", v("x"))] },
    ],
  };
  assert.throws(
    () => lowerProgram(prog, new Map()),
    (err: unknown) => {
      assert.ok(err instanceof NonStratifiableError);
      assert.strictEqual(err.rel, "b");
      assert.strictEqual(err.cycleWith, "a");
      assert.strictEqual(
        err.message,
        "relation `b` is aggregated or negated inside a recursive cycle with `a`",
      );
      return true;
    },
  );
});

test("negation: a NegRelRef against a rel outside any cycle stratifies fine (control case)", () => {
  // sanity: negation alone doesn't trip the check when the negated rel is acyclic and
  // outside the reader's SCC (this is the common/expected shape, already covered above
  // by the missing()/leaf_round()/ax_first() tests, but assert directly against
  // buildRuleGraph+stratify so the rulegraph-level contract is pinned independent of
  // lowerProgram).
  const prog: Program = {
    rels: [edbRel("all", ["x"]), edbRel("seen", ["x"]), derivedRel("missing", ["x"])],
    rules: [
      { head: "missing", headTerms: [headVar("x")], body: [relRef("all", v("x")), notRel("seen", v("x"))] },
    ],
  };
  const graph = buildRuleGraph(prog);
  assert.doesNotThrow(() => stratify(graph, scc(graph)));
});

// ---- (e) evaluation order: the negated rel's stratum is COMPLETE before its reader --
// src/engine/derive.rs:347 — v5 evaluates stratum by stratum, so a higher stratum's
// negation reads a relation lower strata already finished. `stratify`'s topo order
// already guarantees this (dependencies always precede dependents, and a negated ref
// is a dependency edge just like a positive one) — asserted here directly rather than
// re-derived, per the redirect's instruction.

test("negation: stratify orders a DERIVED negated rel strictly before its reader (v5 derive.rs:347)", () => {
  // seen(x) <- raw(x), compare(...).   (derived, not EDB, so it has a real stratum order)
  // missing(x) <- raw(x), !seen(x).
  const prog: Program = {
    rels: [edbRel("raw", ["x"]), derivedRel("seen", ["x"]), derivedRel("missing", ["x"])],
    rules: [
      { head: "seen", headTerms: [headVar("x")], body: [relRef("raw", v("x")), compare("ne", "x", "z")] },
      { head: "missing", headTerms: [headVar("x")], body: [relRef("raw", v("x")), notRel("seen", v("x"))] },
    ],
  };
  const graph = buildRuleGraph(prog);
  const strata = stratify(graph, scc(graph));
  const orderOf = (rel: string): number => strata.find((stratum) => stratum.rels.includes(rel))!.order;
  assert.ok(
    orderOf("seen") < orderOf("missing"),
    "the negated rel's stratum runs strictly before the reader's — it is complete first",
  );

  // and the lowered values agree with a from-scratch reference, confirming the order
  // guarantee is not just topological bookkeeping but semantically load-bearing.
  const raw: Row[] = [["a"], ["z"], ["c"]];
  const seenExpected = new Set(raw.filter(([x]) => x !== "z").map(([x]) => x));
  const expectedMissing: Row[] = raw.filter(([x]) => !seenExpected.has(x)); // {z}

  const sources: Sources = new Map([["raw", makeReplay(raw)]]);
  const { rels, deferred } = lowerProgram(prog, sources);
  assert.deepStrictEqual(deferred, []);
  assert.deepStrictEqual(currentRows(rels.get("missing")!), sortRows(expectedMissing));
});

// =============================================================================
// helper: build a ReplaySubject seeded with one row set (the rel's current facts).
// =============================================================================
function makeReplay(rows: Row[]): Observable<Row[]> {
  const subject = new ReplaySubject<Row[]>(1);
  subject.next(rows);
  return subject.asObservable();
}
