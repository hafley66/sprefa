/**
 * relationDepth.test.ts — plan receipts for relation values nested more than
 * one level deep.
 *
 * WHY A PLAN TEST AND NOT AN END-STATE TEST. The end state is already graded:
 * conformance/fixtures/6_relation_depth.pl runs eight of these programs
 * against the oracle and the sweep replays the emitted modules byte-for-byte
 * in both emitter modes. What that grade cannot see is HOW the rows were
 * reached. The defect this arc fixed compiled to
 * `json_extract(b1."repo", '$.fn') = 'repo'` against an INTEGER endpoint --
 * a condition that is simply never true -- and the shape that replaced it is
 * a join chain whose whole justification is that each hop lands on an index.
 * A chain that answered the same rows by scanning the dictionary at every
 * level would pass every fixture in the corpus and be the wrong lowering
 * (count-test law: a formerly-broken path gets a plan assertion, never
 * end-state equality alone).
 *
 * WHAT IS ASSERTED, per statement:
 *   0. EVERY ROW SOURCE, per delta arm, by name and in FROM order. This is the
 *      assertion the first version of this file did not have, and its absence
 *      is how a real regression walked past every other check here: a head
 *      value that IS a body atom had its endpoint interned anyway, so the
 *      target table was joined to a TEMP VIEW over itself on every value
 *      column. That extra source is a dictionary and is reached by SEARCH, so
 *      the dictionary-name and access-method assertions below were both happy.
 *      An access method is not a count; the count is asserted here.
 *   1. ONE HOP PER LEVEL, per delta arm. A depth-N read names exactly the N
 *      dictionary views on the path, each once. A level the rule's own body
 *      already reads is NOT one of them -- its endpoint is that atom's `__id`
 *      -- which is why a depth-2 construction names two views and not three.
 *   2. NO SCAN, ANYWHERE IN THE PLAN. Not "no scan of a dictionary": SQLite
 *      FLATTENS `__ref_<type>` (a plain view over the target table) into the
 *      base table and reports the view's own alias `t`, so the plan text never
 *      contains the view name at all and the claim has to be made about the
 *      access METHOD. On these statements the driving side is a delta frontier
 *      with its own index, so zero SCAN lines is the honest bar.
 *   3. ONE INTEGER PRIMARY KEY LOOKUP PER LEVEL on the destructure statements,
 *      where every hop keys on `__id`. A construction keys on the target's
 *      identity UNIQUE instead (the id is the OUTPUT of that lookup), so its
 *      receipt is 2 plus the per-level indexed access.
 *   4. NO json_extract OVER AN ALIASED COLUMN. The exact text of the old
 *      defect, pinned so a regression is caught at the string and not only at
 *      the plan.
 *
 * SABOTAGE RECEIPTS, both run at the fix commit and reverted:
 *
 *   lower.pl expand_relation_pattern_rules/4 forced to a no-op (the pre-fix
 *   behaviour). "depth 2 construction" and "depth 2 destructure" both go red
 *   on assertion 1 -- zero `__ref_` views where three and two are required.
 *   The depth-3 tests stay green, because their gen_emitted modules were not
 *   recompiled: an incomplete sabotage, recorded as such.
 *
 *   lower.pl elide_dictionary_atoms_the_body_already_joins/4 forced to a
 *   no-op and the two construction modules recompiled (run 2026-07-30,
 *   reverted). Both construction tests go red on the view list, verbatim:
 *     actual: [ '__ref_file', '__ref_fpath', '__ref_repo' ]
 *     expected: [ '__ref_fpath', '__ref_repo' ]
 *     actual: [ '__ref_file', '__ref_fpath', '__ref_repo', '__ref_span' ]
 *     expected: [ '__ref_file', '__ref_fpath', '__ref_repo' ]
 *   Both destructure tests stay green, correctly: nothing about a READ moved.
 *
 *   intern_relation_value/6's memo lookup forced to always fail. Every
 *   conformance fixture and the whole sweep stay GREEN -- the rows are the
 *   same rows -- while the depth-2 `span` insert grows a second `__ref_repo`
 *   and a second `__ref_fpath` join (5 per arm instead of 3), which
 *   assertion 1 catches. That is precisely the class end-state equality is
 *   blind to, and the reason the hop count is asserted at all.
 *
 * NAMED BLIND SPOT: `supportSql`, `recomputeSql` and the naive-mode arm of the
 * SAME rules are not planned here. They are built from the same rewritten rule
 * body by the same compiler, and the sweep grades both modes for row equality,
 * but nothing in this file would notice if only the incremental insert kept
 * its indexes.
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import { concatMap, firstValueFrom, map } from "rxjs";

import { BootRunner } from "../runtime/2_boot.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import { TickFold } from "../runtime/tickLoop.ts";
import type { IArrivalBatch, IBootStatement, IGenProgram, ISqlSeam } from "../runtime/types.ts";

import * as depth2 from "../gen_emitted/relation_depth2_construct_and_read.ts";
import * as depth3 from "../gen_emitted/relation_depth3_construct_and_read.ts";

type EmittedProgram = IGenProgram & { readonly boot: readonly IBootStatement[] };

const DEPTH2_ARRIVAL: readonly IArrivalBatch[] = [
  [{ rel: "raw", sign: "add", row: ["acme", "src/a.rs", 10, 20] as unknown as string[] }],
];
const DEPTH3_ARRIVAL: readonly IArrivalBatch[] = [
  [{ rel: "rawk", sign: "add", row: ["acme", "src/a.rs", 10, 20, "def"] as unknown as string[] }],
];

/** Boot, then run the arrival so the tables hold real rows: an EXPLAIN over an
 *  empty schema still returns a plan, but a plan taken against populated
 *  tables is the one the runtime actually executes. */
