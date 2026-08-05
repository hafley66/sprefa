// perf-n1: DL_PERF_LOG JSONL aggregator. One TSV row per (tick, unit); unit is the
// rule id or rel name the line's shape provides.

// suspect = 1 when statements >= max(min_stmts, rows) AND rows > 0 AND statements > 1;
// the mechanical "one statement per row" N+1 shape, never a judgment.

// Serve shape (tsv2/serve/0_trace.ts, IServeTickLine): units from rules[]; one element
// = one executed statement (runtime/1_incremental.ts:327,:430; no dedup at 0_trace.ts:52).

// DL shape (dl/src/0_trace.ts, PerfTickLine): units from binds[]; one element = one bind
// firing. statements = contributing event count; tick-level totals never distribute onto units.

// Unparseable or shape-less lines are skipped and counted on stderr; no tick field
// aggregates under tick `-`.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

export function classifyLine(parsed) {
  if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) return null;

  if (typeof parsed.statements === "number" && Array.isArray(parsed.rules)) {
    return {
      kind: "serve",
      tick: tickOf(parsed.tick),
      units: parsed.rules.map((rule) => ({
        unit: String(rule.rule),
        rows: toNumber(rule.rows),
        wall_ms: toNumber(rule.wall_ms),
      })),
    };
  }

  if (typeof parsed.stmt_count === "number" && Array.isArray(parsed.binds)) {
    return {
      kind: "dl",
      tick: tickOf(parsed.tick),
      units: parsed.binds.map((bind) => ({
        unit: String(bind.rel),
        rows: toNumber(bind.rows),
        wall_ms: toNumber(bind.ms),
      })),
    };
  }

  return null;
}

function tickOf(value) {
  return typeof value === "number" ? value : "-";
}

function toNumber(value) {
  if (typeof value === "number") return value;
  if (typeof value === "string" && value.trim() !== "") {
    const coerced = Number(value);
    return Number.isNaN(coerced) ? 0 : coerced;
  }
  return 0;
}

export function computeSuspect(statements, rows, minStatements) {
  const threshold = Math.max(minStatements, rows);
  if (statements >= threshold && rows > 0 && statements > 1) return 1;
  return 0;
}

export function processJsonlLines(rawLines, minStatements) {
  const accumulator = new Map();
  let skipCount = 0;

  for (const raw of rawLines) {
    if (raw.trim() === "") continue;

    let parsed;
    try {
      parsed = JSON.parse(raw);
    } catch {
      skipCount += 1;
      continue;
    }

    const classified = classifyLine(parsed);
    if (classified === null) {
      skipCount += 1;
      continue;
    }

    for (const unit of classified.units) {
      const key = `${classified.tick}\u0000${unit.unit}`;
      const existing = accumulator.get(key);
      if (existing) {
        existing.statements += 1;
        existing.rows += unit.rows;
        existing.wall_ms += unit.wall_ms;
      } else {
        accumulator.set(key, {
          tick: classified.tick,
          unit: unit.unit,
          statements: 1,
          rows: unit.rows,
          wall_ms: unit.wall_ms,
        });
      }
    }
  }

  const rows = [...accumulator.values()];
  rows.sort(
    (left, right) =>
      right.wall_ms - left.wall_ms ||
      String(left.tick).localeCompare(String(right.tick)) ||
      left.unit.localeCompare(right.unit),
  );

  return { rows, skipCount };
}

export function renderRows(rows, minStatements, includeHeader = true) {
  const lines = [];
  if (includeHeader) lines.push(["tick", "unit", "statements", "rows", "wall_ms", "suspect"].join("\t"));
  for (const row of rows) {
    lines.push(
      [
        String(row.tick),
        row.unit,
        row.statements,
        row.rows,
        row.wall_ms,
        computeSuspect(row.statements, row.rows, minStatements),
      ].join("\t"),
    );
  }
  return lines.join("\n");
}

export function parseMinStatements(argv) {
  const flagIndex = argv.indexOf("--min-stmts");
  if (flagIndex === -1 || argv[flagIndex + 1] === undefined) return 10;
  const parsed = Number(argv[flagIndex + 1]);
  return Number.isFinite(parsed) && parsed >= 0 ? parsed : 10;
}

export function main(argv) {
  const positional = argv.filter((argument) => !argument.startsWith("--"));
  const logPath = positional[0];
  if (logPath === undefined) {
    process.stderr.write("usage: node perf-n1.mjs <path-to-jsonl> [--min-stmts <n>]\n");
    process.exit(2);
  }

  const minStatements = parseMinStatements(argv);
  const rawLines = readFileSync(logPath, "utf8").split("\n");
  const { rows, skipCount } = processJsonlLines(rawLines, minStatements);

  process.stdout.write(renderRows(rows, minStatements, true) + "\n");
  if (skipCount > 0) {
    process.stderr.write(`perf-n1: skipped ${skipCount} unparseable or shape-less line(s)\n`);
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2));
}
