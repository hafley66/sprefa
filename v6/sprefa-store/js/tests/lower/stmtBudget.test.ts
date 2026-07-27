/**
 * stmtBudget.test.ts: the N+1 rail for the v6 SQL datalog evaluator.
 *
 * THE LAW (v5 tick-counter law, promoted here into a graded check):
 *
 *   statements per evaluation = f(rules, strata, fixpoint rounds)
 *   and NEVER f(rows).
 *
 * Batch is the default unit of work. A statement issued per row, per fact, or per
 * derived tuple is an N+1 and a blocking defect, not a performance follow-up. Any
 * future change that makes the count at N=1000 differ from the count at N=10 for
 * data of the same SHAPE breaks this file, and that break is the defect report.
 *
 * WHERE THE COUNT IS TAKEN. `SqlRunner` is the single seam every statement crosses
 * (src/engine/sqlRunner.ts): it increments `stmt_counter` and calls the optional
 * `TraceStatement` hook in the same breath, so a statement cannot reach SQLite
 * through the evaluator and escape either one. The evaluator takes that hook as a
 * constructor argument (`DatalogEvaluator`'s 5th parameter), so this file needs no
 * decorator and no src change: it passes a recording trace and reads the SQL back.
 * Every run additionally asserts trace-length === stmt_counter delta, which is what
 * catches a future statement path that runs SQL without the trace.
 *
 * PHASES ARE COUNTED SEPARATELY. Setup (CREATE TABLE + the EDB load) is issued by
 * this test directly against the connection, never through SqlRunner, so it never
 * lands in the evaluation budget. Within the evaluation the statements split three
 * ways (`phaseOf` below): `clear` (one DELETE per rule-headed rel), `acyclic` (one
 * INSERT..SELECT per rule in a non-recursive stratum), and `fixpoint` (the
 * semi-naive delta/next machinery).
 *
 * THE ONE DATA-DEPENDENT TERM, stated rather than hidden. Fixpoint ROUNDS track the
 * recursion depth of the input graph, and depth is a property of the data. The count
 * is exactly `13 + 6 * rounds` for the battery program below. It is flat in the
 * number of rows at fixed depth (test 1, 100x rows, byte-identical statement list)
 * and flat in graph WIDTH (test 3). It is linear in depth (test 4). Semi-naive
 * evaluation cannot avoid one round per derivation level; escaping it means a
 * different closure algorithm (a single recursive CTE), which is a design decision,
 * not a bug fix. Test 4 pins the current truth so a ruling can be made against
 * numbers instead of guesses.
 */

import { test } from "node:test";
import { strict as assert } from "node:assert";
import { firstValueFrom } from "rxjs";
import { createClient } from "@libsql/client";

import {
  derivedRel,
  edbRel,
  headAgg,
  headVar,
  notRel,
  relRef,
  v,
  wild,
  type Program,
} from "../../src/lower/ast.ts";
import { DatalogEvaluator } from "../../src/lower/lowerSql.ts";
import { stmt_counter } from "../../src/engine/counter.ts";
import type { RelTable, RelTables } from "../../src/lower/types.ts";

type Row = readonly (number | string)[];
type Edb = Readonly<Record<string, readonly Row[]>>;

/**
 * The battery program: 6 rels across 6 strata, one recursive SCC, one aggregate head,
 * one negated body predicate. Deliberately wider than any single feature test so the
 * budget covers every lowering path the evaluator has.
 *
 *   has_out(x)         <- edge(x, _)                     acyclic
 *   path(x, y)         <- edge(x, y)                     recursive SCC, exit rule
 *   path(x, z)         <- path(x, y), edge(y, z)         recursive SCC, step rule
 *   sink(x)            <- node(x), !has_out(x)           negation, later stratum
 *   reach_count(x, #y) <- path(x, y)                     aggregate, later stratum
 */
const PROGRAM: Program = {
  rels: [
    edbRel("edge", ["src", "dst"]),
    edbRel("node", ["id"]),
    derivedRel("has_out", ["id"]),
    derivedRel("path", ["src", "dst"]),
    derivedRel("sink", ["id"]),
    derivedRel("reach_count", ["src", "n"]),
  ],
  rules: [
    { head: "has_out", headTerms: [headVar("x")], body: [relRef("edge", v("x"), wild())] },
    { head: "path", headTerms: [headVar("x"), headVar("y")], body: [relRef("edge", v("x"), v("y"))] },
    {
      head: "path",
      headTerms: [headVar("x"), headVar("z")],
      body: [relRef("path", v("x"), v("y")), relRef("edge", v("y"), v("z"))],
    },
    { head: "sink", headTerms: [headVar("x")], body: [relRef("node", v("x")), notRel("has_out", v("x"))] },
    { head: "reach_count", headTerms: [headVar("x"), headAgg("count", "y")], body: [relRef("path", v("x"), v("y"))] },
  ],
};

