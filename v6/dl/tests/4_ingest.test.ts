/**
 * tests/4_ingest.test.ts - M3 gate: extract subprocess -> spine EDB rels -> per-file
 * re-ingest DIFF. Min tests, max coverage per the repo's test law:
 *   1. the F2 mapping (toFactLines), one record per shape, hand-built + a real fixture
 *      read for the span_line byte-offset index.
 *   2. same-file-twice idempotence (zero deltas on the identical second ingest).
 *   3. GOLDEN: an edited file's re-ingest retracts+inserts only for that path, proven
 *      against a control file that is never touched by the edit.
 *   4. delete-file: a vanished path retracts everything ingested for it.
 *   5. extract exit 1 on a real (non-ENOENT) failure is thrown, not swallowed.
 *
 * Reuses tests/1_helpers_db.ts's bootFixture/cleanupDbFile (never duplicated here).
 */
import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";

import { extractFile, ingestFile, spineDeclsLocal, toFactLines } from "../src/4_ingest.ts";
import type { ExtractRecord, Row } from "../tasks.d.ts";
import { bootFixture, cleanupDbFile } from "./1_helpers_db.ts";

const CORPUS_DIR = path.join(import.meta.dirname, "..", "fixtures", "corpus");
const BAD_TS = path.join(CORPUS_DIR, "bad.ts");
const BAD_V2_TS = path.join(CORPUS_DIR, "bad_v2.ts");
const GOLDEN_PATH = path.join(import.meta.dirname, "..", "fixtures", "golden", "ingest.bad-v1-v2.json");

function tempCorpusPath(): string {
  return path.join(os.tmpdir(), `dl-m3-ingest-${crypto.randomBytes(8).toString("hex")}.ts`);
}

function byJson(a: unknown, b: unknown): number {
  const left = JSON.stringify(a);
  const right = JSON.stringify(b);
  return left < right ? -1 : left > right ? 1 : 0;
}

/** Every spine rel as EDB, no rules — every ingest test boots a runtime against this
 *  (see the file-header comment on spineDeclsLocal in 4_ingest.ts: integration replaces
 *  this with 5_diag.ts's spineDecls/builtinDecls). */
function spineProgram() {
  return { rels: spineDeclsLocal, rules: [] };
}

test("F2 mapping: one record per shape maps to spine rel rows + span_line + file", () => {
  // Hand-built, one of each ExtractRecord shape. Offsets are real byte offsets in
  // fixtures/corpus/bad.ts (147/158/167 sit on line 3, `  console.log(message);`,
  // bytes 145-169 — verified against the real extract binary's own output for this
  // fixture); the file read here is only to build the byte-offset -> {line,col} index.
  const records: readonly ExtractRecord[] = [
    { record: "node", family: "cst", span: { start: 147, end: 167 }, kind: "call_expression", name: null },
    { record: "edge", family: "cst", kind: "child", from: { start: 0, end: 189 }, to: { start: 0, end: 188 } },
    { record: "sig", family: "type", owner: { start: 7, end: 188 }, slot: "param", pos: 0, ty: "string" },
    { record: "site", family: "call", span: { start: 147, end: 158 }, callee: "log", callee_path: null },
    {
      record: "const",
      family: "type",
      owner: { start: 54, end: 79 },
      field: null,
      text: "`hello ${name}`",
      kind: "template",
    },
  ];

  const lines = toFactLines(records, BAD_TS);
  const expectedContentHash = crypto.createHash("sha256").update(fs.readFileSync(BAD_TS)).digest("hex");

  assert.deepEqual(lines, [
    { t: "rel", name: "file", row: [BAD_TS, expectedContentHash] },
    { t: "rel", name: "node", row: [BAD_TS, "cst", 147, 167, "call_expression", null] }, // null name covered
    { t: "rel", name: "edge", row: [BAD_TS, "cst", "child", 0, 189, 0, 188] },
    { t: "rel", name: "sig", row: [BAD_TS, 7, 188, "param", 0, "string"] },
    { t: "rel", name: "site", row: [BAD_TS, 147, 158, "log", null] }, // null callee_path covered
    { t: "rel", name: "const", row: [BAD_TS, 54, 79, null, "`hello ${name}`", "template"] },
    // span_line: one row per DISTINCT offset from the node+site spans above (147, 158,
    // 167), sorted ascending. Line 3 (0-based) is `  console.log(message);`, byte-start
    // 145, so col = offset - 145.
    { t: "rel", name: "span_line", row: [BAD_TS, 147, 3, 2] },
    { t: "rel", name: "span_line", row: [BAD_TS, 158, 3, 13] },
    { t: "rel", name: "span_line", row: [BAD_TS, 167, 3, 22] },
  ]);
});

