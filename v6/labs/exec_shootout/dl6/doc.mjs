// Regenerates the dl6 long-pager from FACTS.json plus the compiled module, so
// the document is an output of the program rather than a description of it.

import Database from "/Users/chrishafley/projects/sprefa-lanes/emitrace/v6/tsv2/node_modules/.pnpm/libsql@0.5.29/node_modules/libsql/index.js";
import { readFileSync, existsSync, writeFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const facts = JSON.parse(readFileSync(join(HERE, "FACTS.json"), "utf8"));
const baselinePath = join(HERE, "FACTS.baseline.json");
const baseline = existsSync(baselinePath)
  ? JSON.parse(readFileSync(baselinePath, "utf8"))
  : null;

const { program, incrementalPlan } = await import(join(HERE, ".compiled", "reachability.ts"));

const db = new Database(":memory:");
db.exec("PRAGMA temp_store=MEMORY;");
for (const ddl of program.ddl) db.exec(ddl);

/** Every emitted statement, named by where the emitter put it. */
function emittedStatements() {
  const named = [];
  const supportNames = [
    "supportSql[0] clear scratch",
    "supportSql[1] seed scratch, the recursive CTE",
    "supportSql[2] subtract into the head's own count",
    "supportSql[3] stage retractions",
    "supportSql[4] delete what fell to zero",
    "supportSql[5] clear the new-row set",
    "supportSql[6] fill the new-row set, ONE antijoin",
    "supportSql[7] stage additions to the delta",
    "supportSql[8] stage the frontier copy",
    "supportSql[9] stage the next-frontier copy",
    "supportSql[10] fill the head",
  ];
  for (const statement of incrementalPlan.levels) {
    (statement.supportSql ?? []).forEach((sql, index) => {
      named.push({ label: supportNames[index] ?? `supportSql[${index}]`, sql, rel: statement.headRel });
    });
    if (statement.insertSql) {
      named.push({ label: "insertSql, the delta join", sql: statement.insertSql, rel: statement.headRel });
    }
  }
  for (const relation of incrementalPlan.relations) {
    named.push({ label: `boundarySql ${relation.rel}`, sql: relation.boundarySql, rel: relation.rel });
  }
  return named;
}

function queryPlan(sql) {
  const bare = sql.replace(/\s*RETURNING[\s\S]*$/, "").replace(/\?/g, "0");
  try {
    return db.prepare(`EXPLAIN QUERY PLAN ${bare}`).all().map((step) => step.detail);
  } catch (error) {
    return [`(no plan: ${String(error.message).slice(0, 60)})`];
  }
}

/** FACTS.json truncates each shape to 150 chars, so match on that prefix. */
function costFor(caseFacts, sql) {
  const normalized = sql.replace(/\s+/g, " ").replace(/'[^']*'/g, "'?'").trim();
  return caseFacts.shapes.find((shape) => normalized.startsWith(shape.shape.slice(0, 100)));
}

function heatClass(share) {
  if (share >= 40) return "hot";
  if (share >= 10) return "warm";
  if (share >= 1) return "cool";
  return "cold";
}

/** Hard-wrap so a code shape does not blow the canvas out sideways. */
function wrapSql(sql, width = 62) {
  const out = [];
  for (const line of sql.replace(/\s+/g, " ").trim().split(/(?=\bUNION\b|\bWITH RECURSIVE\b|\bSELECT\b|\bFROM\b|\bWHERE\b|\bLEFT JOIN\b|\bGROUP BY\b|\bON\b)/)) {
    let rest = line.trim();
    if (rest.length === 0) continue;
    while (rest.length > width) {
      let cut = rest.lastIndexOf(" ", width);
      if (cut <= 0) cut = width;
      out.push(rest.slice(0, cut));
      rest = `  ${rest.slice(cut + 1)}`;
    }
    out.push(rest);
  }
  return out.join("\n  ");
}

function deltaNote(caseName, shapeText, ms) {
  if (baseline === null) return "";
  const before = baseline.cases
    .find((one) => one.name === caseName)
    ?.shapes.find((shape) => shape.shape === shapeText);
  if (before === undefined) return " · **new**";
  const change = ms - before.ms;
  if (Math.abs(change) < Math.max(before.ms * 0.05, 1)) return " · flat";
  const arrow = change < 0 ? "▼" : "▲";
  return ` · ${arrow} ${Math.abs(change).toFixed(0)} ms vs baseline`;
}

const lines = [];
lines.push(`vars: {
  d2-config: {
    layout-engine: elk
    theme-id: 0
    pad: 24
  }
}

classes: {
  hot: { style.fill: "#fee2e2"; style.stroke: "#b91c1c"; style.stroke-width: 3 }
  warm: { style.fill: "#ffedd5"; style.stroke: "#c2410c"; style.stroke-width: 2 }
  cool: { style.fill: "#fef9c3"; style.stroke: "#ca8a04" }
  cold: { style.fill: "#f3f4f6"; style.stroke: "#9ca3af" }
  code: { style.fill: "#f6f8fa"; style.stroke: "#d0d7de"; style.font-size: 12 }
  head: { style.fill: "#eef2ff"; style.stroke: "#4338ca"; style.stroke-width: 2 }
  plan: { style.fill: "#f0f9ff"; style.stroke: "#0369a1" }
}
`);

const first = facts.cases[0];
lines.push(`masthead: |~md
  # dl6, documented by running it
  Regenerate with \`just dl6-doc\`. Every number is produced by
  \`labs/exec_shootout/dl6/bench.sh\`, read out of \`FACTS.json\`.

  \`${first.name}\` · ${first.edges.toLocaleString()} edges · **${first.derived.toLocaleString()} derived rows** · checksum \`${first.checksum}\`

  | | |
  |---|---|
  | fixpoint | **${first.fixpointMs} ms** (${Math.round(first.derived / (first.fixpointMs / 1000)).toLocaleString()} rows/sec) |
  | load | ${first.loadedMs} ms |
  | peak RSS | ${Math.round(first.peakRssKb / 1024)} MB |
  | statements per tick | ${first.statements} |
  | measured on | ${facts.node} · ${facts.platform} |
  | at | ${facts.at} |
  | batches | ${facts.unbatched ? "**split**, so cost lands per statement" : "atomic, so a batch is one timing"} |
  ${baseline === null ? "" : `| baseline | \`FACTS.baseline.json\` from ${baseline.at} |`}
~|
`);

const statements = emittedStatements();
const withCost = statements
  .map((one) => ({ ...one, cost: costFor(first, one.sql) }))
  .filter((one) => one.cost !== undefined)
  .sort((left, right) => right.cost.ms - left.cost.ms);
const totalMs = withCost.reduce((sum, one) => sum + one.cost.ms, 0) || 1;

const untimed = statements.filter((one) => costFor(first, one.sql) === undefined);

lines.push(`legend: |~md
  ### colour is share of the tick
  | | |
  |---|---|
  | 🟥 hot | 40% or more |
  | 🟧 warm | 10% to 40% |
  | 🟨 cool | 1% to 10% |
  | ⬜ cold | under 1% |
~|
legend.class: head
masthead -> legend
`);

let previous = "legend";
withCost.forEach((one, index) => {
  const share = (one.cost.ms / totalMs) * 100;
  const key = `s${index}`;
  const heat = heatClass(share);
  lines.push(`${key}: "${one.label.replaceAll('"', "'")} · ${one.cost.ms.toFixed(0)} ms · ${share.toFixed(1)}%" {
  grid-columns: 2
  grid-gap: 24
  class: ${heat}

  sql: |\`sql
  ${wrapSql(one.sql)}
  \`|
  sql.class: code

  runtime: |~md
    ### runtime
    | | |
    |---|---|
    | wall | **${one.cost.ms.toFixed(1)} ms**${deltaNote(first.name, one.cost.shape, one.cost.ms)} |
    | share of tick | **${share.toFixed(1)}%** |
    | executions | ${one.cost.calls} |
    | rows returned | ${one.cost.rows.toLocaleString()} |
    | head rel | \`${one.rel}\` |

    ### query plan
    ${queryPlan(one.sql).map((step) => `- \`${step.replaceAll("|", "\\|")}\``).join("\n    ")}
  ~|
  runtime.class: plan
}
${previous} -> ${key}
`);
  previous = key;
});

if (untimed.length > 0) {
  lines.push(`unrun: |~md
  ### emitted but never executed on this workload
  ${untimed.map((one) => `- \`${one.label}\``).join("\n  ")}

  Every one is dead weight for \`${first.name}\` and live for some other program.
~|
unrun.class: cold
${previous} -> unrun
`);
  previous = "unrun";
}

const ddlIndexes = program.ddl.filter((sql) => sql.startsWith("CREATE INDEX"));
lines.push(`schema: "the schema this tick writes into" {
  grid-columns: 2
  grid-gap: 24
  class: head

  tables: |\`sql
  ${program.ddl.filter((sql) => sql.includes("CREATE ")).slice(0, 8).map((sql) => wrapSql(sql)).join("\n  ")}
  \`|
  tables.class: code

  cost: |~md
    ### what every staged row pays
    ${ddlIndexes.length} index${ddlIndexes.length === 1 ? "" : "es"} on this program, so each staged row
    costs its table write plus one btree write per index over that table.

    ${ddlIndexes.map((sql) => `- \`${/CREATE INDEX "([^"]+)"/.exec(sql)[1]}\``).join("\n    ")}
  ~|
  cost.class: plan
}
${previous} -> schema
`);

const outPath = process.argv[2] ?? join(HERE, "..", "..", "..", "..", "plans", "2026-08-06-dl6-live.d2");
writeFileSync(outPath, lines.join("\n"));
process.stderr.write(`dl6-doc: wrote ${outPath} (${withCost.length} timed statements)\n`);
