/**
 * listReadSurface.test.ts — the EXPLAIN and statement-count receipts for the
 * `__list_<entity>` read surface (lower.pl list_view_ddl/4, and the LEFT JOIN
 * delta_statement/3 adds). The sweep already grades the BYTES: every list
 * fixture is tick-log and final-state identical to the oracle in both doors.
 * What byte grading cannot see:
 *
 *   ORDER   whether the elements came out in idx order because the UNIQUE
 *           (list_id, idx) index supplied it, or because a temp b-tree sorted
 *           them. `json_group_array(... ORDER BY ...)` is sqlite 3.44+ and the
 *           system sqlite is 3.43.2, so the index IS the order and a plan that
 *           stopped using it would still pass the byte diff on small inputs.
 *   COST    whether the whole-rel render builds the view ONCE or re-executes
 *           it per row (repo law: formerly-quadratic paths get EXPLAIN or
 *           COUNT tests, never end-state equality alone).
 *   PUSHDOWN whether a point lookup on list_id reaches the member index or
 *           materializes every list in the program first.
 *
 * REJECTED-FORM RECEIPTS (both probed on system sqlite 3.43.2 against a
 * hand-built copy of the emitted schema before the view landed; each is the
 * shape this file's assertions exist to keep out):
 *   a. the element read written as the correlated `(SELECT s."content" FROM
 *      "__str" s WHERE s."__id" = m."value")` the `__txt_` views use:
 *      CORRELATED SCALAR SUBQUERY 5 | SEARCH s USING INTEGER PRIMARY KEY.
 *   b. the ordered-subquery form `SELECT list_id, json_group_array(value)
 *      FROM (SELECT ... ORDER BY list_id, idx) GROUP BY list_id`:
 *      CO-ROUTINE (subquery-3) | SCAN (subquery-3) | USE TEMP B-TREE FOR
 *      GROUP BY -- correct bytes, a sort the member index already had.
 *   c. `json_group_array(value ORDER BY idx)`: SYNTAX ERROR near "ORDER" on
 *      3.43.2. In-aggregate ORDER BY landed in 3.44.
 *
 * NAMED BLIND SPOT: assertion A plans the WHOLE boundary read, whose temp
 * b-tree and correlated subqueries belong to the delta GROUP BY and the
 * `__txt_` text decode, not to this view. The view's own plan is asserted
 * separately, against a SELECT over the view alone.
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import { concatMap, firstValueFrom, map } from "rxjs";

import { BootRunner } from "../runtime/2_boot.ts";
import { row_value_from_sql } from "../runtime/rows.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import { TickLogEmitter } from "../runtime/ticklog.ts";
import type { IBootStatement, IGenProgram, ISqlSeam } from "../runtime/types.ts";

import * as scalar_list from "../gen_emitted/split_value_is_the_interned_list_id.ts";
import * as rel_list from "../gen_emitted/recursive_list_arg_parent_holds_child_node_values.ts";

type EmittedProgram = IGenProgram & { readonly boot: readonly IBootStatement[] };

const SCALAR_ENTITY = "__gen__list_text_df210f232c1299bd";
const SCALAR_VIEW = `__list_${SCALAR_ENTITY}`;

function booted_seam(program: EmittedProgram): Promise<ISqlSeam> {
  const seam = ScratchStore.open(":memory:");
  return firstValueFrom(
    ScratchStore.boot(seam, program.ddl).pipe(concatMap(() => BootRunner.run(seam, program.boot))),
  ).then(() => seam);
}

function query_plan(seam: ISqlSeam, sql: string): Promise<string> {
  return firstValueFrom(
    seam.runner
      .execute(seam.db, `EXPLAIN QUERY PLAN ${sql}`)
      .pipe(map((result) => result.rows.map((row) => String(row["detail"])).join(" | "))),
  );
}

function boundary_read(plan: typeof scalar_list.incremental_plan, rel: string): string {
  const relation = plan.relations.find((entry) => entry.rel === rel);
  assert.ok(relation, `no relation plan for ${rel}`);
  return relation.boundary_sql;
}

// ── the surface is a VIEW, never a table ─────────────────────────────────────

test("the list read surface is a non-materialized TEMP VIEW over the member rel", () => {
  const ddl = (scalar_list.program as EmittedProgram).ddl;
  const view = ddl.find((line) => line.includes(`CREATE TEMP VIEW "${SCALAR_VIEW}"`));
  assert.ok(view, `no __list_ view in the emitted DDL: ${ddl.join(" | ")}`);
  assert.match(view, /json_group_array/, "the elements must aggregate in SQL, never in JS");
  assert.match(view, new RegExp(`GROUP BY m\\."list_id"$`), "the view groups by the member key");
  assert.ok(
    !ddl.some((line) => line.includes(`CREATE TABLE "${SCALAR_VIEW}"`)),
    "the read surface must add no table",
  );
  assert.ok(
    !view.includes("ORDER BY"),
    "in-aggregate ORDER BY is 3.44+; the UNIQUE (list_id, idx) index is the order",
  );
});

// ── EXPLAIN, whole-rel render ────────────────────────────────────────────────

test("plan: the view itself rides the member index with no temp b-tree", async () => {
  const seam = await booted_seam(scalar_list.program as EmittedProgram);
  const plan = await query_plan(seam, `SELECT "list_id", "value_text" FROM "${SCALAR_VIEW}"`);
  assert.match(
    plan,
    /SCAN m USING INDEX sqlite_autoindex___gen__list_text_[0-9a-f]+__member_1/,
    `the member scan must drive the aggregate, got: ${plan}`,
  );
  assert.ok(
    !/USE TEMP B-TREE FOR GROUP BY/.test(plan),
    `the grouping must ride the member index with no temp b-tree, got: ${plan}`,
  );
  assert.ok(
    !/CORRELATED SCALAR SUBQUERY/.test(plan),
    `the element read is a JOIN, never a correlated subquery, got: ${plan}`,
  );
});

test("plan: the whole-rel render builds the view once and probes it by key", async () => {
  const seam = await booted_seam(scalar_list.program as EmittedProgram);
  const sql = boundary_read(scalar_list.incremental_plan, "row_parts");
  assert.ok(sql.includes(`LEFT JOIN "${SCALAR_VIEW}"`), `the boundary read must join the view: ${sql}`);
  const plan = await query_plan(seam, sql.replace(/\?/g, "1"));
  assert.match(plan, /MATERIALIZE __list___gen__list_text_/, `the view is built once, got: ${plan}`);
  assert.equal(
    (plan.match(/__list___gen__list_text_/g) ?? []).length,
    1,
    `one build, never one per row, got: ${plan}`,
  );
  assert.match(
    plan,
    /SEARCH __l_parts USING AUTOMATIC COVERING INDEX \(list_id=\?\)/,
    `the join probes the built view by key, got: ${plan}`,
  );
});

// ── EXPLAIN, point lookup ────────────────────────────────────────────────────

test("plan: a point lookup on list_id pushes through the GROUP BY into the member index", async () => {
  const seam = await booted_seam(scalar_list.program as EmittedProgram);
  const plan = await query_plan(seam, `SELECT "value_text" FROM "${SCALAR_VIEW}" WHERE "list_id" = 1`);
  assert.match(plan, /CO-ROUTINE __list___gen__list_text_/, `no materialization for a keyed read, got: ${plan}`);
  assert.match(
    plan,
    /SEARCH m USING INDEX sqlite_autoindex___gen__list_text_[0-9a-f]+__member_1 \(list_id=\?\)/,
    `the constraint must reach the member index, got: ${plan}`,
  );
});

// ── COST: the render is one statement, flat in the row count ─────────────────

test("cost: the boundary render of N rows is ONE statement, not one per row", async () => {
  const program = scalar_list.program as EmittedProgram;
  const seam = await booted_seam(program);
  const sql = boundary_read(scalar_list.incremental_plan, "row_parts");
  assert.equal(
    (sql.match(/\bSELECT\b/g) ?? []).length,
    2,
    `one boundary SELECT plus the text-decode CASE's json_each probe, got: ${sql}`,
  );
  assert.equal(
    (sql.match(new RegExp(`"${SCALAR_VIEW}"`, "g")) ?? []).length,
    1,
    `the view is joined once, never once per column read: ${sql}`,
  );
});

// ── a rel-typed element reads the target's memoized rendering ────────────────

test("a rel element aggregates the target view's __rendered, not its id", () => {
  const ddl = (rel_list.program as EmittedProgram).ddl;
  const view = ddl.find((line) => line.startsWith('CREATE TEMP VIEW "__list___gen__list_node_'));
  assert.ok(view, `no __list_ view for a rel-element list: ${ddl.join(" | ")}`);
  assert.match(view, /json\(r\."__rendered"\)/, `the element is the target's value, got: ${view}`);
  assert.match(view, /LEFT JOIN "__ref_node" r ON r\."__id" = m\."value"/, `a join, not a probe: ${view}`);
});

// F3: the reader hands the consumer Array<T>, never the array text.

test("the row reader parses a list column into an array at the seam", () => {
  assert.deepEqual(row_value_from_sql("list", '["usr","local","bin"]'), ["usr", "local", "bin"]);
  assert.deepEqual(row_value_from_sql("list", "[]"), []);
  assert.deepEqual(row_value_from_sql("list", '[{"name":"ada"}]'), [{ name: "ada" }]);
});

// FAIL-PRE-FIX: with the array text crossing as a plain string, a consumer
// reading the `Array<string>` typegen promises got a string, and `.length`
// counted CHARACTERS.
test("the row reader names a list column whose text is not an array", () => {
  assert.throws(() => row_value_from_sql("list", '{"a":1}'), /non-array text/);
  assert.throws(() => row_value_from_sql("list", 7), /crossed SQLite/);
});

test("the tick-log encoder canonicalizes an already-parsed list, keys sorted", () => {
  assert.equal(TickLogEmitter.value_text(["usr", "local"], "list"), '["usr","local"]');
  assert.equal(
    TickLogEmitter.value_text([{ url: "ada.io", name: "ada" } as never], "list"),
    '[{"name":"ada","url":"ada.io"}]',
  );
});
