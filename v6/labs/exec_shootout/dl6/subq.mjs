import Database from "/Users/chrishafley/projects/sprefa-lanes/emitrace/v6/tsv2/node_modules/.pnpm/libsql@0.5.29/node_modules/libsql/index.js";
import { program, incrementalPlan } from "./.compiled/reachability.ts";
import { readFileSync } from "node:fs";

const edges = readFileSync(process.argv[2], "utf8").split("\n")
  .map((line) => line.trim())
  .filter((line) => line.length > 0 && !line.startsWith("p "))
  .map((line) => line.split(/\s+/).map(Number));

const level = incrementalPlan.levels.find((s) => s.supportSql !== null);
const S = level.supportSql;

/** A store already carrying the full closure, so the head-side subqueries are
 *  probing 1M rows the way a steady-state tick does. */
function populated() {
  const db = new Database(":memory:");
  db.exec("PRAGMA journal_mode=WAL;PRAGMA synchronous=NORMAL;PRAGMA temp_store=MEMORY;");
  for (const ddl of program.ddl) db.exec(ddl);
  const insert = db.prepare(`INSERT OR IGNORE INTO "edge" ("source","target") VALUES (?,?)`);
  db.transaction(() => { for (const edge of edges) insert.run(edge); })();
  for (const sql of [S[0], S[1]]) db.prepare(sql).run();
  db.prepare(`INSERT OR IGNORE INTO "reachable" ("source","target","__refcount")
      SELECT "source","target","__refcount" FROM "__support_next_reachable"`).run();
  db.exec(`DELETE FROM "__delta_reachable"; DELETE FROM "__frontier_reachable"; DELETE FROM "__new_reachable"`);
  return db;
}

function time(label, db, run) {
  const started = performance.now();
  run(db);
  const ms = performance.now() - started;
  const head = db.prepare(`SELECT count(*) AS n FROM "reachable"`).get().n;
  const zeros = db.prepare(`SELECT count(*) AS n FROM "reachable" WHERE "__refcount" <= 0`).get().n;
  console.log(`${label.padEnd(38)} ${String(Math.round(ms)).padStart(5)}ms   head=${head} zero-refcount=${zeros}`);
  db.close();
}

const headSize = (() => { const db = populated(); const n = db.prepare(`SELECT count(*) AS n FROM "reachable"`).get().n; db.close(); return n; })();
console.log(`steady state: head already holds ${headSize.toLocaleString()} rows\n`);

console.log("-- the refcount UPDATE, one correlated probe per head row --");
time("shipped: x - (x - COALESCE(sub,0))", populated(), (db) => { db.prepare(S[0]).run(); db.prepare(S[1]).run(); db.prepare(S[2]).run(); });
time("U1 algebraic: COALESCE(sub,0)", populated(), (db) => {
  db.prepare(S[0]).run(); db.prepare(S[1]).run();
  db.prepare(`UPDATE "reachable" AS h SET "__refcount" = COALESCE((SELECT n."__refcount" FROM "__support_next_reachable" n WHERE n."source"=h."source" AND n."target"=h."target"), 0)`).run();
});
time("U2 zero-then-UPDATE..FROM join", populated(), (db) => {
  db.prepare(S[0]).run(); db.prepare(S[1]).run();
  db.prepare(`UPDATE "reachable" SET "__refcount" = 0`).run();
  db.prepare(`UPDATE "reachable" AS h SET "__refcount" = n."__refcount" FROM "__support_next_reachable" n WHERE n."source"=h."source" AND n."target"=h."target"`).run();
});

console.log("\n-- the antijoin that fills the new-row scratch --");
time("shipped: NOT EXISTS correlated", populated(), (db) => { db.prepare(S[0]).run(); db.prepare(S[1]).run(); db.prepare(S[6]).run(); });
time("N1 LEFT JOIN ... IS NULL", populated(), (db) => {
  db.prepare(S[0]).run(); db.prepare(S[1]).run();
  db.prepare(`INSERT INTO "__new_reachable" ("source","target","__refcount")
      SELECT n."source", n."target", n."__refcount" FROM "__support_next_reachable" n
      LEFT JOIN "reachable" h ON n."source"=h."source" AND n."target"=h."target"
      WHERE h."source" IS NULL`).run();
});
time("N2 EXCEPT, two sorted WITHOUT ROWID", populated(), (db) => {
  db.prepare(S[0]).run(); db.prepare(S[1]).run();
  db.prepare(`INSERT INTO "__new_reachable" ("source","target","__refcount")
      SELECT "source","target", 1 FROM (
        SELECT "source","target" FROM "__support_next_reachable"
        EXCEPT SELECT "source","target" FROM "reachable")`).run();
});

console.log("\n-- the retraction predicate, scanned twice today --");
time("shipped: stage then DELETE, 2 scans", populated(), (db) => {
  db.prepare(S[3]).run(); db.prepare(S[4]).run();
});
time("R1 DELETE .. RETURNING into delta", populated(), (db) => {
  const gone = db.prepare(`DELETE FROM "reachable" WHERE "__refcount" <= 0 RETURNING "source","target"`).all();
  if (gone.length > 0) {
    const insert = db.prepare(`INSERT INTO "__delta_reachable" ("_sign","_sequence","source","target") VALUES (-1,?,?,?)`);
    db.transaction(() => gone.forEach((row, index) => insert.run(index, row.source, row.target)))();
  }
});
time("R2 partial index on __refcount<=0", populated(), (db) => {
  db.exec(`CREATE INDEX "__reachable_zero" ON "reachable" ("__refcount") WHERE "__refcount" <= 0`);
  db.prepare(S[3]).run(); db.prepare(S[4]).run();
});

console.log("\n-- the recursive CTE itself --");
time("shipped: UNION inside WITH RECURSIVE", populated(), (db) => { db.prepare(S[0]).run(); db.prepare(S[1]).run(); });
time("C1 recursive arm reordered", populated(), (db) => {
  db.prepare(S[0]).run();
  db.prepare(`INSERT INTO "__support_next_reachable" ("source","target","__refcount")
    WITH RECURSIVE "closure" ("source","target") AS (
      SELECT b0."source", b0."target" FROM "edge" b0
      UNION
      SELECT b0."source", b1."target" FROM "edge" b1, "closure" b0 WHERE b1."source" = b0."target")
    SELECT "source","target", 1 FROM "closure"`).run();
});
