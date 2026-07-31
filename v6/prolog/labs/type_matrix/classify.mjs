// classify.mjs -- turn out/run-results.json into the four graded verdicts and
// write MATRIX.md + out/matrix.json. Nothing here runs a door; every judgement
// is a comparison over bytes drive.mjs already banked.
//
// THE FOUR VERDICTS (header contract), decided in this order:
//
//   NAMED REFUSAL     both doors refused by name. Sub-labelled `both` when the
//                     names agree, `name_mismatch` when they do not, and
//                     `compiler_only` for the standing bucket where the
//                     compiler is narrower than the reference engine (that is
//                     the sweep's own `unsupported` class, not a divergence).
//   DIVERGENT         the doors disagree: a byte difference in the graded
//                     surface, a refusal on one door and an answer on the
//                     other, an unnamed crash, or the two EMITTER MODES
//                     disagreeing with each other.
//   SILENT COERCION   every door agrees, and the value that came out is not
//                     the value that went in.
//   IDENTICAL         every door agrees and the value survived.
//
// LOSSLESSNESS is measured against ONE fixed rendering per value (`survives`
// below), never against the cell's own declared type: "did the value survive"
// has a single answer per value, and making the yardstick move with the
// declared type would define every coercion away.
//
// Run: node v6/prolog/labs/type_matrix/classify.mjs

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const OUT = join(HERE, "out");

// The tick-log rendering each value has when nothing has bent it. A json
// document's arrival is TEXT and its rendering is the parsed value, which is
// the arrival contract rather than a loss.
const SURVIVES = {
  int: "4",
  float: "1.5",
  numeric_text: '"4"',
  plain_text: '"north"',
  json_object: '{"key":1}',
  json_array: "[1,2]",
  bool: "true",
  wide_int: "9007199254740993",
  float_integral: "1.0",
  neg_zero: "-0.0",
};

/** `unsupported_construct(edge_into_unkeyed_set(probe_out/1))` -> the inner
 *  functor, which is the name the refusal is known by everywhere else. A
 *  refusal thrown WITHOUT a wrapper (the engine's own, see oracle_all.pl)
 *  keeps its own functor. */
