import Database from "/Users/chrishafley/projects/sprefa-lanes/emitrace/v6/tsv2/node_modules/.pnpm/libsql@0.5.29/node_modules/libsql/index.js";
import { program, incrementalPlan } from "./.compiled/reachability.ts";
import { readFileSync } from "node:fs";

const mb = () => Math.round(process.resourceUsage().maxRSS / 1024);
const heapMb = () => Math.round(process.memoryUsage().heapUsed / 1048576);
const say = (label) => console.log(`${label.padEnd(34)} peakRSS=${mb()}MB  jsHeap=${heapMb()}MB`);

say("process start");
const db = new Database(":memory:");
db.exec("PRAGMA journal_mode=WAL;PRAGMA synchronous=NORMAL;PRAGMA temp_store=MEMORY;");
for (const ddl of program.ddl) db.exec(ddl);
say("schema created");

const edges = [];
for (const raw of readFileSync(process.argv[2], "utf8").split("\n")) {
  const line = raw.trim();
  if (line.length === 0 || line.startsWith("p ")) continue;
  const [source, target] = line.split(/\s+/).map(Number);
  edges.push([source, target]);
}
const insertEdge = db.prepare(`INSERT OR IGNORE INTO "edge" ("source","target") VALUES (?,?)`);
db.transaction(() => { for (const edge of edges) insertEdge.run(edge); })();
say("edges loaded");

const level = incrementalPlan.levels.find((statement) => statement.insertSql !== null);
const [clear, seed, update, stageRetract, collectZero, stageAdd, stageFrontier, , insertNew] = level.supportSql;
db.prepare(clear).run();
db.prepare(seed).run();
say("support table seeded (CTE)");
db.prepare(update).run();
db.prepare(stageRetract).run();
db.prepare(collectZero).run();
db.prepare(stageAdd).run();
say("delta staged in SQL");
db.prepare(stageFrontier).run(2);
db.prepare(insertNew).run();
say("head + frontier written, ALL IN SQL");

const boundary = db.prepare(level.selectSql.replace('SELECT', 'SELECT')).all();
say(`final select pulled ${boundary.length} rows into JS`);

const deltaRows = db.prepare(`SELECT "source","target","_sign" AS s, count(*) AS c FROM "__delta_reachable" WHERE "_sign" IN (-1,1) GROUP BY "source","target","_sign"`).all();
say(`boundary read pulled ${deltaRows.length} rows into JS`);