// M8-alpha (IdentityWitnessLaw, tasks.d.ts): the `file` row's content_hash is the
// witness a salted probe joins against: these two tests are the witness's own
// EXIST (this one) and FLOW (the next one) half.
test("file row holds the sha-256 hex digest of the file's bytes", async () => {
  const { rt, dbPath } = await bootFixture(spineProgram());
  try {
    await ingestFile(rt, BAD_TS);
    const fileRows = ((await rt.rows("file")) as Row[]).filter((row) => row.path === BAD_TS);
    assert.equal(fileRows.length, 1);
    const expectedHash = crypto.createHash("sha256").update(fs.readFileSync(BAD_TS)).digest("hex");
    assert.equal(fileRows[0]!.content_hash, expectedHash);
  } finally {
    await rt.dispose();
    cleanupDbFile(dbPath);
  }
});

// The witness heartbeat: editing a file's CONTENT (same path) retracts the old
// content_hash row and inserts the new one. Note this nets to 0 in TickReport.changed
// (src/3_runtime.ts applyEdbTxn: net = genuinelyNew.length - allRetracted.length, both
// 1 here); the net summary is not where the heartbeat shows. It shows on the per-tick
// delta STREAM (rt.deltas$), which reports the raw insert/retract sets regardless of
// net, so this test asserts against that stream directly instead of TickReport.
test("editing content flips the file row: old content_hash retracts, new one inserts (witness heartbeat)", async () => {
  const { rt, dbPath } = await bootFixture(spineProgram());
  const tempPath = tempCorpusPath();
  try {
    fs.copyFileSync(BAD_TS, tempPath);
    await ingestFile(rt, tempPath);
    const hashV1 = crypto.createHash("sha256").update(fs.readFileSync(BAD_TS)).digest("hex");
    assert.deepEqual(
      ((await rt.rows("file")) as Row[]).filter((row) => row.path === tempPath),
      [{ path: tempPath, content_hash: hashV1 }],
    );

    fs.copyFileSync(BAD_V2_TS, tempPath);
    const fileDeltaEvents: { inserts: readonly Row[]; retracts: readonly Row[] }[] = [];
    const subscription = rt.deltas$.subscribe((event) => {
      if (event.rel === "file") fileDeltaEvents.push({ inserts: event.inserts, retracts: event.retracts });
    });
    const reportEdit = await ingestFile(rt, tempPath);
    subscription.unsubscribe();

    const hashV2 = crypto.createHash("sha256").update(fs.readFileSync(BAD_V2_TS)).digest("hex");
    assert.notEqual(hashV1, hashV2, "test sanity: the two fixture bodies must actually differ");
    assert.ok(
      !reportEdit.changed.some(([rel]) => rel === "file"),
      "the net TickReport delta for `file` is 0 on a same-path content edit (1 insert, 1 retract)",
    );

    assert.equal(fileDeltaEvents.length, 1, "expected exactly one `file` delta event for the edit tick");
    assert.deepEqual(fileDeltaEvents[0]!.retracts, [{ path: tempPath, content_hash: hashV1 }]);
    assert.deepEqual(fileDeltaEvents[0]!.inserts, [{ path: tempPath, content_hash: hashV2 }]);

    assert.deepEqual(
      ((await rt.rows("file")) as Row[]).filter((row) => row.path === tempPath),
      [{ path: tempPath, content_hash: hashV2 }],
    );
  } finally {
    await rt.dispose();
    cleanupDbFile(dbPath);
    fs.rmSync(tempPath, { force: true });
  }
});

