import Database from "/Users/chrishafley/projects/sprefa-lanes/emitrace/v6/tsv2/node_modules/.pnpm/libsql@0.5.29/node_modules/libsql/index.js";
import { program, incrementalPlan } from "./.compiled/reachability.ts";

const db = new Database(":memory:");
db.exec("PRAGMA temp_store=MEMORY;");
for (const ddl of program.ddl) db.exec(ddl);

const queries = new Map();
for (const relation of incrementalPlan.relations) {
  queries.set(`boundarySql ${relation.rel}`, relation.boundarySql);
}
for (const statement of incrementalPlan.levels) {
  if (statement.insertSql) queries.set(`insertSql ${statement.headRel}`, statement.insertSql.replace(/\s*RETURNING[\s\S]*$/, ""));
  for (const [index, sql] of (statement.supportSql ?? []).entries()) {
    if (/FROM|WHERE/.test(sql)) queries.set(`supportSql[${index}] ${statement.headRel}`, sql.replace(/\?/g, "0"));
  }
}
for (const statement of incrementalPlan.edges) {
  if (statement.insertSql) queries.set(`edge ${statement.headRel}`, statement.insertSql.replace(/\s*RETURNING[\s\S]*$/, ""));
}

const used = new Set();
for (const [name, sql] of queries) {
  let plan;
  try { plan = db.prepare(`EXPLAIN QUERY PLAN ${sql}`).all(); } catch { continue; }
  for (const step of plan) {
    const match = /USING (?:COVERING )?INDEX ([^\s(]+)/.exec(step.detail);
    if (match) { used.add(match[1]); }
  }
}

const declared = program.ddl
  .filter((sql) => sql.startsWith("CREATE INDEX"))
  .map((sql) => /CREATE INDEX "([^"]+)"/.exec(sql)[1]);

console.log("| index | read by any emitted query? |");
console.log("|---|---|");
for (const name of declared) {
  console.log(`| \`${name}\` | ${used.has(name) ? "YES" : "**never**"} |`);
}
