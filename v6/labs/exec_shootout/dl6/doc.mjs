// Regenerates the dl6 long-pager from FACTS.json plus the compiled module, so
// the document is an output of the program rather than a description of it.

import Database from "/Users/chrishafley/projects/sprefa-lanes/emitrace/v6/tsv2/node_modules/.pnpm/libsql@0.5.29/node_modules/libsql/index.js";
import { readFileSync, existsSync, writeFileSync } from "node:fs";
import { join, dirname, resolve as resolvePath } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolvePath(HERE, "..", "..", "..", "..");
// vscode:// opens the editor at the line; file:// opens the default handler.
const LINK_SCHEME = process.env.DL6_DOC_LINKS ?? "vscode";

const facts = JSON.parse(readFileSync(join(HERE, "FACTS.json"), "utf8"));
const baselinePath = join(HERE, "FACTS.baseline.json");
const baseline = existsSync(baselinePath) ? JSON.parse(readFileSync(baselinePath, "utf8")) : null;
const { program, incrementalPlan } = await import(join(HERE, ".compiled", "reachability.ts"));

const db = new Database(":memory:");
db.exec("PRAGMA temp_store=MEMORY;");
for (const ddl of program.ddl) db.exec(ddl);

/** Repo-relative file plus a literal needle to file:line, so a rename shows up
 *  as line 0 rather than as a silently stale number. */
function site(relativePath, needle) {
  const absolute = join(REPO, relativePath);
  let lineNumber = 0;
  try {
    const lines = readFileSync(absolute, "utf8").split("\n");
    const anchored = needle.startsWith("^");
    const target = anchored ? needle.slice(1) : needle;
    lineNumber = lines.findIndex((line) => (anchored ? line.startsWith(target) : line.includes(target))) + 1;
  } catch {
    lineNumber = 0;
  }
  const href =
    LINK_SCHEME === "file"
      ? `file://${absolute}`
      : `vscode://file${absolute}:${lineNumber || 1}`;
  return { path: relativePath, line: lineNumber, href, absolute };
}

function linkedBox(key, className, markdown, where) {
  return `${key}: |~md
${markdown}

  [\`${where.path}:${where.line}\`](${where.href})
~|
${key}.shape: rectangle
${key}.class: ${className}
${key}.link: ${where.href}
${key}.tooltip: ${where.absolute}:${where.line}
`;
}

const CHAIN = [
  {
    key: "driver",
    className: "driver",
    where: site("v6/labs/exec_shootout/dl6/run.ts", "function main"),
    md: `  ### 1 · the driver
  \`run.ts\` reads the edge file and hands every edge to the
  tick loop as ONE arrival batch.

  \`\`\`
  TickFold.run(program, seam, [arrivals], 1_000_000)
  \`\`\``,
  },
  {
    key: "tickfold",
    className: "runtime",
    where: site("v6/tsv2/runtime/tickLoop.ts", "run("),
    md: `  ### 2 · the tick loop
  \`TickFold.run\` drains the batch queue, calling the program's
  own \`tick\` once per tick and emitting one tick-log line each.`,
  },
  {
    key: "emitted",
    className: "emitted",
    where: site("v6/prolog/compile/scripts/compile_dl6.sh", "#!"),
    md: `  ### 3 · the COMPILED program
  \`.compiled/reachability.ts\`, written by the prolog compiler from
  \`reachability.dl6\`. Its \`runIncrementalTick\` names the phases:

  \`\`\`
  prepareTick -> applyArrivals -> applyLevelsBeforeEdges
    -> applyEdges -> recomputeLevelsAfterEdges
    -> readBoundary -> promoteFrontiers
  \`\`\`

  This file is GENERATED. Regenerate it, never edit it.`,
  },
  {
    key: "levels",
    className: "runtime",
    where: site("v6/tsv2/runtime/1_incremental.ts", "  applyLevelsBeforeEdges("),
    md: `  ### 4 · the level phase
  For a head on a CYCLE of the level graph, this calls the refCount
  reconcile once. Everything else stays a single pass.`,
  },
  {
    key: "reconcile",
    className: "runtime",
    where: site("v6/tsv2/runtime/1_incremental.ts", "^function reconcileRefCountStatement"),
    md: `  ### 5 · THE THING THAT RUNS THE SQL
  \`reconcileRefCountStatement\` takes the emitted \`supportSql\`
  array, picks which frontier copies this pass wants, and hands
  the whole list to the seam as one atomic batch.

  Nothing here builds SQL. Every string came from the compiler.`,
  },
  {
    key: "seam",
    className: "seam",
    where: site("v6/sprefa-store/js/src/engine/sqlRunner.ts", "batch("),
    md: `  ### 6 · the SQL seam
  \`SqlRunner.batch\` is the only place TypeScript touches the
  database. It wraps \`db.batch\` in an Observable and counts
  statements. The bench wraps THIS to time each shape.`,
  },
  {
    key: "sqlite",
    className: "sqlite",
    where: site("v6/tsv2/package.json", "@libsql/client"),
    md: `  ### 7 · SQLite
  \`@libsql/client\` over the bundled native SQLite, opened at
  \`:memory:\` for this benchmark. Everything below is C.`,
  },
];