test("same file ingested twice = second TickReport has zero deltas", async () => {
  const { rt, dbPath } = await bootFixture(spineProgram());
  try {
    const reportA = await ingestFile(rt, BAD_TS);
    assert.ok(reportA.changed.length > 0, "first ingest of a real file must change something");
    const changedRels = new Set(reportA.changed.map(([rel]) => rel));
    for (const rel of ["node", "edge", "const", "site", "span_line", "file"]) {
      assert.ok(changedRels.has(rel), `expected '${rel}' to appear in the first ingest's changed set`);
    }
    for (const [, delta] of reportA.changed) assert.ok(delta > 0, "first ingest of a new file is all inserts");

    const reportB = await ingestFile(rt, BAD_TS);
    assert.deepEqual(reportB.changed, [], "identical re-ingest of the same file must be a zero-delta noop");
  } finally {
    await rt.dispose();
    cleanupDbFile(dbPath);
  }
});

// Regression context (found while writing this test, fixed at M3 integration): the
// runtime's existence pre-check and physical DELETE originally used
// `WHERE (cols) IN (VALUES ...)`, which never matches a stored row with a NULL column
// (SQL row-value comparison yields unknown, not true). node.name is null for every real
// record in bad.ts, so per-file retraction silently dropped rows. Fixed in
// src/3_runtime.ts sqlAnyRowMatch (NULL-safe `col IS val` groups); this golden and the
// delete-file test below are the standing regressions for it.
test("edited file: per-file retract+insert only for that path (golden)", async () => {
  const { rt, dbPath } = await bootFixture(spineProgram());
  const tempPath = tempCorpusPath();
  try {
    // Control file: a DIFFERENT path, ingested once and never touched again. Proves the
    // per-file diff never moves another file's rows.
    await ingestFile(rt, BAD_TS);
    const controlNodeRowsBefore = ((await rt.rows("node")) as Row[])
      .filter((row) => row.path === BAD_TS)
      .sort(byJson);

    fs.copyFileSync(BAD_TS, tempPath);
    const reportA = await ingestFile(rt, tempPath);
    assert.ok(
      reportA.changed.some(([rel, delta]) => rel === "node" && delta > 0),
      "first ingest of the temp copy must insert node rows",
    );

    fs.copyFileSync(BAD_V2_TS, tempPath); // bad_v2.ts = bad.ts minus the console.log line
    const reportB = await ingestFile(rt, tempPath);

    const nodeDelta = reportB.changed.find(([rel]) => rel === "node");
    assert.ok(nodeDelta !== undefined && nodeDelta[1] < 0, "expected a negative node delta (removed console.log call)");
    const spanLineDelta = reportB.changed.find(([rel]) => rel === "span_line");
    assert.ok(spanLineDelta !== undefined && spanLineDelta[1] < 0, "expected a negative span_line delta");

    const controlNodeRowsAfter = ((await rt.rows("node")) as Row[])
      .filter((row) => row.path === BAD_TS)
      .sort(byJson);
    assert.deepEqual(
      controlNodeRowsAfter,
      controlNodeRowsBefore,
      "the control file's rows must be untouched by editing+re-ingesting a different path",
    );

    const snapshot = [reportA.changed, reportB.changed];
    if (process.env.REGEN_GOLDEN === "1") {
      fs.mkdirSync(path.dirname(GOLDEN_PATH), { recursive: true });
      fs.writeFileSync(GOLDEN_PATH, `${JSON.stringify(snapshot, null, 2)}\n`);
    }
    const expected: unknown = JSON.parse(fs.readFileSync(GOLDEN_PATH, "utf8"));
    assert.deepEqual(snapshot, expected);
  } finally {
    await rt.dispose();
    cleanupDbFile(dbPath);
    fs.rmSync(tempPath, { force: true });
  }
});

