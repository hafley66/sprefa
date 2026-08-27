/**
 * reloadPlan.test.ts: receipts for the reload planner that diffs two __rel
 * catalogs (the running program's rows against a freshly compiled module's).
 *
 * KEYED BY the parent local_name chain, NOT by h_id and NOT by module_id.
 * rel_h_id hashes the module hash, the name AND the arity (v6/prolog/7_lower/lower.pl
 * rel_h_id/4), so adding a column moves h_id. Keying on h_id therefore read a
 * column addition as "the old rel vanished, a stranger arrived": a unsupported
 * construct plus a create, for a routine edit. module_id is positional in the
 * emitted id sequence, so keying on it read any id-layout shift (a new
 * primitive, a synthetic list row) as drop-everything with identical rels.
 * MEASURED on one file compiled three ways, same module name:
 *   2 cols                 h_id 975158fcc67ee6af  h_schema 76bdfb91a44e84a1
 *   to_path text -> int    h_id 975158fcc67ee6af  h_schema fa2a45594779d533
 *   + weight: int          h_id be1425dc796502d5  h_schema 225740de73633ea1
 * Those are the fixtures below, so the recreate cases differ in the field the
 * compiler actually moves rather than in a hand-invented hash.
 *
 * Only kind === "rel" rows are compared. A column reshuffle reaches the parent
 * row as an h_schema change (schema_hash covers Columns/ColumnTypes/KeyOrNone),
 * so column rows carry no separate verdict.
 *
 * h_schema encodes the table shape; h_rule encodes the derivation (sorted rule
 * bodies), is "" for a source rel, and moves only when a rule body moves. That
 * split is what lets one path say `recreate` (DROP then CREATE, data lost) and
 * another say `refill` (DELETE, data recomputed in place).
 *
 * SABOTAGE RECEIPTS, every one RUN and reverted, with the tests each turned red:
 *   1. `verdicts.set(key, "create")` -> "keep": cold boot, a new rel creates,
 *      a renamed rel is a drop plus a create.
 *   2. the h_schema comparison -> `false`: all four recreate cases.
 *   3. the h_rule comparison -> `false`: a rule body change refills, and the
 *      reshape-plus-rule-change case.
 *   4. `verdicts.set(key, "keep")` -> "create": an unchanged program, a new rel
 *      creates, only rel rows are compared.
 *   5. `if (next_rels.has(key)) continue;` -> `continue;`: all three drop cases.
 *   6. the `unsupported.push` template -> a literal: the refused-by-name case.
 *   7. rel_key returns row.h_id: all 13, since every keyed lookup then misses.
 *      The semantic half is the unsupported construct assertion in "a column added
 *      recreates", which is the defect this keying exists to prevent.
 *
 * An eighth sabotage was RUN and DELETED THE CODE IT TESTED: the condition
 * carried `prev_row.h_id !== next_row.h_id ||` before the h_schema check, and
 * removing that half failed NOTHING. It was unreachable. schema_hash covers the
 * column list, so an arity change always moves h_schema, and h_id differing
 * while h_schema matches cannot happen for one (module, name) pair.
 */
import assert from "node:assert/strict";
import { test } from "node:test";

import type { IRelCatalogRow } from "../runtime/types.ts";
import { ReloadPlanner } from "../serve/reloadPlan.ts";

function rel(row: {
  readonly local_name: string;
  readonly arity: number;
  readonly h_id: string;
  readonly h_schema: string;
  readonly h_rule: string;
}): IRelCatalogRow {
  return {
    rel_id: 0,
    parent_id: 6,
    ordinal: 0,
    local_name: row.local_name,
    kind: "rel",
    type_id: 0,
    arity: row.arity,
    module_id: 6,
    h_id: row.h_id,
    h_schema: row.h_schema,
    h_rule: row.h_rule,
  };
}

const EDGE: IRelCatalogRow = rel({
  local_name: "edge",
  arity: 2,
  h_id: "975158fcc67ee6af",
  h_schema: "76bdfb91a44e84a1",
  h_rule: "",
});
const EDGE_TYPE_CHANGED: IRelCatalogRow = rel({ ...EDGE, h_schema: "fa2a45594779d533" });
const EDGE_COLUMN_ADDED: IRelCatalogRow = rel({
  ...EDGE,
  arity: 3,
  h_id: "be1425dc796502d5",
  h_schema: "225740de73633ea1",
});
const REACH: IRelCatalogRow = rel({
  local_name: "reach",
  arity: 2,
  h_id: "fbdcdb48481fdfb8",
  h_schema: "76bdfb91a44e84a1",
  h_rule: "8a1c0d5e6f7b2a34",
});

const EDGE_KEY = "edge";
const REACH_KEY = "reach";

test("cold boot is all create", () => {
  const plan = ReloadPlanner.plan([], [EDGE], false);
  assert.equal(plan.verdicts.get(EDGE_KEY), "create");
  assert.equal(plan.statements.length, 0);
  assert.equal(plan.unsupported.length, 0);
});

test("an id-layout shift with identical rels keeps everything", () => {
  const shifted: IRelCatalogRow = { ...EDGE, rel_id: 9, parent_id: 7, module_id: 7 };
  const plan = ReloadPlanner.plan([EDGE], [shifted], false);
  assert.equal(plan.verdicts.size, 1);
  assert.equal(plan.verdicts.get(EDGE_KEY), "keep");
  assert.equal(plan.statements.length, 0);
  assert.equal(plan.unsupported.length, 0);
});