async function seamWithRows(
  program: EmittedProgram,
  schedule: readonly IArrivalBatch[],
): Promise<ISqlSeam> {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(
    ScratchStore.boot(seam, program.ddl).pipe(concatMap(() => BootRunner.run(seam, program.boot))),
  );
  await firstValueFrom(TickFold.run(program, seam, schedule));
  return seam;
}

function insertSqlFor(plan: { readonly levels: readonly { readonly headRel: string; readonly insertSql: string }[] }, rel: string): string {
  const level = plan.levels.find((entry) => entry.headRel === rel);
  assert.ok(level, `no emitted level statement for ${rel}`);
  return level.insertSql;
}

/** The distinct dictionary views one statement joins. */
function dictionaryViews(sql: string): string[] {
  const names = new Set<string>();
  for (const match of sql.matchAll(/"(__ref_[a-z_]+)"/g)) names.add(match[1]!);
  return [...names].sort();
}

/** Aliased dictionary joins PER DELTA ARM, duplicates included, so a lowering
 *  that joins the same dictionary twice in one arm is visible. A semi-naive
 *  insert is a UNION ALL with one arm per changed body atom, and each arm
 *  legitimately repeats the whole chain, so the total is divided by the arm
 *  count rather than compared raw. */
function dictionaryJoinsPerArm(sql: string): number {
  const arms = (sql.match(/ UNION ALL /g) ?? []).length + 1;
  const joins = (sql.match(/"__ref_[a-z_]+" b\d+/g) ?? []).length;
  assert.equal(joins % arms, 0, `every delta arm must carry the same chain: ${sql}`);
  return joins / arms;
}

/**
 * EVERY row source this statement names, per delta arm, in FROM order.
 *
 * Counting `__ref_` views alone is structurally blind to an extra ORDINARY
 * table, and that blindness let a real regression through: a depth-1
 * construction whose head value IS a body atom
 * (`selected(user(Id, Name)) <- user(Id, Name)`) went from
 * `FROM "user" b0` to `FROM "user" b0, "__ref_user" b1` -- the table joined to
 * a view of itself on every value column -- and every dictionary-name and
 * access-method assertion in this file stayed green, because the extra source
 * IS a dictionary and IS reached by SEARCH. What changed was the number of
 * tables, so that is what is asserted now, name by name (count-test law: the
 * receipt has to be able to see the thing that moved).
 */