const EMITTER = [
  {
    key: "dl6src",
    className: "source",
    where: site("v6/labs/exec_shootout/dl6/reachability.dl6", "reachable"),
    md: `  ### the program, as written
  \`\`\`
  reachable(Source, Target) <- edge(Source, Target).
  reachable(Source, Target) <- reachable(Source, Mid),
                               edge(Mid, Target).
  \`\`\``,
  },
  {
    key: "lowerpl",
    className: "compiler",
    where: site("v6/prolog/lower.pl", "^level_ref_count_sql("),
    md: `  ### the predicate that WROTE every statement below
  \`level_ref_count_sql/4\` formats all 11 \`supportSql\` strings.
  Change the SQL on this page by changing this predicate.`,
  },
  {
    key: "emitpl",
    className: "compiler",
    where: site("v6/prolog/emit_ts.pl", "ref_count_sql_text(refcountsql"),
    md: `  ### the predicate that PRINTS them into TypeScript
  \`ref_count_sql_text/2\` renders the term as a JS array literal.`,
  },
];

function queryPlan(sql) {
  const bare = sql.replace(/\s*RETURNING[\s\S]*$/, "").replace(/\?/g, "0");
  try {
    return db.prepare(`EXPLAIN QUERY PLAN ${bare}`).all().map((step) => step.detail);
  } catch (error) {
    return [`(no plan: ${String(error.message).slice(0, 60)})`];
  }
}

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

function wrapSql(sql, width = 60) {
  const out = [];
  const parts = sql
    .replace(/\s+/g, " ")
    .trim()
    .split(/(?=\bUNION\b|\bWITH RECURSIVE\b|\bSELECT\b|\bFROM\b|\bWHERE\b|\bLEFT JOIN\b|\bGROUP BY\b|\bON\b)/);
  for (const part of parts) {
    let rest = part.trim();
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
  const before = baseline.cases.find((one) => one.name === caseName)?.shapes.find((shape) => shape.shape === shapeText);
  if (before === undefined) return " · **new**";
  const change = ms - before.ms;
  if (Math.abs(change) < Math.max(before.ms * 0.05, 1)) return " · flat";
  return ` · ${change < 0 ? "▼" : "▲"} ${Math.abs(change).toFixed(0)} ms`;
}

const SUPPORT_NAMES = [
  "clear the scratch table",
  "seed the scratch table, the recursive CTE",
  "set each head row's count from the scratch table",
  "stage retractions into the delta",
  "delete what fell to zero",
  "clear the new-row set",
  "fill the new-row set, ONE antijoin",
  "stage additions into the delta",
  "stage the frontier copy",
  "stage the next-frontier copy",
  "fill the head",
];

function emittedStatements() {
  const named = [];
  for (const statement of incrementalPlan.levels) {
    (statement.supportSql ?? []).forEach((sql, index) => {
      named.push({
        label: `supportSql[${index}] ${SUPPORT_NAMES[index] ?? ""}`.trim(),
        sql,
        rel: statement.headRel,
        origin: site("v6/prolog/lower.pl", "^level_ref_count_sql("),
      });
    });
    if (statement.insertSql) {
      named.push({
        label: "insertSql, the delta join",
        sql: statement.insertSql,
        rel: statement.headRel,
        origin: site("v6/prolog/lower.pl", "^level_delta_insert_sql("),
      });
    }
  }
  for (const relation of incrementalPlan.relations) {
    named.push({
      label: `boundarySql ${relation.rel}`,
      sql: relation.boundarySql,
      rel: relation.rel,
      origin: site("v6/prolog/lower.pl", `'SELECT ~w, "_sign" AS "__sign", count(*)`),
    });
  }
  return named;
}


// A diamond: 1 reaches 4 through 2 AND through 3, so reachable(1,4) has TWO
// derivations. That is the whole reason a refCount exists.
const TOY_EDGES = [[1, 2], [2, 4], [1, 3], [3, 4]];

function toyDb() {
  const fresh = new Database(":memory:");
  fresh.exec("PRAGMA temp_store=MEMORY;");
  for (const ddl of program.ddl) fresh.exec(ddl);
  const insertEdge = fresh.prepare(`INSERT OR IGNORE INTO "edge" ("source","target") VALUES (?,?)`);
  const stageEdge = fresh.prepare(`INSERT INTO "__frontier_edge" ("_phase","_sequence","source","target") VALUES (2,?,?,?)`);
  const deltaEdge = fresh.prepare(`INSERT INTO "__delta_edge" ("_sign","_sequence","source","target") VALUES (1,?,?,?)`);
  TOY_EDGES.forEach(([source, target], index) => {
    insertEdge.run(source, target);
    stageEdge.run(index, source, target);
    deltaEdge.run(index, source, target);
  });
  return fresh;
}

/** The table a statement WRITES, so the snapshot is of the thing that moved. */
function writtenTable(sql) {
  const match = /^\s*(?:INSERT(?:\s+OR\s+IGNORE)?\s+INTO|DELETE\s+FROM|UPDATE)\s+"([^"]+)"/i.exec(sql);
  return match?.[1] ?? null;
}

