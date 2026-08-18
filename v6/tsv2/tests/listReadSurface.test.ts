/**
 * listReadSurface.test.ts — the EXPLAIN and statement-count receipts for the
 * `__list_<entity>` read surface (lower.pl list_view_ddl/4, and the LEFT JOIN
 * delta_statement/3 adds). The sweep already grades the BYTES: every list
 * fixture is tick-log and final-state identical to the oracle in both doors.
 * What byte grading cannot see:
 *
 *   ORDER   whether the elements come out in idx order from an explicit
 *           ordered subquery. `json_group_array(... ORDER BY ...)` is sqlite
 *           3.44+ and the system sqlite is 3.43.2, so the view orders its
 *           input rows before aggregation.
 *   COST    whether the whole-rel boundary stays flattened into one entity
 *           primary-key lookup plus one keyed member lookup per owner row.
 *   PUSHDOWN whether a point lookup on list_id reaches the member index or
 *           materializes every list in the program first.
 *
 * REJECTED-FORM RECEIPTS (both probed on system sqlite 3.43.2 against a
 * hand-built copy of the emitted schema before the view landed; each is the
 * shape this file's assertions exist to keep out):
 *   a. the element read written as the correlated `(SELECT s."content" FROM
 *      "__str" s WHERE s."__id" = m."value")` the `__txt_` views use:
 *      CORRELATED SCALAR SUBQUERY 5 | SEARCH s USING INTEGER PRIMARY KEY.
 *   b. `json_group_array(value ORDER BY idx)`: SYNTAX ERROR near "ORDER" on
 *      3.43.2. In-aggregate ORDER BY landed in 3.44.
 *
 * NAMED BLIND SPOT: assertion A plans the WHOLE boundary read, whose temp
 * b-tree and correlated subqueries belong to the delta GROUP BY and the
 * `__txt_` text decode, not to this view. The view's own plan is asserted
 * separately, against a SELECT over the view alone.
 */

import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { test } from "node:test";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { concatMap, firstValueFrom, map, toArray } from "rxjs";

import { BootRunner } from "../runtime/2_boot.ts";
import { row_value_from_sql } from "../runtime/rows.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import { TickLogEmitter } from "../runtime/ticklog.ts";
import { TickFold } from "../runtime/tickLoop.ts";
import type { IArrivalBatch, IBootStatement, IGenProgram, ISqlSeam } from "../runtime/types.ts";

import * as scalar_list from "../gen_emitted/split_value_is_the_interned_list_id.ts";
import * as rel_list from "../gen_emitted/recursive_list_arg_parent_holds_child_node_values.ts";

type EmittedProgram = IGenProgram & {
  readonly boot: readonly IBootStatement[];
  readonly final_select: Readonly<Record<string, string>>;
};

// The rel names are what a schedule names; the table names carry the
// compilation unit's module prefix (compile.pl relation_storage_names/4).
const SCALAR_MODULE = "split_value_is_the_interned_list_id";
const SCALAR_ENTITY = "__gen__list_text_df210f232c1299bd";
const SCALAR_TABLE = `${SCALAR_MODULE}_${SCALAR_ENTITY}`;
const SCALAR_VIEW = `__list_${SCALAR_TABLE}`;
const NODE_MODULE = "recursive_list_arg_parent_holds_child_node_values";
const NODE_ENTITY = "__gen__list_node_4205b0871c875897";
const NODE_MEMBER = `${NODE_ENTITY}__member`;
const NODE_VIEW = `__list_${NODE_MODULE}_${NODE_ENTITY}`;

function booted_seam(program: EmittedProgram, path = ":memory:", temporary_only = false): Promise<ISqlSeam> {
  const seam = ScratchStore.open(path);
  const ddl = temporary_only ? program.ddl.filter((statement) => statement.startsWith("CREATE TEMP")) : program.ddl;
  return firstValueFrom(
    ScratchStore.boot(seam, ddl).pipe(concatMap(() => BootRunner.run(seam, program.boot))),
  ).then(() => seam);
}

