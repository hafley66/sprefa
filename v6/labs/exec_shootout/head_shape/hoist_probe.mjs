import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";

const HERE = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
const LIBSQL_PATH = join(
  HERE, "..", "..", "..", "tsv2", "node_modules", ".pnpm",
  "libsql@0.5.29", "node_modules", "libsql", "index.js",
);
const libsqlModule = require(LIBSQL_PATH);
const Database = libsqlModule.default ?? libsqlModule;

function argumentValue(flag, fallback) {
  const at = process.argv.indexOf(flag);
  return at === -1 ? fallback : process.argv[at + 1];
}

const rowCount = Number(argumentValue("--rows", "1000000"));
const database = new Database(":memory:");
database.exec(`PRAGMA page_size=16384`);
database.exec(`PRAGMA temp_store=MEMORY`);
database.exec(
  `CREATE TABLE "__str" ("__id" INTEGER PRIMARY KEY, "content" TEXT NOT NULL UNIQUE)`,
);
database.exec(
  `CREATE TABLE "body_page" ("id" INTEGER NOT NULL, PRIMARY KEY ("id")) WITHOUT ROWID`,
);

const dictionaryWords = ["page", "ok", "warning", "rust", "acme"];
database.transaction(() => {
  const insert = database.prepare(`INSERT INTO "__str" ("content") VALUES (?)`);
  for (const word of dictionaryWords) insert.run(word);
  const insertBody = database.prepare(`INSERT INTO "body_page" ("id") VALUES (?)`);
  for (let index = 0; index < rowCount; index += 1) insertBody.run(index);
})();

const pageIdentifier = database
  .prepare(`SELECT "__id" FROM "__str" WHERE "content" = 'page'`)
  .get().__id;

const headDdl = (name) =>
  `CREATE TABLE "${name}" ("id" INTEGER NOT NULL, "tag" INTEGER NOT NULL, PRIMARY KEY ("id", "tag")) WITHOUT ROWID`;

// The three write-side spellings of the same literal: the emitted scalar
// subquery, the §5.3 fallback bind parameter, and a spliced constant.
const shapes = {
  subquery: `INSERT OR IGNORE INTO "body_tag" ("id", "tag") SELECT b0."id", (SELECT s."__id" FROM "__str" s WHERE s."content" = 'page') FROM "body_page" b0`,
  bind: `INSERT OR IGNORE INTO "body_tag" ("id", "tag") SELECT b0."id", ? FROM "body_page" b0`,
  spliced: `INSERT OR IGNORE INTO "body_tag" ("id", "tag") SELECT b0."id", ${pageIdentifier} FROM "body_page" b0`,
};

// EXPLAIN holds a read cursor for the life of the statement, so it runs on a
// connection of its own and never shares one with a timed transaction.
const explainDatabase = new Database(":memory:");
explainDatabase.exec(
  `CREATE TABLE "__str" ("__id" INTEGER PRIMARY KEY, "content" TEXT NOT NULL UNIQUE)`,
);
explainDatabase.exec(
  `CREATE TABLE "body_page" ("id" INTEGER NOT NULL, PRIMARY KEY ("id")) WITHOUT ROWID`,
);
explainDatabase.exec(headDdl("body_tag"));

process.stdout.write(`## EXPLAIN QUERY PLAN, emitted literal subquery\n\n`);
for (const row of explainDatabase.prepare(`EXPLAIN QUERY PLAN ${shapes.subquery}`).all()) {
  process.stdout.write(`    ${row.detail}\n`);
}
process.stdout.write(`\n## EXPLAIN opcodes gating the subquery\n\n`);
const opcodes = explainDatabase.prepare(`EXPLAIN ${shapes.subquery}`).all();
for (const row of opcodes) {
  if (["Once", "Init", "OpenRead", "SeekGE", "IdxGT", "Column", "Goto"].includes(row.opcode)) {
    process.stdout.write(
      `    addr=${row.addr} ${row.opcode} p1=${row.p1} p2=${row.p2} p3=${row.p3} ${row.comment ?? ""}\n`,
    );
  }
}
const onceCount = opcodes.filter((row) => row.opcode === "Once").length;
process.stdout.write(`\n    Once opcodes: ${onceCount}\n`);

database.exec(headDdl("body_tag"));
process.stdout.write(`\n## Timing, ${rowCount.toLocaleString()} rows, best of 3\n\n`);
process.stdout.write(`| spelling | ms |\n|---|---|\n`);
for (const [name, sql] of Object.entries(shapes)) {
  let best = Infinity;
  for (let attempt = 0; attempt < 3; attempt += 1) {
    database.exec(`DELETE FROM "body_tag"`);
    const statement = database.prepare(sql);
    const startedAt = performance.now();
    database.transaction(() => {
      if (name === "bind") statement.run(pageIdentifier);
      else statement.run();
    })();
    const elapsed = performance.now() - startedAt;
    if (elapsed < best) best = elapsed;
    const stored = database.prepare(`SELECT count(*) AS rows FROM "body_tag"`).get().rows;
    if (stored !== rowCount) throw new Error(`${name} stored ${stored}`);
  }
  process.stdout.write(`| \`${name}\` | ${Math.round(best)} |\n`);
}

// The read-side decode: a correlated subquery per row against a value
// comparison the id space makes unnecessary.
process.stdout.write(`\n## Read side, decode vs id comparison, best of 3\n\n`);
database.exec(`DELETE FROM "body_tag"`);
database
  .prepare(
    `INSERT OR IGNORE INTO "body_tag" ("id", "tag") SELECT b0."id", ${pageIdentifier} FROM "body_page" b0`,
  )
  .run();
const readShapes = {
  decode_per_row: `SELECT count(*) AS n FROM "body_tag" t WHERE (SELECT s."content" FROM "__str" s WHERE s."__id" = t."tag") = 'page'`,
  id_compare: `SELECT count(*) AS n FROM "body_tag" t WHERE t."tag" = (SELECT s."__id" FROM "__str" s WHERE s."content" = 'page')`,
};
process.stdout.write(`| spelling | ms | rows |\n|---|---|---|\n`);
for (const [name, sql] of Object.entries(readShapes)) {
  let best = Infinity;
  let seen = 0;
  for (let attempt = 0; attempt < 3; attempt += 1) {
    const statement = database.prepare(sql);
    const startedAt = performance.now();
    seen = statement.get().n;
    const elapsed = performance.now() - startedAt;
    if (elapsed < best) best = elapsed;
  }
  process.stdout.write(`| \`${name}\` | ${Math.round(best)} | ${seen.toLocaleString()} |\n`);
}
process.stdout.write(`\n## EXPLAIN QUERY PLAN, read side decode\n\n`);
for (const row of explainDatabase.prepare(`EXPLAIN QUERY PLAN ${readShapes.decode_per_row}`).all()) {
  process.stdout.write(`    ${row.detail}\n`);
}
process.stdout.write(`\n## EXPLAIN QUERY PLAN, read side id comparison\n\n`);
for (const row of explainDatabase.prepare(`EXPLAIN QUERY PLAN ${readShapes.id_compare}`).all()) {
  process.stdout.write(`    ${row.detail}\n`);
}
explainDatabase.close();
database.close();
