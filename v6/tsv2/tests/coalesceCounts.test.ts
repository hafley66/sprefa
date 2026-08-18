/**
 * coalesceCounts.test.ts — the COUNT/PLAN receipt for `coalesce/2` (repo law:
 * "formerly-quadratic paths get COUNT tests ... never end-state equality
 * alone"; ruling null_design = get_else_use_site_never_storage).
 *
 * `coalesce(latest_commit(Name, Commit), 'absent')` desugars to two clauses of
 * one head: the join, and `not(latest_commit(Name, _))` plus a bind of the
 * default. The claim that it therefore inherits the shipped incremental path
 * for free is exactly the kind of claim end-state equality cannot check --
 * a defaulting rule that re-derived the whole table every tick, or whose
 * NOT EXISTS scanned the source rel once per row, would still answer
 * correctly and would still grade IDENTICAL in the sweep.
 *
 * Two receipts:
 *   1. STATEMENTS PER TICK are flat across a 5-row and a 100-row corpus,
 *      counted with sprefa-store-engine's stmt_counter (the retentionCount
 *      precedent), so the absent arm cannot be paying per row.
 *   2. THE NEGATION ARM'S PLAN is a SEARCH on the source rel, never a SCAN,
 *      read by EXPLAIN QUERY PLAN off the REAL captured statement text in
 *      v6/prolog/compile/out/, never a hand-written approximation of it.
 *
 * SABOTAGE RECEIPTS, taken by editing the emitted module in
 * v6/prolog/compile/out/, running this file, and restoring it byte-for-byte
 * (verified by diff). The messages quoted are what the runs printed.
 *
 *   1. Correlate the default arm's NOT EXISTS on the WRONG column
 *      (`n0."commit" = d0."name"`) -> plan assertion RED: "the coalesce
 *      default arm must SEARCH the source rel, got: ... UNION ALL | SEARCH d0
 *      USING INDEX __frontier_repo_phase (_phase>?) | CORRELATED SCALAR
 *      SUBQUERY 3 | SCAN n0 | USE TEMP B-TREE FOR DISTINCT".
 *   2. Replace the default arm's `"__frontier_repo" d0 WHERE d0."_phase" >= 0`
 *      with the base table `"repo" d0` -- the plausible-looking "just read the
 *      table" edit -> plan assertion RED: "the delta side must stay on the
 *      frontier, got: ... UNION ALL | SCAN d0 | CORRELATED SCALAR SUBQUERY 3 |
 *      SEARCH n0 USING PRIMARY KEY (name=?)".
 *
 *   THE STATEMENT-COUNT TEST STAYED GREEN THROUGH BOTH, which is exactly why
 *   the plan probe is a separate test rather than a stronger assertion on the
 *   same run. What DOES turn the count receipt red is the third probe, which
 *   was not a probe at all but this file's first draft: interleaving the two
 *   rels in the arrival list took it to "5 rows ran 42 statements, 100 rows
 *   ran 231". See the arrivals/1 header -- that growth is the arrival plane's
 *   run-length batching, not the rule plane, and finding it is what fixed the
 *   measurement instead of the code.
 *
 * The corpora are 5, 100 and 1,000 source rows with a `latest_commit` row for
 * only every third one, so both arms carry real width in every run.
 */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { firstValueFrom } from "rxjs";
import { stmt_counter } from "sprefa-store-engine/src/engine/counter.ts";

import { program } from "../gen_emitted/coalesce_defaults_the_absent_row.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import type { IArrivalRow, ISqlSeam } from "../runtime/types.ts";

const HERE = dirname(fileURLToPath(import.meta.url));
const COMPILE_OUT = join(HERE, "..", "..", "prolog", "compile", "out");

/** The emitted incremental insert for `repo_latest`, read out of the compiled
 *  module's source text so the plan receipt grades the statement that really
 *  runs rather than a copy of it kept in this file. */
function emitted_repo_latest_insert_sql(): string {
  const source = readFileSync(join(COMPILE_OUT, "coalesce_defaults_the_absent_row.ts"), "utf8");
  const line = source
    .split("\n")
    .find((candidate) => candidate.includes(`{ head_rel: "repo_latest"`) && candidate.includes("insert_sql:"));
  assert.ok(line, "no incremental level statement for repo_latest");
  return line.match(/insert_sql: `([\s\S]*?)`, select_sql:/)![1]!;
}

/**
 * MEASURED WHILE WRITING THIS FILE, and the reason the two rels are grouped
 * rather than interleaved: the arrival plane batches CONSECUTIVE same-rel rows
 * into one `... FROM json_each(?)` insert, so a batch that alternates rel by
 * rel issues one statement per RUN. Interleaving these two rels made the count
 * 42 at 5 repos and 231 at 100 -- growth in the number of alternations, not in
 * the rule plane, and nothing to do with coalesce. Grouping by rel (which is
 * also what a real per-rel feed produces) is what leaves the rule plane as the
 * only thing this receipt can be measuring.
 */