/** Statements per fixpoint round for PROGRAM: drop+create `next`, one delta pass for the
 *  one recursive body position, merge `next` into the full table, drop+rename to `delta`. */
const PER_ROUND = 6;
/** Everything outside the round loop: 4 clears, has_out, sink, reach_count, and the
 *  fixpoint's seed (drop+create delta, 2 rule inserts, 1 merge) plus the final delta drop. */
const FIXED = 13;

// ─────────────────────────────────────────────────────────────────────────────
// Fixtures. Each one names which dimension it grows.
// ─────────────────────────────────────────────────────────────────────────────

/** `clusters` independent depth-2 diamonds: a -> {b, c} -> d. Rows scale with
 *  `clusters`; recursion depth is 2 no matter how large it gets. */
function diamonds(clusters: number): Edb {
  const edge: Row[] = [];
  const node: Row[] = [];
  for (let cluster = 0; cluster < clusters; cluster++) {
    const a = cluster * 4;
    edge.push([a, a + 1], [a, a + 2], [a + 1, a + 3], [a + 2, a + 3]);
    node.push([a], [a + 1], [a + 2], [a + 3]);
  }
  return { edge, node };
}

/** `width` disjoint chains, each of `depth` edges. Grows rows along either axis
 *  independently, so a test can move one and hold the other. */
function chains(width: number, depth: number): Edb {
  const edge: Row[] = [];
  const node: Row[] = [];
  for (let lane = 0; lane < width; lane++) {
    const base = lane * (depth + 1);
    node.push([base]);
    for (let step = 0; step < depth; step++) {
      edge.push([base + step, base + step + 1]);
      node.push([base + step + 1]);
    }
  }
  return { edge, node };
}

// ─────────────────────────────────────────────────────────────────────────────
// The measurement.
// ─────────────────────────────────────────────────────────────────────────────

type Phase = "clear" | "acyclic" | "fixpoint";

/** Which budget line a statement belongs to. The `_dl_` prefix is the evaluator's own
 *  working-table namespace (RecursiveStratum's delta/next names), so it identifies the
 *  semi-naive machinery without matching any user rel. */
function phaseOf(sql: string): Phase {
  if (sql.startsWith("DELETE FROM")) return "clear";
  if (sql.includes("_dl_delta_") || sql.includes("_dl_next_")) return "fixpoint";
  return "acyclic";
}

interface Budget {
  /** Statements the test issued to create + load the tables. Never through SqlRunner. */
  setup: number;
  /** Every statement the evaluation ran, in order, as SQL. */
  evaluation: string[];
  byPhase: Record<Phase, number>;
  /** Fixpoint rounds, recovered from the round loop's rename statement. */
  rounds: number;
  strata: number;
  /** Rows in each IDB rel after settling, so a flat budget cannot be flat because the
   *  evaluation quietly did nothing. */
  derived: Record<string, number>;
}

