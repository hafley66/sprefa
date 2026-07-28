import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";

const args = process.argv.slice(2);
const get = (name) => args[args.indexOf(name) + 1];
const nodeRoot = path.resolve(get("--node-root"));
const outPath = path.resolve(get("--out"));
const requireFromRoot = createRequire(path.join(nodeRoot, "package.json"));
const load = async (name) => import(pathToFileURL(requireFromRoot.resolve(name)).href);
const results = [];

function row(result) {
  return result.rows[0]?.[0] ?? null;
}

const libsql = await load("@libsql/client");
const libsqlDb = libsql.createClient({ url: `file:${path.join(nodeRoot, "libsql-probe.db")}` });
await libsqlDb.execute("CREATE TABLE IF NOT EXISTS probe(value TEXT)");
const libsqlNames = ["createFunction", "create_function", "function", "registerFunction"];
results.push({
  candidate: "@libsql/client@0.17.4",
  version: row(await libsqlDb.execute("SELECT sqlite_version()")),
  methods: Object.fromEntries(libsqlNames.map((name) => [name, typeof libsqlDb[name]])),
});
let libsqlError = null;
try { await libsqlDb.execute("SELECT udf_probe('x')"); } catch (error) { libsqlError = String(error); }
results.at(-1).unknown_function_error = libsqlError;
await libsqlDb.execute("DROP TABLE IF EXISTS bind_probe");
await libsqlDb.execute("CREATE TABLE bind_probe(text_col TEXT, int_col INTEGER)");
await libsqlDb.execute({ sql: "INSERT INTO bind_probe VALUES (?, ?)", args: [1, 1] });
await libsqlDb.execute({ sql: "INSERT INTO bind_probe VALUES (?, ?)", args: [1n, 1n] });
results.at(-1).bind_probe = (await libsqlDb.execute(
  "SELECT text_col, typeof(text_col), int_col, typeof(int_col) FROM bind_probe ORDER BY rowid",
)).rows;
libsqlDb.close();

try {
  const betterModule = await load("better-sqlite3");
  const BetterDatabase = betterModule.default ?? betterModule;
  const betterDb = new BetterDatabase(":memory:");
  betterDb.function("udf_probe", (value) => `better:${value}`);
  results.push({ candidate: "better-sqlite3", method: "function", registered: true, result: betterDb.prepare("SELECT udf_probe(?)").pluck().get("x") });
  betterDb.close();
} catch (error) {
  results.push({ candidate: "better-sqlite3", executed: false, load_error: String(error) });
}

try {
  const sqlite3Module = await load("sqlite3");
  const sqlite3 = sqlite3Module.default ?? sqlite3Module;
  const sqlite3Db = await new Promise((resolve, reject) => {
    const db = new sqlite3.Database(":memory:", (error) => error ? reject(error) : resolve(db));
  });
  const sqlite3Names = ["function", "create_function", "createFunction", "registerFunction"];
  results.push({ candidate: "node-sqlite3", methods: Object.fromEntries(sqlite3Names.map((name) => [name, typeof sqlite3Db[name]])) });
  await new Promise((resolve) => sqlite3Db.close(() => resolve()));
} catch (error) {
  results.push({ candidate: "node-sqlite3", executed: false, load_error: String(error) });
}

const sqlJsModule = await load("sql.js");
const initSqlJs = sqlJsModule.default ?? sqlJsModule;
const SQL = await initSqlJs({ locateFile: (file) => path.join(nodeRoot, "node_modules/sql.js/dist", file) });
const sqlJsDb = new SQL.Database();
const sqlJsNames = ["create_function", "createFunction", "function"];
const sqlJsMethods = Object.fromEntries(sqlJsNames.map((name) => [name, typeof sqlJsDb[name]]));
let sqlJsResult = null;
let sqlJsRegistered = false;
if (typeof sqlJsDb.create_function === "function") {
  sqlJsDb.create_function("udf_probe", (value) => `sqljs:${value}`);
  sqlJsResult = sqlJsDb.exec("SELECT udf_probe('x')")[0].values[0][0];
  sqlJsRegistered = true;
}
results.push({ candidate: "sql.js", methods: sqlJsMethods, registered: sqlJsRegistered, result: sqlJsResult });
sqlJsDb.close();

fs.writeFileSync(outPath, `${JSON.stringify({ kind: "node_driver_probe", results })}\n`);