function rowSourcesPerArm(sql: string): string[][] {
  return sql.split(" UNION ALL ").map((arm) => {
    const fromClause = arm.slice(arm.indexOf(' FROM "'));
    return [...fromClause.matchAll(/"([a-z_0-9]+)" [bdrn]\d+/g)].map((match) => match[1]!);
  });
}

function queryPlan(seam: ISqlSeam, sql: string): Promise<string[]> {
  return firstValueFrom(
    seam.runner
      .execute(seam.db, `EXPLAIN QUERY PLAN ${sql}`)
      .pipe(map((result) => result.rows.map((row) => String(row["detail"])))),
  );
}

/** SQLite FLATTENS `__ref_<type>` (a plain view over the target table) into
 *  the base table and reports the view's own alias, `t`. So the plan text
 *  never contains the view name and the claim has to be made about the access
 *  METHOD instead: every table this statement touches is reached by SEARCH,
 *  with at least one such SEARCH per dictionary hop. A SCAN line anywhere is
 *  the failure -- there is no level of this chain that has to be scanned. */
function assertEveryHopIsIndexed(plan: readonly string[], sql: string, hops: number): void {
  const scans = plan.filter((line) => line.startsWith("SCAN"));
  assert.deepEqual(scans, [], `no level of the chain may be scanned: ${plan.join(" | ")}`);
  const searches = plan.filter((line) => line.startsWith("SEARCH"));
  assert.ok(
    searches.length >= hops,
    `expected at least ${hops} indexed hops, got ${searches.length}: ${plan.join(" | ")} for ${sql}`,
  );
}

/** The sharpest form of the receipt, available where every hop keys on `__id`
 *  (a destructure walks parent -> child through the endpoint): exactly one
 *  INTEGER PRIMARY KEY lookup per level. */
function rowidSearchCount(plan: readonly string[]): number {
  return plan.filter((line) => /SEARCH \w+ USING INTEGER PRIMARY KEY \(rowid=\?\)/.test(line)).length;
}

