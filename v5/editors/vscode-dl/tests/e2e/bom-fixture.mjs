// Canned rel_bom_node/rel_bom_edge pair for the "exploded" list-mode e2e
// coverage (Track C, C3). Loaded via `node tests/fixture-bridge.mjs --bom`
// (mirrors how tests/perf/big-graph-fixture.mjs's perfTables() is loaded via
// --perf-count) -- kept OUT of the default TABLES map in fixture-bridge.mjs
// on purpose: rel_bom_node/rel_bom_edge end in `_node`/`_edge`, so adding
// them to the SHARED fixture panel.spec.ts's tests run against would make
// discoverLayers() find a 3rd layer pair and break its "exactly 2 layer
// chips" assertion. This pair is served on its own dedicated port instead
// (see playwright.config.ts's third webServer entry + explode.spec.ts).
//
// rel_bom_node's columns are the ALREADY-JOINED shape PRESETS.bomTable's SQL
// projects in production (bom_node LEFT JOIN bom_tier LEFT JOIN bom_weld,
// see .dl/bom.dl + media/flow-panel.html): sym, name, kind, file, line,
// parent, member_count, fan_in, fan_out, weight, tier, sccRep, sccSize.
// tests/fixture-bridge.mjs's resolveQuery only regex-matches the literal
// `FROM rel_bom_node` (a LEFT JOIN is not a second `FROM`), so it returns
// this table's rows verbatim -- the join is pre-baked here rather than
// executed by the fixture, same simplification the rest of TABLES relies on
// (see resolveQuery's doc comment).
//
// Shape: three strata (tier 0/1/2, a straight top->middle->foundation
// dependency chain) plus one independent 2-file cycle at tier 0
// (cycle_a.rs <-> cycle_b.rs, rep = cycle_a.rs, size 2) -- exercises
// tier-major ordering AND the welded-card collapse in one fixture.
export function bomTables() {
  return {
    rel_bom_node: {
      cols: [
        "sym", "name", "kind", "file", "line", "parent",
        "member_count", "fan_in", "fan_out", "weight", "tier", "sccRep", "sccSize",
      ],
      rows: [
        ["found::fn::f", "f", "function", "foundation.rs", 1, "", 0, 1, 0, 3, 0, "foundation.rs", 1],
        ["mid::fn::g", "g", "function", "middle.rs", 1, "", 0, 1, 1, 4, 1, "middle.rs", 1],
        ["top::fn::h", "h", "function", "top.rs", 1, "", 0, 0, 1, 5, 2, "top.rs", 1],
        ["cyclea::fn::p", "p", "function", "cycle_a.rs", 1, "", 0, 1, 1, 2, 0, "cycle_a.rs", 2],
        ["cycleb::fn::q", "q", "function", "cycle_b.rs", 1, "", 0, 1, 1, 2, 0, "cycle_a.rs", 2],
      ],
    },
    rel_bom_edge: {
      cols: ["src", "dst", "kind"],
      rows: [
        ["top::fn::h", "mid::fn::g", "calls"],
        ["mid::fn::g", "found::fn::f", "calls"],
        ["cyclea::fn::p", "cycleb::fn::q", "calls"],
        ["cycleb::fn::q", "cyclea::fn::p", "calls"],
      ],
    },
    // Empty on purpose: PRESETS.bomTable's node query LEFT JOINs these two
    // tables by name (see flow-panel.html), and updatePresetAvailability()'s
    // presetRels() regex-scans the WHOLE preset SQL string for any `rel_\w+`
    // token -- LEFT JOIN counts same as FROM -- so the "BOM table" preset
    // option stays disabled ("needs .dl") unless both names resolve via
    // sqlite_master, even though resolveQuery()'s row-fetch path only ever
    // reads the literal `FROM rel_bom_node` (the join is pre-baked into
    // rel_bom_node's own tier/sccRep/sccSize columns above). Rows stay empty:
    // nothing here is ever actually queried.
    rel_bom_tier: { cols: ["file", "tier"], rows: [] },
    rel_bom_weld: { cols: ["file", "rep", "size"], rows: [] },
  };
}