function snapshot(handle, table) {
  try {
    const rows = handle.prepare(`SELECT * FROM "${table}" LIMIT 12`).all();
    const total = handle.prepare(`SELECT count(*) AS n FROM "${table}"`).get().n;
    return { rows, total };
  } catch {
    return { rows: [], total: 0 };
  }
}

function renderRows(shot, table) {
  if (shot.total === 0) return `\`${table}\` is **empty**`;
  const columns = Object.keys(shot.rows[0]);
  const header = `| ${columns.join(" | ")} |`;
  const rule = `|${columns.map(() => "---").join("|")}|`;
  const body = shot.rows
    .map((row) => `| ${columns.map((column) => String(row[column])).join(" | ")} |`)
    .join("\n    ");
  const more = shot.total > shot.rows.length ? `\n    _...${shot.total - shot.rows.length} more_` : "";
  return `${header}\n    ${rule}\n    ${body}${more}`;
}

/** Runs the emitted sequence on the toy graph, snapshotting the written table
 *  either side of every statement. */
function toyTrace() {
  const handle = toyDb();
  const trace = new Map();
  const statement = incrementalPlan.levels.find((one) => one.supportSql !== null);
  (statement.supportSql ?? []).forEach((sql, index) => {
    const table = writtenTable(sql);
    const before = table ? snapshot(handle, table) : null;
    const bound = /\?/.test(sql) ? [2] : [];
    try {
      handle.prepare(sql).run(...bound);
    } catch (error) {
      trace.set(sql, { table, before, after: null, error: String(error.message).slice(0, 80) });
      return;
    }
    const after = table ? snapshot(handle, table) : null;
    trace.set(sql, { table, before, after, error: null });
  });
  const closure = snapshot(handle, "reachable");
  handle.close();
  return { trace, closure };
}

const toy = toyTrace();

function toySection(sql) {
  const step = toy.trace.get(sql);
  if (step === undefined || step.table === null) return "";
  if (step.error !== null) return `\n\n    ### toy run\n    refused: \`${step.error}\``;
  const changed = step.before.total !== step.after.total;
  return `\n\n    ### the toy graph, this statement's table
    \`${step.table}\` ${changed ? `**${step.before.total} rows -> ${step.after.total} rows**` : `stayed at ${step.after.total} rows`}

    **before**

    ${renderRows(step.before, step.table)}

    **after**

    ${renderRows(step.after, step.table)}`;
}

const first = facts.cases[0];
const statements = emittedStatements();
const withCost = statements
  .map((one) => ({ ...one, cost: costFor(first, one.sql) }))
  .filter((one) => one.cost !== undefined)
  .sort((left, right) => right.cost.ms - left.cost.ms);