function run_schedule(
  program: EmittedProgram,
  seam: ISqlSeam,
  schedule: readonly IArrivalBatch[],
): Promise<readonly string[]> {
  return firstValueFrom(TickFold.run(program, seam, schedule).pipe(toArray()));
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

test("the list read surface is a non-materialized TEMP VIEW over member rows", () => {
  const ddl = (scalar_list.program as EmittedProgram).ddl;
  const view = ddl.find((line) => line.includes(`CREATE TEMP VIEW "${SCALAR_VIEW}"`));
  assert.ok(view, `no __list_ view in the emitted DDL: ${ddl.join(" | ")}`);
  assert.match(view, /json_group_array/, "the elements must aggregate in SQL, never in JS");
  assert.match(
    view,
    /FROM "[a-z_0-9]*__gen__list_text_[0-9a-f]+__member" m0 WHERE NOT EXISTS \(SELECT 1 FROM "[a-z_0-9]*__gen__list_text_[0-9a-f]+__member" m1 WHERE m1\."list_id" = m0\."list_id" AND m1\."idx" < m0\."idx"\)$/,
    "one outer row per list id, taken off the member plane so the view stays flattenable",
  );
  assert.ok(
    !ddl.some((line) => line.includes(`CREATE TABLE "${SCALAR_VIEW}"`)),
    "the read surface must add no table",
  );
  assert.match(view, /ORDER BY m\."idx"/, "the aggregate input is explicitly ordered");
});

// ── EXPLAIN, whole-rel render ────────────────────────────────────────────────

test("plan: a keyed view read orders members through the member index", async () => {
  const seam = await booted_seam(scalar_list.program as EmittedProgram);
  const plan = await query_plan(seam, `SELECT "list_id", "value_text" FROM "${SCALAR_VIEW}"`);
  assert.match(
    plan,
    /SEARCH m USING INDEX sqlite_autoindex_[a-z_0-9]*__gen__list_text_[0-9a-f]+__member_1 \(list_id=\?\)/,
    `the correlated aggregate must use the keyed member index, got: ${plan}`,
  );
});

test("generated ticks preserve order, duplicates, empty lists, deletion, and restart", async () => {
  const directory = mkdtempSync(join(tmpdir(), "list-persistence-"));
  const path = `file:${join(directory, "lists.sqlite")}`;
  try {
    const program = scalar_list.program as EmittedProgram;
    const initial = await booted_seam(program, path);
    await run_schedule(program, initial, [
      [{ rel: "row_text", sign: "add", row: ["ordered", "beta/alpha/beta"] }],
      [{ rel: SCALAR_ENTITY, sign: "add", row: ["[]"] }],
    ]);
    const before_restart = await firstValueFrom(
      initial.runner.execute(initial.db, `SELECT "list_id", "value_text" FROM "${SCALAR_VIEW}" ORDER BY "list_id"`),
    );
    assert.ok(
      before_restart.rows.some((row) => row.value_text === '["beta","alpha","beta"]'),
      `generated list members must retain order and duplicate values: ${JSON.stringify(before_restart.rows)}`,
    );
    const empty = await firstValueFrom(
      initial.runner.execute(
        initial.db,
        `SELECT coalesce((SELECT "value_text" FROM "${SCALAR_VIEW}" WHERE "list_id" = -1), '[]') AS "value_text"`,
      ),
    );
    assert.equal(
      empty.rows[0]?.value_text,
      "[]",
      `a list id with no member rows reads as the empty list through the boundary coalesce: ${JSON.stringify(empty.rows)}`,
    );
    initial.db.close();

    // Persistent tables survive; TEMP DDL and the generated boot closure are
    // recreated before the delete tick, matching a process restart.
    const reopened = await booted_seam(program, path, true);
    const after_restart = await firstValueFrom(
      reopened.runner.execute(reopened.db, `SELECT "list_id", "value_text" FROM "${SCALAR_VIEW}" ORDER BY "list_id"`),
    );
    assert.ok(
      after_restart.rows.some((row) => row.value_text === '["beta","alpha","beta"]'),
      `restart must read the durable ordered list: ${JSON.stringify(after_restart.rows)}`,
    );
    await run_schedule(program, reopened, [
      [{ rel: "row_text", sign: "del", row: ["ordered", "beta/alpha/beta"] }],
    ]);
    const public_after_delete = await firstValueFrom(
      reopened.runner.execute(reopened.db, program.final_select['row_parts']!),
    );
    assert.ok(
      !public_after_delete.rows.some((row) => row.name === "ordered"),
      `the generated owner row must retract on source deletion: ${JSON.stringify(public_after_delete.rows)}`,
    );
    reopened.db.close();

    const after_delete_restart = await booted_seam(program, path, true);
    const public_after_delete_restart = await firstValueFrom(
      after_delete_restart.runner.execute(after_delete_restart.db, program.final_select['row_parts']!),
    );
    assert.ok(
      !public_after_delete_restart.rows.some((row) => row.name === "ordered"),
      `deletion must survive restart: ${JSON.stringify(public_after_delete_restart.rows)}`,
    );
    after_delete_restart.db.close();
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("plan: the whole-rel render stays flattened and keyed", async () => {
  const seam = await booted_seam(scalar_list.program as EmittedProgram);
  const sql = boundary_read(scalar_list.incremental_plan, "row_parts");
  assert.ok(sql.includes(`LEFT JOIN "${SCALAR_VIEW}"`), `the boundary read must join the view: ${sql}`);
  const plan = await query_plan(seam, sql.replace(/\?/g, "1"));
  assert.match(
    plan,
    /SEARCH m0 USING COVERING INDEX sqlite_autoindex_[a-z_0-9]*__gen__list_text_[0-9a-f]+__member_1 \(list_id=\?\)/,
    `the id side must seek the member index, got: ${plan}`,
  );
  assert.match(
    plan,
    /SEARCH m USING INDEX sqlite_autoindex_[a-z_0-9]*__gen__list_text_[0-9a-f]+__member_1 \(list_id=\?\)/,
    `the correlated member aggregate must use the list_id index, got: ${plan}`,
  );
  assert.doesNotMatch(
    plan,
    /MATERIALIZE __list_[a-z_0-9]*__gen__list_text_|SCAN m USING INDEX sqlite_autoindex_[a-z_0-9]*__gen__list_text_[0-9a-f]+__member_1/,
    `the boundary must not materialize every list or scan every member, got: ${plan}`,
  );
});

// ── EXPLAIN, point lookup ────────────────────────────────────────────────────

test("plan: a point lookup keeps the ordered member scan keyed", async () => {
  const seam = await booted_seam(scalar_list.program as EmittedProgram);
  const plan = await query_plan(seam, `SELECT "value_text" FROM "${SCALAR_VIEW}" WHERE "list_id" = 1`);
  assert.doesNotMatch(plan, /CO-ROUTINE __list_[a-z_0-9]*__gen__list_text_/, `the keyed view is flattened, got: ${plan}`);
  assert.match(
    plan,
    /SEARCH m0 USING COVERING INDEX sqlite_autoindex_[a-z_0-9]*__gen__list_text_[0-9a-f]+__member_1 \(list_id=\?\)/,
    `the id lookup must be keyed, got: ${plan}`,
  );
  assert.match(
    plan,
    /SEARCH m USING INDEX sqlite_autoindex_[a-z_0-9]*__gen__list_text_[0-9a-f]+__member_1 \(list_id=\?\)/,
    `the ordered input must use the member index with list_id pushdown, got: ${plan}`,
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
  const view = ddl.find((line) => line.startsWith(`CREATE TEMP VIEW "${NODE_VIEW}"`));
  assert.ok(view, `no __list_ view for a rel-element list: ${ddl.join(" | ")}`);
  assert.match(view, /json\(r\."__rendered"\)/, `the element is the target's value, got: ${view}`);
  assert.match(
    view,
    new RegExp(`LEFT JOIN "__ref_${NODE_MODULE}_node" r ON r\\."__id" = m\\."value"`),
    `a join, not a probe: ${view}`,
  );
});

test("generated nested structured elements survive a restart", async () => {
  const directory = mkdtempSync(join(tmpdir(), "nested-list-persistence-"));
  const path = `file:${join(directory, "lists.sqlite")}`;
  try {
    const program = rel_list.program as EmittedProgram;
    const initial = await booted_seam(program, path);
    await run_schedule(program, initial, [
      [{ rel: NODE_ENTITY, sign: "add", row: ['[{"name":"leaf","children":1}]'] }],
      [
        {
          rel: NODE_MEMBER,
          sign: "add",
          row: [1, 0, { name: "leaf", children: 1 } as unknown as string],
        },
      ],
    ]);
    const before_restart = await firstValueFrom(
      initial.runner.execute(initial.db, `SELECT "value_text" FROM "${NODE_VIEW}" WHERE "list_id" = 1`),
    );
    assert.deepEqual(before_restart.rows, [{ value_text: '[{"name":"leaf","children":1}]' }]);
    initial.db.close();

    const reopened = await booted_seam(program, path, true);
    const after_restart = await firstValueFrom(
      reopened.runner.execute(reopened.db, `SELECT "value_text" FROM "${NODE_VIEW}" WHERE "list_id" = 1`),
    );
    assert.deepEqual(after_restart.rows, [{ value_text: '[{"name":"leaf","children":1}]' }]);
    reopened.db.close();
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
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
