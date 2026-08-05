// perf-n1.test.mjs — node --test coverage for the DL_PERF_LOG N+1 aggregator.
//
// Fixture line shapes are copied faithfully from the real emitters:
//   serve shape: v6/tsv2/serve/0_trace.ts + runtime/types.ts (IServeTickLine,
//                IServeRuleEvent { rule, rows, wall_ms }), real bytes from
//                v6/tsv2/goldens/trace-line.jsonl.
//   dl shape:    v6/dl/src/0_trace.ts + 0_types.ts (PerfTickLine, PerfBindEntry
//                { rel, rows, ms }).
// Each fixture comment names the source file its shape came from.

import assert from "node:assert/strict";
import { test } from "node:test";

import { computeSuspect, processJsonlLines, renderRows } from "./perf-n1.mjs";

test("aggregation sums statements/rows/wall_ms across lines of one (tick, unit)", () => {
  // Serve shape. Source: v6/tsv2/goldens/trace-line.jsonl (IServeTickLine,
  // v6/tsv2/serve/0_trace.ts). Two lines share (tick=1, rule R); each field sums.
  const lines = [
    '{"tick":1,"rels":2,"rows":8,"statements":10,"rules":[{"rule":"prog:R/1#1","rows":5,"wall_ms":10}],"effects":[],"binds":[],"watches":[]}',
    '{"tick":1,"rels":2,"rows":4,"statements":7,"rules":[{"rule":"prog:R/1#1","rows":3,"wall_ms":4}],"effects":[],"binds":[],"watches":[]}',
  ];

  const { rows } = processJsonlLines(lines, 10);
  assert.equal(rows.length, 1);
  assert.equal(rows[0].tick, 1);
  assert.equal(rows[0].unit, "prog:R/1#1");
  assert.equal(rows[0].statements, 2);
  assert.equal(rows[0].rows, 8);
  assert.equal(rows[0].wall_ms, 14);
});

test("suspect fires on a synthetic N+1 fixture (50 statements, 50 rows)", () => {
  // Serve shape, v6/tsv2/serve/0_trace.ts (IServeTickLine). Real N+1 shape: 50
  // rules[] elements of one row each, since one element = one executed statement.
  const ruleEvents = Array.from({ length: 50 }, () => '{"rule":"prog:R/1#1","rows":1,"wall_ms":1}').join(",");
  const line = `{"tick":3,"rels":1,"rows":50,"statements":50,"rules":[${ruleEvents}],"effects":[],"binds":[],"watches":[]}`;

  const { rows } = processJsonlLines([line], 10);
  assert.equal(rows.length, 1);
  assert.equal(rows[0].statements, 50);
  assert.equal(rows[0].rows, 50);
  assert.equal(computeSuspect(rows[0].statements, rows[0].rows, 10), 1);
  assert.ok(renderRows(rows, 10).includes("\t1"));
});

test("suspect stays 0 on a batched fixture (1 statement, 50 rows)", () => {
  // Serve shape. Source: v6/tsv2/serve/0_trace.ts (IServeTickLine). Batched:
  // one statement covers 50 rows, so statements < rows.
  const line =
    '{"tick":4,"rels":1,"rows":50,"statements":1,"rules":[{"rule":"prog:R/1#1","rows":50,"wall_ms":20}],"effects":[],"binds":[],"watches":[]}';

  const { rows } = processJsonlLines([line], 10);
  assert.equal(rows.length, 1);
  assert.equal(computeSuspect(rows[0].statements, rows[0].rows, 10), 0);
});

test("edb lines aggregate by normalized sql shape", () => {
  // Standalone EDB shape: {seam:"edb", sql, ms} from v6/dl/src/0_trace.ts
  // onSqlMessage; digits collapse to ? so per-row statements share one unit.
  const lines = [
    '{"level":30,"time":1,"seam":"edb","sql":"SELECT kind FROM relbase_node WHERE path = 7","ms":0.2}',
    '{"level":30,"time":1,"seam":"edb","sql":"SELECT kind FROM relbase_node WHERE path = 9","ms":0.3}',
  ];

  const { rows } = processJsonlLines(lines, 10);
  assert.equal(rows.length, 1);
  assert.equal(rows[0].tick, "-");
  assert.equal(rows[0].unit, "SELECT kind FROM relbase_node WHERE path = ?");
  assert.equal(rows[0].statements, 2);
  assert.equal(rows[0].wall_ms, 0.5);
});

test("malformed lines are skipped and counted, valid lines still aggregate", () => {
  // dl shape: PerfTickLine + PerfBindEntry { rel, rows, ms } from
  // v6/dl/src/0_trace.ts + v6/dl/src/0_types.ts. Served beside a shape-less
  // valid-JSON line and a non-JSON line; those two are skipped and counted.
  const lines = [
    '{"tick":1,"wall_ms":12.34,"stmt_count":8,"stmt_ms_total":9.9,"stmt_ms_max":3.2,"effects":[],"binds":[{"rel":"clock","rows":50,"ms":1.5}],"ingest":null,"rss_kb":12345}',
    '{"tick":1,"wall_ms":3, "stmt_count":5, "effects": [], "binds": [{"rel":"clock","rows":2,"ms":0.5}], "ingest": null, "rss_kb":9}',
    '{"hello":"not a trace shape"}',
    "not json at all",
  ];

  const { rows, skipCount } = processJsonlLines(lines, 10);
  assert.equal(skipCount, 2);
  assert.equal(rows.length, 1);
  assert.equal(rows[0].unit, "clock");
  assert.equal(rows[0].statements, 2);
  assert.equal(rows[0].rows, 52);
});

test("standard envelope (any line with actor and seam) parses as one statement per line, rows/ms mapped", () => {
  // Standard envelope: one fixture line per emitter: dl (bind seam),
  // tsv2 serve (host seam), tsv2 runtime (sql seam).
  const lines = [
    '{"actor":"dl.runtime","seam":"bind","unit":"clock_bucket","rows":5,"ms":1.5,"tick":1}',
    '{"actor":"tsv2.serve","seam":"host","unit":"weigh","rows":3,"ms":2.0,"tick":2}',
    '{"actor":"tsv2.runtime","seam":"sql","unit":"prog:R/1#1","rows":7,"ms":4.0,"tick":3}',
  ];

  const { rows, skipCount } = processJsonlLines(lines, 10);
  assert.equal(skipCount, 0);
  assert.equal(rows.length, 3);
  for (const row of rows) assert.equal(row.statements, 1);
  assert.deepEqual(
    rows.find((row) => row.unit === "weigh"),
    { tick: 2, unit: "weigh", statements: 1, rows: 3, wall_ms: 2.0 },
  );
  assert.deepEqual(
    rows.find((row) => row.unit === "clock_bucket"),
    { tick: 1, unit: "clock_bucket", statements: 1, rows: 5, wall_ms: 1.5 },
  );
});

test("standard envelope rows/ms aggregate across lines of one (tick, unit)", () => {
  const lines = [
    '{"actor":"tsv2.runtime","seam":"sql","unit":"prog:R/1#1","rows":7,"ms":4.0,"tick":3}',
    '{"actor":"tsv2.runtime","seam":"sql","unit":"prog:R/1#1","rows":2,"ms":1.0,"tick":3}',
  ];

  const { rows } = processJsonlLines(lines, 10);
  assert.equal(rows.length, 1);
  assert.equal(rows[0].statements, 2);
  assert.equal(rows[0].rows, 9);
  assert.equal(rows[0].wall_ms, 5.0);
});