const totalMs = withCost.reduce((sum, one) => sum + one.cost.ms, 0) || 1;
const untimed = statements.filter((one) => costFor(first, one.sql) === undefined);

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
  driver: { style.fill: "#ede9fe"; style.stroke: "#6d28d9"; style.stroke-width: 2 }
  runtime: { style.fill: "#dcfce7"; style.stroke: "#15803d"; style.stroke-width: 2 }
  emitted: { style.fill: "#fef3c7"; style.stroke: "#b45309"; style.stroke-width: 2 }
  seam: { style.fill: "#e0f2fe"; style.stroke: "#0369a1"; style.stroke-width: 2 }
  sqlite: { style.fill: "#dbeafe"; style.stroke: "#1d4ed8"; style.stroke-width: 3 }
  source: { style.fill: "#fae8ff"; style.stroke: "#a21caf"; style.stroke-width: 2 }
  compiler: { style.fill: "#fce7f3"; style.stroke: "#be185d"; style.stroke-width: 2 }
  plan: { style.fill: "#f0f9ff"; style.stroke: "#0369a1" }
  head: { style.fill: "#eef2ff"; style.stroke: "#4338ca"; style.stroke-width: 2 }
}
`);

lines.push(`masthead: |~md
  # Who runs this SQL

  **Every box on this page is a link.** Click one and your editor opens at
  that file and line. Nothing here was typed by hand: \`just dl6-doc\` runs
  the benchmark, reads \`FACTS.json\`, and writes this file.

  \`${first.name}\` · ${first.edges.toLocaleString()} edges · **${first.derived.toLocaleString()} derived rows** · checksum \`${first.checksum}\`

  | | |
  |---|---|
  | fixpoint | **${first.fixpointMs} ms** (${Math.round(first.derived / (first.fixpointMs / 1000)).toLocaleString()} rows/sec) |
  | peak RSS | ${Math.round(first.peakRssKb / 1024)} MB |
  | statements per tick | ${first.statements} |
  | measured | ${facts.node} · ${facts.platform} · ${facts.at} |
  | batches | ${facts.unbatched ? "**split**, so cost lands per statement" : "atomic, so a batch is one timing"} |
  ${baseline === null ? "" : `| baseline | \`FACTS.baseline.json\`, ${baseline.at} |`}

  Links use \`${LINK_SCHEME}://\`. Set \`DL6_DOC_LINKS=file\` for the other kind.
~|
`);

lines.push(`chain: "the call chain, driver down to C" {
  style.fill: "#ffffff"
  style.stroke: "#374151"

${CHAIN.map((step) => linkedBox(step.key, step.className, step.md, step.where)).join("\n")}
${CHAIN.slice(1).map((step, index) => `  ${CHAIN[index].key} -> ${step.key}`).join("\n")}
}
masthead -> chain
`);

lines.push(`emitter: "where the SQL strings come from" {
  style.fill: "#ffffff"
  style.stroke: "#be185d"

${EMITTER.map((step) => linkedBox(step.key, step.className, step.md, step.where)).join("\n")}
  dl6src -> lowerpl: "compiled by"
  lowerpl -> emitpl: "printed by"
}
chain -> emitter: "step 3 is this file's output"
`);

const byName = (needle) => withCost.find((one) => one.label.includes(needle));
const ms = (needle) => (byName(needle) ? byName(needle).cost.ms.toFixed(0) : "?");

lines.push(`flow: "one tick, as a sequence" {
  shape: sequence_diagram

  driver: run.ts
  fold: TickFold
  module: "compiled module"
  runtime: IncrementalRuntime
  seam: SqlRunner
  db: SQLite

  driver -> fold: "${first.edges.toLocaleString()} edges, ONE batch"
  fold -> module: "tick(seam, arrivals)"

  arrivals: {
    module -> runtime: prepareTick
    runtime -> seam: "clear delta + next-frontier"
    seam -> db: batch
    module -> runtime: applyArrivals
    runtime -> seam: "json_each insert"
    seam -> db: "batch, ${ms("boundarySql edge") === "?" ? "3" : "3"} statements"
    db -> runtime: "${first.edges.toLocaleString()} edge rows staged"
  }

  the closure: {
    module -> runtime: applyLevelsBeforeEdges
    runtime -> runtime: "head on a cycle?"
    runtime -> seam: "the 11 refCount statements, ONE batch"
    seam -> db: "batch"
    db -> db: "WITH RECURSIVE, ${ms("recursive CTE")} ms"
    db -> db: "antijoin into the scratch set, ${ms("ONE antijoin")} ms"
    db -> db: "stage delta, ${ms("additions into the delta")} ms"
    db -> db: "stage frontier, ${ms("frontier copy")} ms"
    db -> db: "fill the head, ${ms("fill the head")} ms"
    db -> runtime: "no rows cross"
  }

  the rest: {
    module -> runtime: applyEdges
    module -> runtime: recomputeLevelsAfterEdges
    runtime -> seam: "retraction guard"
    seam -> db: "EXISTS _sign = -1"
    db -> runtime: "0, so skip the reconcile"
    module -> runtime: readBoundary
    runtime -> runtime: "rel in unreadRels?"
    runtime -> module: "empty delta, rows stay in SQLite"
    module -> runtime: promoteFrontiers
    runtime -> seam: "merge next into current"
  }

  module -> fold: "ITickDeltas"
  fold -> driver: "one tick-log line"
  driver -> db: "paged checksum fold, 250k rows at a time"
  db -> driver: "${first.derived.toLocaleString()} rows, ${first.checksum}"
}
emitter -> flow: "and this is the order it runs in"
`);