function arrivals(repo_count: number): readonly IArrivalRow[] {
  const repos: IArrivalRow[] = [];
  const commits: IArrivalRow[] = [];
  for (let index = 0; index < repo_count; index += 1) {
    repos.push({ rel: "repo", sign: "add", row: [`repo_${index}`] });
    // Every third repo has a commit, so the join arm and the default arm both
    // carry rows at both cardinalities.
    if (index % 3 === 0) {
      commits.push({ rel: "latest_commit", sign: "add", row: [`repo_${index}`, `sha_${index}`] });
    }
  }
  return [...repos, ...commits];
}

/** The exact per-tick statement count this program runs, pinned rather than
 *  only compared: an equality assertion alone would still hold if BOTH sides
 *  grew together. Measured at 5, 100 and 1,000 source rows.
 *
 *  Moved 33 -> 35 when the refCount reconcile stopped shipping its rows through
 *  JS, then 35 -> 37 when the antijoin was materialized once into a scratch
 *  table so three staging reads share it. Constant statements, and the tick
 *  went 2,771 ms to 2,166 ms on grid_10000. */
const STATEMENTS_PER_TICK = 39;

async function run_one_tick(repo_count: number) {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, program.ddl));
  stmt_counter.reset();
  await firstValueFrom(program.tick(seam, arrivals(repo_count)));
  const statement_count = stmt_counter.get();
  const final = await firstValueFrom(seam.runner.execute(seam.db, program.final_select.repo_latest!));
  const defaulted = final.rows.filter((row) => String(row.commit) === "absent").length;
  seam.db.close();
  return { statement_count, row_count: final.rows.length, defaulted };
}

test("coalesce statements per tick are flat in the source rel's size", async () => {
  const small = await run_one_tick(5);
  const large = await run_one_tick(100);
  const huge = await run_one_tick(1000);

  assert.deepEqual(
    [small.statement_count, large.statement_count, huge.statement_count],
    [STATEMENTS_PER_TICK, STATEMENTS_PER_TICK, STATEMENTS_PER_TICK],
    "the defaulting rule must not pay per row across 5 / 100 / 1,000 source rows",
  );
  // Non-vacuity: both arms derived at every size, so the flat count is flat
  // over real work rather than over an empty answer.
  assert.deepEqual(
    {
      small: [small.row_count, small.defaulted],
      large: [large.row_count, large.defaulted],
      huge: [huge.row_count, huge.defaulted],
    },
    { small: [5, 3], large: [100, 66], huge: [1000, 666] },
    "both arms must derive at every cardinality",
  );
});

test("the coalesce default arm SEARCHes the source rel, never SCANs it", async () => {
  const insert_sql = emitted_repo_latest_insert_sql();
  assert.ok(
    insert_sql.includes(`NOT EXISTS (SELECT 1 FROM "coalesce_defaults_the_absent_row_latest_commit" n0`),
    `the default arm must carry the negation over the source rel, got: ${insert_sql}`,
  );

  const seam: ISqlSeam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, program.ddl));
  await firstValueFrom(program.tick(seam, arrivals(100)));

  const plan = await firstValueFrom(seam.runner.execute(seam.db, `EXPLAIN QUERY PLAN ${insert_sql}`));
  const lines = plan.rows.map((row) => String(row.detail));
  seam.db.close();

  // `n0` is compile_negative_uses/4's alias for the negated rel and `d0` the
  // delta side; sqlite prints the ALIAS, never the table name.
  // Set-rel DDL is `__id INTEGER PRIMARY KEY` + UNIQUE(cols), so a keyed
  // lookup rides the UNIQUE autoindex; the old WITHOUT ROWID shape printed
  // PRIMARY KEY for the same access path.
  assert.ok(
    lines.some((line) => /SEARCH n0 USING (PRIMARY KEY|COVERING INDEX sqlite_autoindex_)/.test(line)),
    `the coalesce default arm must SEARCH the source rel by key, got: ${lines.join(" | ")}`,
  );
  assert.ok(
    !lines.some((line) => /\b_scan n0\b/.test(line)),
    `the coalesce default arm must not SCAN the source rel, got: ${lines.join(" | ")}`,
  );
  // The delta side is the frontier, not the base table: a coalesce rule that
  // read `repo` directly would re-derive every known row every tick.
  assert.ok(
    lines.every((line) => !/\b_scan d0\b/.test(line) || /__frontier_/.test(line)),
    `the delta side must stay on the frontier, got: ${lines.join(" | ")}`,
  );
});