async function measure(edb: Edb): Promise<Budget> {
  const db = createClient({ url: ":memory:" });
  try {
    let setup = 0;
    const tables: Map<string, RelTable> = new Map();
    for (const decl of PROGRAM.rels) {
      tables.set(decl.name, { table: `t_${decl.name}`, columns: decl.columns });
      await db.executeMultiple(
        `CREATE TABLE t_${decl.name}(${decl.columns.join(", ")}, PRIMARY KEY (${decl.columns.join(", ")})) WITHOUT ROWID`,
      );
      setup++;
    }
    // ONE multi-row INSERT per rel. A per-row loop here would be the N+1 this file exists
    // to forbid, and it would make the setup line of the budget grow with the fixture.
    for (const [relName, rows] of Object.entries(edb)) {
      const table = tables.get(relName)!;
      if (rows.length === 0) continue;
      const values = rows.map((row) => `(${row.map((value) => String(value)).join(",")})`).join(",");
      await db.executeMultiple(`INSERT OR IGNORE INTO ${table.table}(${table.columns.join(",")}) VALUES ${values}`);
      setup++;
    }

    const evaluation: string[] = [];
    const evaluator = new DatalogEvaluator(db, PROGRAM, tables as RelTables, undefined, (sql) => evaluation.push(sql));
    const before = stmt_counter.get();
    await firstValueFrom(evaluator.run());
    const counted = stmt_counter.get() - before;

    // The trace and the global counter are incremented by the same lines of SqlRunner.
    // They can only disagree if a new execution path skipped one, which would mean this
    // whole budget is measuring less than the engine actually ran.
    assert.equal(
      evaluation.length,
      counted,
      `trace saw ${evaluation.length} statements but stmt_counter saw ${counted}: a statement path is bypassing the trace hook`,
    );

    const byPhase: Record<Phase, number> = { clear: 0, acyclic: 0, fixpoint: 0 };
    for (const sql of evaluation) byPhase[phaseOf(sql)]++;

    const derived: Record<string, number> = {};
    for (const decl of PROGRAM.rels) {
      if (decl.origin === "EDB") continue;
      const result = await db.execute(`SELECT count(*) FROM t_${decl.name}`);
      derived[decl.name] = Number(result.rows[0]![0]);
    }

    return {
      setup,
      evaluation,
      byPhase,
      rounds: evaluation.filter((sql) => sql.startsWith("ALTER TABLE")).length,
      strata: evaluator.strata.length,
      derived,
    };
  } finally {
    db.close();
  }
}

/** Statement SHAPES, argument values and row counts erased. Two runs over differently
 *  sized data of the same shape must produce the identical list, in order. */