lines.push(`legend: |~md
  ### colour on the statements below is share of the tick
  | | |
  |---|---|
  | 🟥 | 40% or more |
  | 🟧 | 10% to 40% |
  | 🟨 | 1% to 10% |
  | ⬜ | under 1% |

  Every statement below was handed to step 6 by step 5.
~|
legend.class: head
flow -> legend
`);

let previous = "legend";
withCost.forEach((one, index) => {
  const share = (one.cost.ms / totalMs) * 100;
  const key = `s${index}`;
  lines.push(`${key}: "${one.label.replaceAll('"', "'")} · ${one.cost.ms.toFixed(0)} ms · ${share.toFixed(1)}%" {
  grid-columns: 2
  grid-gap: 20
  class: ${heatClass(share)}

  sql: |\`sql
  ${wrapSql(one.sql)}
  \`|
  sql.class: code
  sql.link: ${one.origin.href}
  sql.tooltip: written by ${one.origin.path}:${one.origin.line}

  runtime: |~md
    ### measured
    | | |
    |---|---|
    | wall | **${one.cost.ms.toFixed(1)} ms**${deltaNote(first.name, one.cost.shape, one.cost.ms)} |
    | share of tick | **${share.toFixed(1)}%** |
    | executions | ${one.cost.calls} |
    | rows returned | ${one.cost.rows.toLocaleString()} |
    | head rel | \`${one.rel}\` |

    ### SQLite's plan for it
    ${queryPlan(one.sql).map((step) => `- \`${step.replaceAll("|", "\\|")}\``).join("\n    ")}

    ### written by
    [\`${one.origin.path}:${one.origin.line}\`](${one.origin.href})${toySection(one.sql)}
  ~|
  runtime.class: plan
  runtime.link: ${one.origin.href}
}
${previous} -> ${key}
`);
  previous = key;
});

if (untimed.length > 0) {
  lines.push(`unrun: |~md
  ### emitted, and this workload never ran it
  ${untimed.map((one) => `- \`${one.label}\``).join("\n  ")}

  Dead weight for \`${first.name}\`, live for some other program.
~|
unrun.class: cold
${previous} -> unrun
`);
  previous = "unrun";
}

const indexes = program.ddl
  .filter((sql) => sql.startsWith("CREATE INDEX"))
  .map((sql) => /CREATE INDEX "([^"]+)"/.exec(sql)[1]);
const ddlSite = site("v6/prolog/lower.pl", "^delta_ddl(relplan(");
lines.push(`schema: "the tables all of that writes into" {
  grid-columns: 2
  grid-gap: 20
  class: head

  tables: |\`sql
  ${program.ddl.filter((sql) => sql.startsWith("CREATE ")).slice(0, 9).map((sql) => wrapSql(sql)).join("\n\n  ")}
  \`|
  tables.class: code
  tables.link: ${ddlSite.href}

  cost: |~md
    ### what a staged row pays
    ${indexes.length} index${indexes.length === 1 ? "" : "es"}, so every staged row costs its table
    write plus one btree write per index over that table.

    ${indexes.map((name) => `- \`${name}\``).join("\n    ")}

    ### written by
    [\`${ddlSite.path}:${ddlSite.line}\`](${ddlSite.href})
  ~|
  cost.class: plan
  cost.link: ${ddlSite.href}
}
${previous} -> schema
`);

const outPath = process.argv[2] ?? join(REPO, "plans", "2026-08-06-dl6-live.d2");
writeFileSync(outPath, lines.join("\n"));
process.stderr.write(
  `dl6-doc: wrote ${outPath} (${withCost.length} timed statements, ${CHAIN.length + EMITTER.length} linked call sites)\n`,
);