function assertNoJsonExtractOverRefColumn(sql: string): void {
  // The exact defect: a ref column is an INTEGER endpoint, and json_extract of
  // an integer is NULL. Any json_extract whose first argument is an aliased
  // column (rather than `value`, which is json_each's own row) is the old
  // shape coming back.
  assert.ok(
    !/json_extract\(b\d+\./.test(sql) && !/json_extract\(d\d+\./.test(sql),
    `no ref column may be read with json_extract: ${sql}`,
  );
}

// ── depth 2 ──────────────────────────────────────────────────────────────────

test("depth 2 construction joins one dictionary per level, all indexed", async () => {
  const seam = await seamWithRows(depth2.program as EmittedProgram, DEPTH2_ARRIVAL);
  const sql = insertSqlFor(depth2.incrementalPlan, "span");

  // TWO dictionary views, not three: the `file` level is the rule's own
  // target-membership atom, so its endpoint is that atom's `__id` and a
  // `__ref_file` join would be `file` joined to a view of itself.
  assert.deepEqual(
    dictionaryViews(sql),
    ["__ref_fpath", "__ref_repo"],
    `span builds file(repo, fpath): the two nested levels, each once. got: ${sql}`,
  );
  assert.equal(dictionaryJoinsPerArm(sql), 2, `one join per nested level, no repeats: ${sql}`);
  assert.deepEqual(
    rowSourcesPerArm(sql),
    [
      ["__frontier_raw", "file", "__ref_repo", "__ref_fpath"],
      ["__frontier_file", "file", "raw", "__ref_repo", "__ref_fpath"],
    ],
    `every table each arm names, no extras: ${sql}`,
  );
  assertNoJsonExtractOverRefColumn(sql);
  assertEveryHopIsIndexed(await queryPlan(seam, sql), sql, 3);
});

test("depth 2 destructure reads two hops, both keyed on __id", async () => {
  const seam = await seamWithRows(depth2.program as EmittedProgram, DEPTH2_ARRIVAL);
  const sql = insertSqlFor(depth2.incrementalPlan, "coord");

  // `span(file(_, fpath(Path)), Start, End)` reaches fpath THROUGH file, and
  // never touches repo: the `_` in the repo position must cost no join.
  assert.deepEqual(
    dictionaryViews(sql),
    ["__ref_file", "__ref_fpath"],
    `the wildcard repo position must cost no join. got: ${sql}`,
  );
  assert.equal(dictionaryJoinsPerArm(sql), 2, `one join per level, no repeats: ${sql}`);
  assert.deepEqual(
    rowSourcesPerArm(sql),
    [["__frontier_span", "__ref_fpath", "__ref_file"]],
    `every table the arm names, no extras: ${sql}`,
  );
  assertNoJsonExtractOverRefColumn(sql);
  const plan = await queryPlan(seam, sql);
  assertEveryHopIsIndexed(plan, sql, 2);
  assert.equal(rowidSearchCount(plan), 2, `two levels, two PK lookups: ${plan.join(" | ")}`);
});

// ── depth 3 ──────────────────────────────────────────────────────────────────

test("depth 3 destructure reads three hops, all keyed on __id", async () => {
  const seam = await seamWithRows(depth3.program as EmittedProgram, DEPTH3_ARRIVAL);
  const sql = insertSqlFor(depth3.incrementalPlan, "found");

  assert.deepEqual(
    dictionaryViews(sql),
    ["__ref_file", "__ref_fpath", "__ref_span"],
    `located -> span -> file -> fpath is three hops. got: ${sql}`,
  );
  assert.equal(dictionaryJoinsPerArm(sql), 3, `one join per level, no repeats: ${sql}`);
  assert.deepEqual(
    rowSourcesPerArm(sql),
    [["__frontier_located", "__ref_fpath", "__ref_file", "__ref_span"]],
    `every table the arm names, no extras: ${sql}`,
  );
  assertNoJsonExtractOverRefColumn(sql);
  const plan = await queryPlan(seam, sql);
  assertEveryHopIsIndexed(plan, sql, 3);
  assert.equal(rowidSearchCount(plan), 3, `three levels, three PK lookups: ${plan.join(" | ")}`);
});

test("depth 3 construction interns each level once", async () => {
  const seam = await seamWithRows(depth3.program as EmittedProgram, DEPTH3_ARRIVAL);
  const sql = insertSqlFor(depth3.incrementalPlan, "located");

  // THREE views for four levels, same reason as depth 2: the outermost level
  // (`span`) is the rule's own target-membership atom.
  assert.deepEqual(
    dictionaryViews(sql),
    ["__ref_file", "__ref_fpath", "__ref_repo"],
    `span(file(repo, fpath), Start, End): the three nested levels. got: ${sql}`,
  );
  assert.deepEqual(
    rowSourcesPerArm(sql),
    [
      ["__frontier_rawk", "span", "__ref_repo", "__ref_fpath", "__ref_file"],
      ["__frontier_span", "span", "rawk", "__ref_repo", "__ref_fpath", "__ref_file"],
    ],
    `every table each arm names, no extras: ${sql}`,
  );
  assertNoJsonExtractOverRefColumn(sql);
  // A construction keys on the target's identity UNIQUE rather than on __id
  // (the id is the OUTPUT of the lookup), so the receipt here is the absence
  // of scans plus one indexed access per level per compound arm.
  assertEveryHopIsIndexed(await queryPlan(seam, sql), sql, 4);
});
