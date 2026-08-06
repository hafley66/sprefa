import Database from "libsql";
import { readdirSync } from "node:fs";
import { join } from "node:path";

const OUT = "/Users/chrishafley/projects/sprefa-lanes/emitrace/v6/tsv2/gen_emitted";
const families = new Map();

for (const file of readdirSync(OUT).filter((name) => name.endsWith(".ts"))) {
  let module;
  try { module = await import(join(OUT, file)); } catch { continue; }
  const { program, incrementalPlan } = module;
  if (!program || !incrementalPlan) continue;
  const db = new Database(":memory:");
  db.exec("PRAGMA temp_store=MEMORY;");
  try { for (const ddl of program.ddl) db.exec(ddl); } catch { db.close(); continue; }

  const queries = [];
  for (const relation of incrementalPlan.relations) queries.push(relation.boundarySql);
  for (const statement of incrementalPlan.levels) {
    if (statement.insertSql) queries.push(statement.insertSql);
    queries.push(...(statement.supportSql ?? []));
    queries.push(...(statement.aggregateSql?.scopeSeedSql ?? []));
  }
  for (const statement of incrementalPlan.edges) if (statement.insertSql) queries.push(statement.insertSql);

  const used = new Set();
  for (const raw of queries) {
    if (typeof raw !== "string") continue;
    const sql = raw.replace(/\s*RETURNING[\s\S]*$/, "").replace(/\?/g, "0");
    for (const part of sql.split(";")) {
      let plan;
      try { plan = db.prepare(`EXPLAIN QUERY PLAN ${part}`).all(); } catch { continue; }
      for (const step of plan) {
        const match = /USING (?:COVERING )?INDEX ([^\s(]+)/.exec(step.detail);
        if (match) used.add(match[1]);
      }
    }
  }

  for (const ddl of program.ddl) {
    const match = /CREATE INDEX "([^"]+)"/.exec(ddl);
    if (!match) continue;
    const name = match[1];
    const family = name.replace(/^__(delta|frontier|next_frontier|departure_frontier)_.*_(sign|group|phase)$/, "$1_$2");
    const key = family === name ? name : family;
    const seen = families.get(key) ?? { declared: 0, used: 0 };
    seen.declared += 1;
    if (used.has(name)) seen.used += 1;
    families.set(key, seen);
  }
  db.close();
}

console.log("| index family | declared | ever chosen by a query plan |");
console.log("|---|---|---|");
for (const [family, seen] of [...families].sort()) {
  console.log(`| \`${family}\` | ${seen.declared} | ${seen.used === 0 ? "**never**" : seen.used} |`);
}
