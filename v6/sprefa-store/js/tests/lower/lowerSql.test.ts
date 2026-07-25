/**
 * lowerSql.test.ts — the SQL datalog evaluator (src/lower/lowerSql.ts) proven correct
 * two ways: against a from-scratch hand oracle, and DIFFERENTIALLY against the in-memory
 * rx evaluator (src/lower/lower.ts) for every program the in-memory one can lower.
 *
 * The differential arm is the class-34 defense: a subtly-wrong incremental/SQL
 * maintenance passes a small golden while diverging at scale, so we assert byte-equal
 * output against an independently-written evaluator on the SAME programs, not just a
 * pinned snapshot.
 *
 * Battery: transitive closure, ancestor, same-generation (mutual/self recursion with a
 * join), stratified negation (v5 surface), comparison selection, aggregation.
 */

import { test } from "node:test";
import { strict as assert } from "node:assert";
import { ReplaySubject, firstValueFrom, type Observable } from "rxjs";
import { createClient } from "@libsql/client";

import {
  compare,
  derivedRel,
  edbRel,
  headAgg,
  headVar,
  notRel,
  relRef,
  v,
  type Program,
} from "../../src/lower/ast.ts";
import { lowerProgram, type Row, type Sources } from "../../src/lower/lower.ts";
import { evalProgramSql } from "../../src/lower/lowerSql.ts";
import type { RelTable, RelTables } from "../../src/lower/types.ts";

function sortRows(rows: readonly Row[]): Row[] {
  return rows.map((r) => [...r]).sort((a, b) => (JSON.stringify(a) < JSON.stringify(b) ? -1 : 1));
}

/** Run a program through the SQL evaluator over a fresh :memory: db; return every IDB
 *  rel's rows. EDB tables are created + populated from `edb`; IDB tables created empty. */
async function runSql(prog: Program, edb: Record<string, Row[]>): Promise<Map<string, Row[]>> {
  const db = createClient({ url: ":memory:" });
  try {
    const tables: Map<string, RelTable> = new Map();
    for (const decl of prog.rels) {
      const cols = decl.columns;
      tables.set(decl.name, { table: `t_${decl.name}`, columns: cols });
      await db.executeMultiple(
        `CREATE TABLE t_${decl.name}(${cols.join(", ")}, PRIMARY KEY (${cols.join(", ")})) WITHOUT ROWID`,
      );
    }
    for (const [relName, rows] of Object.entries(edb)) {
      const t = tables.get(relName)!;
      for (const row of rows) {
        const vals = row.map(sqlLit).join(", ");
        await db.executeMultiple(`INSERT OR IGNORE INTO ${t.table}(${t.columns.join(", ")}) VALUES (${vals})`);
      }
    }
    await firstValueFrom(evalProgramSql(db, prog, tables as RelTables));

    const out = new Map<string, Row[]>();
    for (const decl of prog.rels) {
      if (decl.origin === "EDB") continue;
      const t = tables.get(decl.name)!;
      const res = await db.execute(`SELECT ${t.columns.join(", ")} FROM ${t.table}`);
      out.set(
        decl.name,
        res.rows.map((r) => t.columns.map((_, i) => normalize(r[i]))),
      );
    }
    return out;
  } finally {
    db.close();
  }
}

function sqlLit(value: unknown): string {
  if (value === null || value === undefined) return "NULL";
  if (typeof value === "number") return String(value);
  if (typeof value === "boolean") return value ? "1" : "0";
  return `'${String(value).replace(/'/g, "''")}'`;
}

/** libsql returns bigints for integer columns; normalize to number for comparison. */
function normalize(v: unknown): unknown {
  if (typeof v === "bigint") return Number(v);
  return v;
}

/** Run a program through the in-memory rx evaluator; return each IDB rel's current rows,
 *  or null if any stratum deferred (the in-memory backend cannot lower it). */
function runMem(prog: Program, edb: Record<string, Row[]>): Map<string, Row[]> | null {
  const sources: Sources = new Map();
  for (const [relName, rows] of Object.entries(edb)) {
    const subject = new ReplaySubject<Row[]>(1);
    subject.next(rows);
    sources.set(relName, subject.asObservable());
  }
  const { rels, deferred } = lowerProgram(prog, sources);
  if (deferred.length > 0) return null;
  const out = new Map<string, Row[]>();
  for (const decl of prog.rels) {
    if (decl.origin === "EDB") continue;
    const obs = rels.get(decl.name);
    if (!obs) continue;
    out.set(decl.name, currentRows(obs));
  }
  return out;
}

function currentRows(obs: Observable<Row[]>): Row[] {
  const emissions: Row[][] = [];
  const sub = obs.subscribe((x) => emissions.push(x));
  sub.unsubscribe();
  return emissions[emissions.length - 1] ?? [];
}