// Regression pair of the golden test above (same NULL-safe-match root cause, fixed at
// M3 integration): a delete-file ingest must retract null-column rows completely.
test("delete-file: a missing path retracts everything ingested for it", async () => {
  const { rt, dbPath } = await bootFixture(spineProgram());
  const tempPath = tempCorpusPath();
  try {
    fs.copyFileSync(BAD_TS, tempPath);
    const reportA = await ingestFile(rt, tempPath);
    assert.ok(reportA.changed.length > 0);

    fs.rmSync(tempPath);
    const reportB = await ingestFile(rt, tempPath);
    assert.ok(reportB.changed.length > 0, "deleting an ingested file must retract something");
    for (const [rel, delta] of reportB.changed) assert.ok(delta < 0, `expected '${rel}' delta to be a full retraction`);

    for (const decl of spineDeclsLocal) {
      const remaining = ((await rt.rows(decl.name)) as Row[]).filter((row) => row.path === tempPath);
      assert.deepEqual(remaining, [], `rel '${decl.name}' must have zero rows left for the deleted path`);
    }
  } finally {
    await rt.dispose();
    cleanupDbFile(dbPath);
  }
});

test("extract exit 1 on a real (non-ENOENT) failure is thrown", async () => {
  const { rt, dbPath } = await bootFixture(spineProgram());
  try {
    // A directory path: fs.statSync succeeds (so this is NOT the ENOENT delete-file
    // case), but extract itself exits 1 ("Is a directory") — verified empirically
    // against the real DL_EXTRACT_BIN before writing this test.
    await assert.rejects(() => ingestFile(rt, CORPUS_DIR));
  } finally {
    await rt.dispose();
    cleanupDbFile(dbPath);
  }
});

test("extractFile: real extract binary emits exactly 79 records for bad.ts, spawn-only smoke", async () => {
  const records: ExtractRecord[] = [];
  for await (const record of extractFile(BAD_TS)) records.push(record);
  assert.equal(records.length, 79);
  assert.ok(
    records.some(
      (record) =>
        record.record === "node" &&
        record.kind === "call_expression" &&
        record.span.start === 147 &&
        record.span.end === 167,
    ),
    "expected the console.log call_expression node at the known span",
  );
});

// M9-before regression (2026-07-24): the OLD src/3_runtime.ts sqlAnyRowMatch WHERE clause
// built one `(col IS lit AND ...)` group per candidate row, OR-joined -- the SQL
// expression-tree size scaled with candidate_rows x columns for a single rel's insert
// batch in a single commit. This build's bundled libsql/sqlite3 compiles in
// SQLITE_LIMIT_EXPR_DEPTH=1000 (`PRAGMA compile_options` reports MAX_EXPR_DEPTH=1000), so
// any ordinary file whose single-rel row count cleared roughly 150-250 rows crashed
// ingestFile with SQLITE_ERROR "Expression tree is too large". node_modules/rxjs/src/
// index.ts -- an ordinary ~11KB barrel/re-export file the pinned rxjs dependency ships,
// no fixture copy needed -- was the file that surfaced it: 1079 node rows, 1078 edge
// rows, 1783 span_line rows in one ingestFile commit. Fixed by replacing sqlAnyRowMatch
// with a shared temp-table JOIN (tree depth O(columns), independent of row count); this
// is the standing regression that pins the file/row-count combination which used to
// crash and must keep succeeding.
test("regression (M9-before): ingesting a real ~1800-span_line-row file no longer trips the SQL expression-tree depth ceiling", async () => {
  const { rt, dbPath } = await bootFixture(spineProgram());
  const rxjsIndexPath = path.join(import.meta.dirname, "..", "node_modules", "rxjs", "src", "index.ts");
  try {
    const report = await ingestFile(rt, rxjsIndexPath);
    const spanLineDelta = report.changed.find(([rel]) => rel === "span_line");
    assert.ok(spanLineDelta !== undefined, "expected a span_line delta from ingesting this file");
    assert.ok(spanLineDelta[1] > 1000, `expected > 1000 new span_line rows, got ${spanLineDelta?.[1]}`);

    const nodeDelta = report.changed.find(([rel]) => rel === "node");
    assert.ok(nodeDelta !== undefined && nodeDelta[1] > 0, "expected a positive node delta");

    // Same-file re-ingest must still be the ordinary zero-delta noop (proves the fix
    // didn't just make the FIRST commit succeed while leaving the pre-check broken for
    // a second pass over the same, now-stored, large row set).
    const reportAgain = await ingestFile(rt, rxjsIndexPath);
    assert.deepEqual(reportAgain.changed, [], "identical re-ingest of the same large file must be a zero-delta noop");
  } finally {
    await rt.dispose();
    cleanupDbFile(dbPath);
  }
});