function shapes(evaluation: readonly string[]): string[] {
  return evaluation.map((sql) => sql.replace(/\s+/g, " ").trim());
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. THE RAIL. 100x the rows, byte-identical statement list.
// ─────────────────────────────────────────────────────────────────────────────

test("stmt budget: 100x the rows issues the byte-identical statement list", async () => {
  const small = await measure(diamonds(10)); // 40 edges, 40 nodes
  const large = await measure(diamonds(1000)); // 4000 edges, 4000 nodes

  // The evaluation really did 100x the work, so a flat count is not a flat no-op.
  assert.equal(small.derived["path"], 50);
  assert.equal(large.derived["path"], 5000);
  assert.equal(large.derived["path"]! / small.derived["path"]!, 100);

  assert.deepStrictEqual(
    shapes(large.evaluation),
    shapes(small.evaluation),
    "statement list diverged between N=10 and N=1000: the differing statements are the N+1",
  );
  assert.equal(large.evaluation.length, small.evaluation.length);
  assert.deepStrictEqual(large.byPhase, small.byPhase);
  assert.equal(large.setup, small.setup, "setup is one CREATE per rel plus one batched INSERT per EDB rel");
});

// ─────────────────────────────────────────────────────────────────────────────
// 2. The budget's exact composition, pinned per phase so a regression names itself.
// ─────────────────────────────────────────────────────────────────────────────

test("stmt budget: the exact per-phase numbers for the battery program", async () => {
  const budget = await measure(diamonds(1000));

  assert.equal(budget.strata, 6);
  assert.equal(budget.rounds, 2, "depth-2 diamonds settle in one productive round plus one empty round");
  assert.deepStrictEqual(budget.byPhase, {
    clear: 4, // one DELETE per rule-headed rel: has_out, path, sink, reach_count
    acyclic: 3, // one INSERT..SELECT per rule outside the SCC: has_out, sink, reach_count
    fixpoint: 18, // seed 5 + 2 rounds x 6 + 1 final delta drop
  });
  assert.equal(budget.evaluation.length, 25);
  assert.equal(budget.evaluation.length, FIXED + PER_ROUND * budget.rounds);
  assert.equal(budget.setup, 8, "6 CREATE TABLE + 2 batched EDB INSERTs");
});

// ─────────────────────────────────────────────────────────────────────────────
// 3. Flat in graph WIDTH. 100x the lanes at fixed depth, same statements.
// ─────────────────────────────────────────────────────────────────────────────

test("stmt budget: flat in graph width at fixed recursion depth", async () => {
  const narrow = await measure(chains(10, 4));
  const wide = await measure(chains(1000, 4));

  assert.equal(wide.derived["path"]! / narrow.derived["path"]!, 100);
  assert.deepStrictEqual(shapes(wide.evaluation), shapes(narrow.evaluation));
  assert.equal(wide.rounds, narrow.rounds);
});

// ─────────────────────────────────────────────────────────────────────────────
// 4. The one data-dependent term: rounds track recursion DEPTH.
//
// This test does not assert flatness, because the evaluator is not flat here and
// massaging the assertion would hide that. It pins the exact relationship instead.
// The six statements that repeat per round, for the single recursive rel `path`:
//
//   DROP TABLE IF EXISTS "_dl_next_path"
//   CREATE TEMP TABLE "_dl_next_path"("src","dst", PRIMARY KEY ("src","dst")) WITHOUT ROWID
//   INSERT OR IGNORE INTO "_dl_next_path"(...) SELECT ... FROM "_dl_delta_path" b0, t_edge b1 ...
//   INSERT OR IGNORE INTO "t_path"(...) SELECT ... FROM "_dl_next_path"
//   DROP TABLE IF EXISTS "_dl_delta_path"
//   ALTER TABLE "_dl_next_path" RENAME TO "_dl_delta_path"
//
// Four of the six are working-table churn rather than data movement. Whether that
// churn is worth removing (reuse two fixed TEMP tables and DELETE between rounds:
// 6 per round becomes 4) and whether the round loop itself should be replaced by a
// single recursive CTE (rounds becomes 0, depth-independent) are both open design
// calls, and both are the user's, not this test's.
// ─────────────────────────────────────────────────────────────────────────────

test("stmt budget: rounds are linear in recursion depth, at exactly PER_ROUND each", async () => {
  const shallow = await measure(chains(1, 10));
  const deep = await measure(chains(1, 40));

  assert.equal(shallow.rounds, 10);
  assert.equal(deep.rounds, 40);
  assert.equal(shallow.evaluation.length, FIXED + PER_ROUND * 10); // 73
  assert.equal(deep.evaluation.length, FIXED + PER_ROUND * 40); // 253
  assert.equal(
    (deep.evaluation.length - shallow.evaluation.length) / (deep.rounds - shallow.rounds),
    PER_ROUND,
    "the per-round cost is a program constant; only the round COUNT follows the data",
  );

  // Everything outside the round loop stays put while depth moves 4x.
  assert.equal(deep.byPhase.clear, shallow.byPhase.clear);
  assert.equal(deep.byPhase.acyclic, shallow.byPhase.acyclic);
});

// ─────────────────────────────────────────────────────────────────────────────
// 5. Non-recursive strata alone: zero data dependence of any kind.
// ─────────────────────────────────────────────────────────────────────────────

test("stmt budget: an acyclic program is one statement per rule, whatever the data", async () => {
  const acyclic: Program = {
    rels: [
      edbRel("edge", ["src", "dst"]),
      edbRel("node", ["id"]),
      derivedRel("has_out", ["id"]),
      derivedRel("sink", ["id"]),
      derivedRel("out_degree", ["src", "n"]),
    ],
    rules: [
      { head: "has_out", headTerms: [headVar("x")], body: [relRef("edge", v("x"), wild())] },
      { head: "sink", headTerms: [headVar("x")], body: [relRef("node", v("x")), notRel("has_out", v("x"))] },
      {
        head: "out_degree",
        headTerms: [headVar("x"), headAgg("count", "y")],
        body: [relRef("edge", v("x"), v("y"))],
      },
    ],
  };

  const run = async (clusters: number): Promise<string[]> => {
    const db = createClient({ url: ":memory:" });
    try {
      const tables: Map<string, RelTable> = new Map();
      for (const decl of acyclic.rels) {
        tables.set(decl.name, { table: `t_${decl.name}`, columns: decl.columns });
        await db.executeMultiple(
          `CREATE TABLE t_${decl.name}(${decl.columns.join(", ")}, PRIMARY KEY (${decl.columns.join(", ")})) WITHOUT ROWID`,
        );
      }
      for (const [relName, rows] of Object.entries(diamonds(clusters))) {
        const table = tables.get(relName)!;
        const values = rows.map((row) => `(${row.map((value) => String(value)).join(",")})`).join(",");
        await db.executeMultiple(`INSERT OR IGNORE INTO ${table.table}(${table.columns.join(",")}) VALUES ${values}`);
      }
      const evaluation: string[] = [];
      const evaluator = new DatalogEvaluator(db, acyclic, tables as RelTables, undefined, (sql) => evaluation.push(sql));
      await firstValueFrom(evaluator.run());
      return shapes(evaluation);
    } finally {
      db.close();
    }
  };

  const small = await run(10);
  const large = await run(1000);
  assert.deepStrictEqual(large, small);
  assert.equal(small.length, 6, "3 clears + 3 rule INSERT..SELECTs, and nothing else, ever");
});
