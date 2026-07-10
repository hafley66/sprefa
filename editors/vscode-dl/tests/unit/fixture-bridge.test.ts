// Unit coverage for tests/fixture-bridge.mjs's SQL pattern matcher. This is
// the one piece of standalone JS in this arc worth unit-testing without a
// browser: flow-panel.html itself stays untouched (owned by another agent),
// so panel internals aren't reachable outside a real page -- see
// tests/README.md's "follow-up" note.
import { describe, expect, it } from "vitest";
import { resolveQuery } from "../fixture-bridge.mjs";

describe("resolveQuery", () => {
  it("lists every rel_ table for the bare LIKE 'rel_%' schema scan", () => {
    const rows = resolveQuery(
      "SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'rel_%'"
    );
    const names = rows.map((r) => r[0]).sort();
    expect(names).toEqual([
      "rel_demo_edge",
      "rel_demo_node",
      "rel_type_entity",
      "rel_type_link",
    ]);
  });

  it("lists only _node tables for the layer-discovery scan", () => {
    const rows = resolveQuery(
      "SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'rel_%_node' ORDER BY name"
    );
    expect(rows).toEqual([["rel_demo_node"]]);
  });

  it("confirms a matching _edge table exists", () => {
    const rows = resolveQuery(
      "SELECT name FROM sqlite_master WHERE type='table' AND name='rel_demo_edge'"
    );
    expect(rows).toEqual([["rel_demo_edge"]]);
  });

  it("reports zero for an edge table that doesn't exist", () => {
    const rows = resolveQuery(
      "SELECT name FROM sqlite_master WHERE type='table' AND name='rel_nope_edge'"
    );
    expect(rows).toEqual([]);
  });

  it("counts the builtin type-layer pair via name IN (...)", () => {
    const rows = resolveQuery(
      "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('rel_type_entity','rel_type_link')"
    );
    expect(rows).toEqual([[2]]);
  });

  it("returns column names for PRAGMA table_info", () => {
    const rows = resolveQuery("PRAGMA table_info(rel_demo_node)");
    const cols = rows.map((r) => r[1]);
    expect(cols).toEqual(["id", "label", "kind", "file", "line"]);
  });

  it("answers a plain row query against a known table", () => {
    const rows = resolveQuery("SELECT id, label, kind FROM rel_demo_node LIMIT 600");
    expect(rows).toHaveLength(2);
    expect(rows[0][0]).toBe("demo-a");
  });

  it("unions rows across a UNION ALL over two known tables", () => {
    const rows = resolveQuery(
      "SELECT sym, sym, 'x' FROM rel_type_entity UNION ALL SELECT src, src, 'y' FROM rel_type_link"
    );
    expect(rows.length).toBe(2 + 1);
  });

  it("returns an empty result for SQL against an unknown table", () => {
    const rows = resolveQuery("SELECT op, fn FROM rel_op_reach_fn");
    expect(rows).toEqual([]);
  });
});