function refusalName(detail) {
  if (typeof detail !== "string" || detail.length === 0) return "";
  const outer = /^(unsupported_construct|unsupported_surface|parse_findings)\((.*)\)$/s.exec(detail.trim());
  const inner = outer === null ? detail.trim() : outer[2];
  const named = /^\[?\s*([a-z_][A-Za-z0-9_]*)/.exec(inner);
  return named === null ? inner.slice(0, 60) : named[1];
}

/** `dl_parse_error(statement,[112,114,...])` prints its offending statement as
 *  a code list, which no human reads. Rendered back to text here so the
 *  matrix carries the receipt rather than the bytes. (That the door emits
 *  codes at all is finding B4-in-the-wild, recorded in the verdict.) */
function readableDetail(detail) {
  if (typeof detail !== "string") return "";
  return detail.replace(/\[(\d+(?:,\d+)*)\]/g, (whole, digits) => {
    const codes = digits.split(",").map(Number);
    if (codes.some((code) => code < 9 || code > 126)) return whole;
    return JSON.stringify(String.fromCharCode(...codes));
  });
}

/** The rows of `probe_out` in a `{"final":{...}}` line, or null when the log
 *  has no final line at all. */
function finalRows(log, rel) {
  if (typeof log !== "string") return null;
  const line = log.split("\n").filter((text) => text.startsWith('{"final"')).pop();
  if (line === undefined) return null;
  const parsed = JSON.parse(line).final;
  return parsed[rel] ?? [];
}

/** The graded value: the column under test of the first `probe_out` row,
 *  re-rendered as the JSON text it appeared as. */
function gradedValue(log, columnIndex) {
  const rows = finalRows(log, "probe_out");
  if (rows === null) return "NO_FINAL_LINE";
  if (rows.length === 0) return "ABSENT";
  return JSON.stringify(rows[0][columnIndex]);
}

function classify(cell, result) {
  const compiled = result.compiled;
  const oracle = result.oracle;
  const incremental = result.emitter.incremental;
  const naive = result.emitter.naive;

  // THE ORACLE DOOR IS TWO DOORS. A cell is graded against whichever `.dl6`
  // door ACCEPTED its arrival: dl6_oracle.pl first (it is the general door and
  // the only one with type-directed json), golden_oracle.pl when dl6_oracle's
  // schedule mapping cannot carry the value at all. `doorGap` records that
  // fallback and `doorLogsDisagree` records the cells where both doors ran and
  // answered differently -- both are reported on their own axis, never folded
  // into the type verdict.
  const dl6Ran = oracle.status === "ok" && typeof result.oracleLog === "string";
  const goldenRanClean = oracle.goldenStatus === "ok" && typeof result.goldenLog === "string";
  // The fallback is NARROW on purpose. `type_arrival_shape_mismatch` is the
  // one refusal that can come from the DOOR mis-mapping a schedule value
  // rather than from the value genuinely not fitting -- golden_oracle carries
  // floats and booleans that dl6_oracle turns into atoms, and it still refuses
  // the honestly-wrong ones (a text value at a float column fails on both).
  // A wide fallback was measured wrong: golden_oracle also "passes" every
  // `json_capture_type_unknown` cell, VACUOUSLY, because its schedule mapping
  // leaves a json column an atom so the capture is never reached. Rescuing a
  // real refusal with a vacuous run would have deleted 20 named refusals.
  const doorMappingGap = !dl6Ran && goldenRanClean &&
    refusalName(oracle.detail) === "type_arrival_shape_mismatch";
  const goldenRan = doorMappingGap;
  const gradedVia = dl6Ran ? "dl6_oracle" : goldenRan ? "golden_oracle" : "";
  const oracleLog = dl6Ran ? result.oracleLog : goldenRan ? result.goldenLog : null;

  const compilerRefused = compiled.status === "refused";
  const oracleRefused = !dl6Ran && !goldenRan;
  const compilerBroke = compiled.status === "error" || compiled.status === "halt";
  const oracleBroke = oracleRefused &&
    (oracle.status === "error" || oracle.status === "halt") &&
    refusalName(oracle.detail) === "";

  const record = {
    id: cell.id,
    position: cell.position,
    declared: cell.declared,
    value: cell.value,
    compileStatus: compiled.status,
    compileRefusal: compilerRefused ? refusalName(compiled.detail) : "",
    oracleStatus: oracle.status,
    oracleRefusal: oracle.status === "ok" ? "" : refusalName(oracle.detail),
    goldenStatus: oracle.goldenStatus,
    goldenRefusal: oracle.goldenStatus === "ok" ? "" : refusalName(oracle.goldenDetail),
    gradedVia,
    doorGap: !dl6Ran && goldenRan ? refusalName(oracle.detail) : "",
    // goldenRanClean, not goldenRan: this axis asks whether the two doors
    // answer the same when BOTH accept the arrival, which is exactly the case
    // the narrow fallback excludes.
    doorLogsDisagree: dl6Ran && goldenRanClean && result.oracleLog !== result.goldenLog,
    graded: "",
    survives: SURVIVES[cell.value],
    verdict: "",
    label: "",
    detail: "",
  };

  // ── refusal territory ────────────────────────────────────────────────────
  if (compilerRefused && oracleRefused) {
    record.verdict = "NAMED_REFUSAL";
    record.label = record.compileRefusal === record.oracleRefusal ? "both" : "name_mismatch";
    record.detail = record.compileRefusal === record.oracleRefusal
      ? record.compileRefusal
      : `compiler=${record.compileRefusal} oracle=${record.oracleRefusal}`;
    return record;
  }
  if (compilerBroke || oracleBroke) {
    record.verdict = "DIVERGENT";
    record.label = "unnamed_failure";
    record.detail = compilerBroke
      ? `compiler ${compiled.status}: ${readableDetail(compiled.detail).slice(0, 160)}`
      : `oracle ${oracle.status}: ${readableDetail(oracle.detail).slice(0, 160)}`;
    return record;
  }
  if (compilerRefused && !oracleRefused) {
    record.verdict = "NAMED_REFUSAL";
    record.label = "compiler_only";
    record.detail = record.compileRefusal;
    record.graded = gradedValue(oracleLog, cell.columnIndex);
    return record;
  }
  if (oracleRefused && compiled.status === "ok") {
    // The receipt that matters is not that the doors disagree, it is WHAT the
    // emitter stored where the reference engine refused to accept the row.
    const stored = incremental?.status === "ok"
      ? gradedValue(incremental.log, cell.columnIndex)
      : `emitter ${incremental?.status ?? "not run"}`;
    record.verdict = "DIVERGENT";
    record.label = "oracle_only_refusal";
    record.detail = `both oracle doors refuse (dl6 ${record.oracleRefusal}, golden ${record.goldenRefusal}); emitter compiled and stored ${stored}`;
    return record;
  }

  // ── both doors ran ───────────────────────────────────────────────────────
  if (incremental?.status !== "ok" || naive?.status !== "ok") {
    record.verdict = "DIVERGENT";
    record.label = "emitter_run_error";
    record.detail = (incremental?.status !== "ok" ? incremental?.detail : naive?.detail) ?? "";
    record.graded = gradedValue(oracleLog, cell.columnIndex);
    return record;
  }

  record.graded = gradedValue(oracleLog, cell.columnIndex);
  const modesAgree = incremental.log === naive.log;
  const incrementalAgrees = incremental.log === oracleLog;
  const naiveAgrees = naive.log === oracleLog;

  if (!modesAgree) {
    record.verdict = "DIVERGENT";
    record.label = "emitter_modes_disagree";
    record.detail = `incremental ${JSON.stringify(gradedValue(incremental.log, cell.columnIndex))}` +
      ` vs naive ${JSON.stringify(gradedValue(naive.log, cell.columnIndex))}` +
      ` vs oracle ${JSON.stringify(record.graded)}`;
    return record;
  }
  if (!incrementalAgrees || !naiveAgrees) {
    record.verdict = "DIVERGENT";
    record.label = "doors_disagree";
    record.detail = `oracle ${JSON.stringify(record.graded)}` +
      ` vs emitter ${JSON.stringify(gradedValue(incremental.log, cell.columnIndex))}`;
    return record;
  }

  if (record.graded === record.survives) {
    record.verdict = "IDENTICAL";
    record.label = "lossless";
    return record;
  }
  record.verdict = "SILENT_COERCION";
  record.label = record.graded === "ABSENT" ? "row_absent" : "value_changed";
  record.detail = `fed ${record.survives}, graded ${record.graded}`;
  return record;
}

function table(rows, columns) {
  const header = `| ${columns.join(" | ")} |`;
  const rule = `|${columns.map(() => "---").join("|")}|`;
  return [header, rule, ...rows.map((row) => `| ${row.join(" | ")} |`)].join("\n");
}

function main() {
  const cells = JSON.parse(readFileSync(join(OUT, "cells.json"), "utf8"));
  const results = JSON.parse(readFileSync(join(OUT, "run-results.json"), "utf8"));
  const byId = new Map(results.map((result) => [result.id, result]));

  const records = cells
    .filter((cell) => byId.has(cell.id))
    .map((cell) => classify(cell, byId.get(cell.id)));

  // Both graded artifacts live at the lab root, not in out/: out/ is ~1700
  // generated files and is gitignored, and the matrix is the thing worth
  // reading in a diff.
  writeFileSync(join(HERE, "matrix.json"), `${JSON.stringify(records, null, 1)}\n`);

  const counts = {};
  for (const record of records) {
    const key = `${record.verdict}/${record.label}`;
    counts[key] = (counts[key] ?? 0) + 1;
  }

  const lines = [];
  lines.push("# Type matrix -- generated, do not hand-edit");
  lines.push("");
  lines.push("Regenerate: `bash v6/prolog/labs/type_matrix/matrix.sh`");
  lines.push("");
  lines.push(`Cells: ${records.length} constructible / ${cells.length - records.length} not run / 0 n-a`);
  lines.push("");
  lines.push("## Verdict counts");
  lines.push("");
  lines.push(table(
    Object.entries(counts).sort((left, right) => right[1] - left[1]).map(([key, count]) => [key, String(count)]),
    ["verdict / label", "cells"],
  ));
  lines.push("");

  // ── the two .dl6 oracle doors, reported on their own axis ────────────────
  const doorGaps = records.filter((record) => record.doorGap !== "");
  const doorDisagreements = records.filter((record) => record.doorLogsDisagree);
  lines.push("## The two `.dl6` oracle doors");
  lines.push("");
  lines.push(`dl6_oracle.pl accepted the arrival in ${records.filter((r) => r.gradedVia === "dl6_oracle").length} cells,`
    + ` golden_oracle.pl carried ${doorGaps.length} more that dl6_oracle refused outright,`
    + ` and ${doorDisagreements.length} cells ran on BOTH doors and produced DIFFERENT tick logs.`);
  lines.push("");
  const gapByReason = {};
  for (const record of doorGaps) {
    const key = `${record.declared} <- ${record.value} (${record.doorGap})`;
    gapByReason[key] = (gapByReason[key] ?? 0) + 1;
  }
  lines.push(table(
    Object.entries(gapByReason).sort((left, right) => right[1] - left[1]).map(([key, count]) => [key, String(count)]),
    ["dl6_oracle refuses, golden_oracle accepts", "cells"],
  ));
  lines.push("");
  const disagreeByShape = {};
  for (const record of doorDisagreements) {
    const key = `${record.declared} <- ${record.value}`;
    disagreeByShape[key] = (disagreeByShape[key] ?? 0) + 1;
  }
  lines.push(table(
    Object.entries(disagreeByShape).sort((left, right) => right[1] - left[1]).map(([key, count]) => [key, String(count)]),
    ["both doors ran, logs differ", "cells"],
  ));
  lines.push("");

  lines.push("## Per position");
  lines.push("");
  const positions = [...new Set(records.map((record) => record.position))];
  const verdicts = ["IDENTICAL", "SILENT_COERCION", "DIVERGENT", "NAMED_REFUSAL"];
  lines.push(table(
    positions.map((position) => [
      position,
      ...verdicts.map((verdict) =>
        String(records.filter((record) => record.position === position && record.verdict === verdict).length)),
    ]),
    ["position", ...verdicts],
  ));
  lines.push("");

  lines.push("## Per declared type");
  lines.push("");
  const declaredTypes = [...new Set(records.map((record) => record.declared))];
  lines.push(table(
    declaredTypes.map((declared) => [
      declared,
      ...verdicts.map((verdict) =>
        String(records.filter((record) => record.declared === declared && record.verdict === verdict).length)),
    ]),
    ["declared", ...verdicts],
  ));
  lines.push("");

  lines.push("## Per fed value");
  lines.push("");
  const values = [...new Set(records.map((record) => record.value))];
  lines.push(table(
    values.map((value) => [
      value,
      ...verdicts.map((verdict) =>
        String(records.filter((record) => record.value === value && record.verdict === verdict).length)),
    ]),
    ["value", ...verdicts],
  ));
  lines.push("");

  lines.push("## Every cell");
  lines.push("");
  lines.push(table(
    records.map((record) => [
      record.position,
      record.declared,
      record.value,
      record.verdict,
      record.label,
      (record.detail || record.graded).replace(/\|/g, "\\|").slice(0, 140),
    ]),
    ["position", "declared", "value", "verdict", "label", "receipt"],
  ));
  lines.push("");

  writeFileSync(join(HERE, "MATRIX.md"), `${lines.join("\n")}\n`);
  process.stdout.write(`classify: ${records.length} cells\n`);
  for (const [key, count] of Object.entries(counts).sort((left, right) => right[1] - left[1])) {
    process.stdout.write(`  ${key}: ${count}\n`);
  }
}

main();