/** Assert SQL === hand oracle, and (when the in-memory backend lowers it) SQL === memory. */
async function assertRel(
  prog: Program,
  edb: Record<string, Row[]>,
  relName: string,
  expected: Row[],
): Promise<void> {
  const sql = await runSql(prog, edb);
  assert.deepStrictEqual(sortRows(sql.get(relName) ?? []), sortRows(expected), `SQL ${relName}`);
  const mem = runMem(prog, edb);
  if (mem !== null) {
    assert.deepStrictEqual(sortRows(mem.get(relName) ?? []), sortRows(expected), `in-memory ${relName}`);
    assert.deepStrictEqual(sortRows(sql.get(relName) ?? []), sortRows(mem.get(relName) ?? []), `SQL===memory ${relName}`);
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. Transitive closure (the canonical recursion).
// ─────────────────────────────────────────────────────────────────────────────

test("lowerSql: transitive closure — path === reachability oracle, SQL === in-memory", async () => {
  const prog: Program = {
    rels: [edbRel("edge", ["src", "dst"]), derivedRel("path", ["src", "dst"])],
    rules: [
      { head: "path", headTerms: [headVar("x"), headVar("y")], body: [relRef("edge", v("x"), v("y"))] },
      {
        head: "path",
        headTerms: [headVar("x"), headVar("z")],
        body: [relRef("path", v("x"), v("y")), relRef("edge", v("y"), v("z"))],
      },
    ],
  };
  const edge: Row[] = [
    [1, 2],
    [2, 3],
    [3, 4],
    [5, 6],
  ];
  // Floyd-Warshall-style reference closure.
  const nodes = [...new Set(edge.flat())];
  const reach = new Set(edge.map(([a, b]) => `${a},${b}`));
  for (const k of nodes) for (const i of nodes) for (const j of nodes) {
    if (reach.has(`${i},${k}`) && reach.has(`${k},${j}`)) reach.add(`${i},${j}`);
  }
  const expected: Row[] = [...reach].map((s) => s.split(",").map(Number));
  await assertRel(prog, { edge }, "path", expected);
});

// ─────────────────────────────────────────────────────────────────────────────
// 2. Ancestor (recursion built from a base + step rule over a different EDB).
// ─────────────────────────────────────────────────────────────────────────────

test("lowerSql: ancestor — recursion over parent, SQL === in-memory", async () => {
  const prog: Program = {
    rels: [edbRel("parent", ["p", "c"]), derivedRel("ancestor", ["a", "d"])],
    rules: [
      { head: "ancestor", headTerms: [headVar("p"), headVar("c")], body: [relRef("parent", v("p"), v("c"))] },
      {
        head: "ancestor",
        headTerms: [headVar("a"), headVar("d")],
        body: [relRef("parent", v("a"), v("m")), relRef("ancestor", v("m"), v("d"))],
      },
    ],
  };
  const parent: Row[] = [
    ["alice", "bob"],
    ["bob", "carol"],
    ["carol", "dave"],
  ];
  const expected: Row[] = [
    ["alice", "bob"],
    ["bob", "carol"],
    ["carol", "dave"],
    ["alice", "carol"],
    ["alice", "dave"],
    ["bob", "dave"],
  ];
  await assertRel(prog, { parent }, "ancestor", expected);
});

// ─────────────────────────────────────────────────────────────────────────────
// 3. Same-generation (self-recursion with two joins — the classic hard recursion).
//   sg(X,Y) <- sibling base ; sg(X,Y) <- parent(P,X), sg(P,Q), parent(Q,Y).
// ─────────────────────────────────────────────────────────────────────────────

test("lowerSql: same-generation — double-join recursion, SQL === in-memory", async () => {
  const prog: Program = {
    rels: [edbRel("par", ["p", "c"]), edbRel("sibbase", ["x", "y"]), derivedRel("sg", ["x", "y"])],
    rules: [
      { head: "sg", headTerms: [headVar("x"), headVar("y")], body: [relRef("sibbase", v("x"), v("y"))] },
      {
        head: "sg",
        headTerms: [headVar("x"), headVar("y")],
        body: [relRef("par", v("p"), v("x")), relRef("sg", v("p"), v("q")), relRef("par", v("q"), v("y"))],
      },
    ],
  };
  const par: Row[] = [
    ["r", "a"],
    ["r", "b"],
    ["a", "c"],
    ["b", "d"],
  ];
  const sibbase: Row[] = [["r", "r"]];
  // Hand oracle: sg(r,r) base; then children of r are same-gen: sg(a,a),sg(a,b),sg(b,a),sg(b,b);
  // then children of {a,b}: sg(c,c),sg(c,d),sg(d,c),sg(d,d).
  const expected: Row[] = [
    ["r", "r"],
    ["a", "a"],
    ["a", "b"],
    ["b", "a"],
    ["b", "b"],
    ["c", "c"],
    ["c", "d"],
    ["d", "c"],
    ["d", "d"],
  ];
  await assertRel(prog, { par, sibbase }, "sg", expected);
});

// ─────────────────────────────────────────────────────────────────────────────
// 4. Stratified negation (v5 surface !rel): unreachable pairs.
//   path = closure(edge); unreach(X,Y) <- node(X), node(Y), !path(X,Y).
// ─────────────────────────────────────────────────────────────────────────────

test("lowerSql: stratified negation — unreachable = node×node minus path, SQL === in-memory", async () => {
  const prog: Program = {
    rels: [
      edbRel("edge", ["src", "dst"]),
      edbRel("node", ["id"]),
      derivedRel("path", ["src", "dst"]),
      derivedRel("unreach", ["x", "y"]),
    ],
    rules: [
      { head: "path", headTerms: [headVar("x"), headVar("y")], body: [relRef("edge", v("x"), v("y"))] },
      {
        head: "path",
        headTerms: [headVar("x"), headVar("z")],
        body: [relRef("path", v("x"), v("y")), relRef("edge", v("y"), v("z"))],
      },
      {
        head: "unreach",
        headTerms: [headVar("x"), headVar("y")],
        body: [relRef("node", v("x")), relRef("node", v("y")), notRel("path", v("x"), v("y"))],
      },
    ],
  };
  const edge: Row[] = [
    [1, 2],
    [2, 3],
  ];
  const node: Row[] = [[1], [2], [3]];
  // path = {12,23,13}. all pairs 3x3 = 9; minus path (3) = 6 unreachable.
  const pathSet = new Set(["1,2", "2,3", "1,3"]);
  const expected: Row[] = [];
  for (const x of [1, 2, 3]) for (const y of [1, 2, 3]) if (!pathSet.has(`${x},${y}`)) expected.push([x, y]);
  await assertRel(prog, { edge, node }, "unreach", expected);
});

// ─────────────────────────────────────────────────────────────────────────────
// 5. Comparison selection + join (non-recursive).
// ─────────────────────────────────────────────────────────────────────────────

test("lowerSql: comparison selection — big(x) <- val(x,n), n > 10, SQL === in-memory", async () => {
  const prog: Program = {
    rels: [edbRel("val", ["x", "n"]), derivedRel("big", ["x"])],
    rules: [
      { head: "big", headTerms: [headVar("x")], body: [relRef("val", v("x"), v("n")), compare("gt", "n", 10)] },
    ],
  };
  const val: Row[] = [
    ["a", 5],
    ["b", 15],
    ["c", 20],
    ["d", 10],
  ];
  const expected: Row[] = [["b"], ["c"]];
  await assertRel(prog, { val }, "big", expected);
});

// ─────────────────────────────────────────────────────────────────────────────
// 6. Aggregation (count per group), non-recursive stratum.
// ─────────────────────────────────────────────────────────────────────────────

test("lowerSql: aggregation — deg(x, count(y)) <- edge(x,y), SQL === in-memory", async () => {
  const prog: Program = {
    rels: [edbRel("edge", ["src", "dst"]), derivedRel("deg", ["x", "n"])],
    rules: [{ head: "deg", headTerms: [headVar("x"), headAgg("count", "y")], body: [relRef("edge", v("x"), v("y"))] }],
  };
  const edge: Row[] = [
    [1, 2],
    [1, 3],
    [1, 4],
    [2, 3],
  ];
  const expected: Row[] = [
    [1, 3],
    [2, 1],
  ];
  await assertRel(prog, { edge }, "deg", expected);
});

// ─────────────────────────────────────────────────────────────────────────────
// 7. Ungrouped aggregation over an EMPTY body. The divergence this pins (found while
// wiring lowerSql into v6/dl, 2026-07-25): bare `SELECT count(x) FROM t` returns ONE
// row (0) on an empty table, while the in-memory evaluator groups bindings and emits
// NO row for zero bindings. dl's fixtures/sg-rail.dl `hit_count(hits: count(path))`
// rides exactly this shape, so the whole-rel-empty case is the one the curl golden hits.
// ─────────────────────────────────────────────────────────────────────────────

test("lowerSql: ungrouped count over an EMPTY body derives no fact, SQL === in-memory", async () => {
  const prog: Program = {
    rels: [edbRel("hit", ["path"]), derivedRel("hit_count", ["hits"])],
    rules: [{ head: "hit_count", headTerms: [headAgg("count", "path")], body: [relRef("hit", v("path"))] }],
  };
  await assertRel(prog, { hit: [] }, "hit_count", []);
});

test("lowerSql: ungrouped count over a NON-empty body derives the one total row, SQL === in-memory", async () => {
  const prog: Program = {
    rels: [edbRel("hit", ["path"]), derivedRel("hit_count", ["hits"])],
    rules: [{ head: "hit_count", headTerms: [headAgg("count", "path")], body: [relRef("hit", v("path"))] }],
  };
  await assertRel(prog, { hit: [["a"], ["b"], ["c"]] }, "hit_count", [[3]]);
});
