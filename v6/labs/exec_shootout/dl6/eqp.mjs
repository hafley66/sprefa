import Database from "/Users/chrishafley/projects/sprefa-lanes/emitrace/v6/tsv2/node_modules/.pnpm/libsql@0.5.29/node_modules/libsql/index.js";
import { program, incrementalPlan } from "/Users/chrishafley/projects/sprefa-lanes/emitrace/v6/labs/exec_shootout/dl6/.compiled/reachability.ts";

const db = new Database(":memory:");
db.exec("PRAGMA journal_mode=WAL;PRAGMA synchronous=NORMAL;PRAGMA temp_store=MEMORY;");
for (const ddl of program.ddl) db.exec(ddl);

for (const statement of incrementalPlan.levels) {
  if (statement.insertSql === null) continue;
  console.log(`\n### ${statement.ruleId}`);
  const bare = statement.insertSql.replace(/\s*RETURNING[\s\S]*$/, "");
  for (const step of db.prepare(`EXPLAIN QUERY PLAN ${bare}`).all()) {
    console.log(`  ${step.detail}`);
  }
}