test("an unchanged program keeps everything", () => {
  const plan = ReloadPlanner.plan([EDGE, REACH], [EDGE, REACH], false);
  assert.equal(plan.verdicts.size, 2);
  assert.equal(plan.verdicts.get(EDGE_KEY), "keep");
  assert.equal(plan.verdicts.get(REACH_KEY), "keep");
  assert.equal(plan.statements.length, 0);
  assert.equal(plan.unsupported.length, 0);
});

test("a column added recreates", () => {
  const plan = ReloadPlanner.plan([EDGE], [EDGE_COLUMN_ADDED], false);
  assert.equal(plan.verdicts.get(EDGE_KEY), "recreate");
  assert.deepEqual(plan.statements, ['DROP TABLE IF EXISTS "edge"']);
  assert.equal(plan.unsupported.length, 0, "a column addition is not a drop");
});

test("a column dropped recreates", () => {
  const plan = ReloadPlanner.plan([EDGE_COLUMN_ADDED], [EDGE], false);
  assert.equal(plan.verdicts.get(EDGE_KEY), "recreate");
  assert.deepEqual(plan.statements, ['DROP TABLE IF EXISTS "edge"']);
  assert.equal(plan.unsupported.length, 0);
});

test("a type change recreates", () => {
  const plan = ReloadPlanner.plan([EDGE], [EDGE_TYPE_CHANGED], false);
  assert.equal(plan.verdicts.get(EDGE_KEY), "recreate");
  assert.deepEqual(plan.statements, ['DROP TABLE IF EXISTS "edge"']);
});

test("a key change recreates", () => {
  const keyed: IRelCatalogRow = rel({ ...EDGE, h_schema: "3c9f1b6a2d8e4507" });
  const plan = ReloadPlanner.plan([EDGE], [keyed], false);
  assert.equal(plan.verdicts.get(EDGE_KEY), "recreate");
  assert.deepEqual(plan.statements, ['DROP TABLE IF EXISTS "edge"']);
});

test("a rule body change refills", () => {
  const reedited: IRelCatalogRow = rel({ ...REACH, h_rule: "0b4d7e2f9a1c6538" });
  const plan = ReloadPlanner.plan([REACH], [reedited], false);
  assert.equal(plan.verdicts.get(REACH_KEY), "refill");
  assert.deepEqual(plan.statements, ['DELETE FROM "reach"']);
});

test("a new rel creates", () => {
  const plan = ReloadPlanner.plan([EDGE], [EDGE, REACH], false);
  assert.equal(plan.verdicts.get(EDGE_KEY), "keep");
  assert.equal(plan.verdicts.get(REACH_KEY), "create");
  assert.equal(plan.statements.length, 0);
});

test("a renamed rel is a drop plus a create", () => {
  const renamed: IRelCatalogRow = rel({ ...EDGE, local_name: "edges", h_id: "1f2e3d4c5b6a7908" });
  const plan = ReloadPlanner.plan([EDGE], [renamed], true);
  assert.equal(plan.verdicts.get("edges"), "create");
  assert.equal(plan.verdicts.get(EDGE_KEY), "drop");
  assert.deepEqual(plan.statements, ['DROP TABLE IF EXISTS "edge"']);
});

test("a drop without allow-drop is refused by name", () => {
  const plan = ReloadPlanner.plan([EDGE], [], false);
  assert.equal(plan.verdicts.has(EDGE_KEY), false);
  assert.equal(plan.statements.length, 0);
  assert.deepEqual(plan.unsupported, ["rel_drop_needs_allow_drop(edge)"]);
});

test("a drop with allow-drop drops", () => {
  const plan = ReloadPlanner.plan([EDGE], [], true);
  assert.equal(plan.verdicts.get(EDGE_KEY), "drop");
  assert.deepEqual(plan.statements, ['DROP TABLE IF EXISTS "edge"']);
  assert.equal(plan.unsupported.length, 0);
});

test("a reshape and a rule change in one load", () => {
  const reedited: IRelCatalogRow = rel({ ...REACH, h_rule: "0b4d7e2f9a1c6538" });
  const plan = ReloadPlanner.plan([EDGE, REACH], [EDGE_COLUMN_ADDED, reedited], false);
  assert.equal(plan.verdicts.get(EDGE_KEY), "recreate");
  assert.equal(plan.verdicts.get(REACH_KEY), "refill");
  assert.ok(plan.statements.includes('DROP TABLE IF EXISTS "edge"'));
  assert.ok(plan.statements.includes('DELETE FROM "reach"'));
  assert.equal(plan.statements.length, 2);
});

test("only rel rows are compared", () => {
  const column: IRelCatalogRow = {
    ...EDGE,
    kind: "column",
    local_name: "from_path",
    parent_id: 7,
    ordinal: 1,
    h_id: "8e573bdafd8b831d",
  };
  const primitive: IRelCatalogRow = { ...EDGE, kind: "primitive", local_name: "text", h_id: "" };
  const plan = ReloadPlanner.plan([EDGE, column, primitive], [EDGE], false);
  assert.equal(plan.verdicts.size, 1);
  assert.equal(plan.verdicts.get(EDGE_KEY), "keep");
  assert.equal(plan.unsupported.length, 0, "an ignored row cannot be refused as a drop");
});
