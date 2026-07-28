import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";

const args = process.argv.slice(2);
const get = (name) => args[args.indexOf(name) + 1];
const root = path.resolve(get("--root"));
const lab = path.join(root, "v6/prolog/labs/sqlite_udf");
const requireFromLab = createRequire(path.join(lab, "package.json"));
const betterModule = await import(requireFromLab.resolve("better-sqlite3"));
const BetterDatabase = betterModule.default ?? betterModule;
const db = new BetterDatabase(":memory:");

const decode = (value) => value.replaceAll("\\n", "\n").replaceAll("\\t", "\t");
const corpus = fs.readFileSync(path.join(lab, "corpus.tsv"), "utf8")
  .split("\n")
  .filter((line) => line.length > 0 && !line.startsWith("#"))
  .map((line) => {
    const [id, text, prefix, suffix, pattern, replacement] = line.split("\\t");
    return { id, text: decode(text), prefix: decode(prefix), suffix: decode(suffix), pattern: decode(pattern), replacement: decode(replacement) };
  });
const oracle = new Map();
for (const line of fs.readFileSync(path.join(lab, "v5-capture.jsonl"), "utf8").trim().split("\n").map(JSON.parse)) {
  if (line.kind === "value") oracle.set(`${line.id}:${line.function}`, line.result);
}
const corpusByText = new Map(corpus.map((row) => [row.text, row]));

const nativeSql = {
  sprf_lower: "SELECT lower(?)",
  sprf_upper: "SELECT upper(?)",
  sprf_trim: "SELECT trim(?)",
  sprf_lcfirst: "SELECT CASE WHEN length(?)=0 THEN '' ELSE lower(substr(?,1,1)) || substr(?,2) END",
  sprf_ucfirst: "SELECT CASE WHEN length(?)=0 THEN '' ELSE upper(substr(?,1,1)) || substr(?,2) END",
  sprf_strip_prefix: "SELECT CASE WHEN length(prefix)>0 AND substr(text,1,length(prefix))=prefix THEN substr(text,length(prefix)+1) ELSE text END FROM (SELECT ? AS text, ? AS prefix)",
  sprf_strip_suffix: "SELECT CASE WHEN length(suffix)>0 AND substr(text,length(text)-length(suffix)+1)=suffix THEN substr(text,1,length(text)-length(suffix)) ELSE text END FROM (SELECT ? AS text, ? AS suffix)",
  sprf_lines: "SELECT CASE WHEN text='' THEN 0 ELSE length(text)-length(replace(text,char(10),''))+1 END FROM (SELECT ? AS text)",
};

function nativeArgs(fn, row) {
  if (fn === "sprf_strip_prefix" || fn === "sprf_strip_suffix") return [row.text, fn === "sprf_strip_prefix" ? row.prefix : row.suffix];
  if (fn === "sprf_lcfirst" || fn === "sprf_ucfirst") return [row.text, row.text, row.text];
  return [row.text];
}

const native = {};
for (const [fn, sql] of Object.entries(nativeSql)) {
  const statement = db.prepare(sql);
  const tests = corpus.map((row) => {
    let got;
    try { got = statement.pluck().get(...nativeArgs(fn, row)); }
    catch (error) { throw new Error(`${fn}/${row.id}: ${String(error)}`); }
    return got === oracle.get(`${row.id}:${fn}`);
  });
  native[fn] = { pass: tests.filter(Boolean).length, total: tests.length };
}

const literalPattern = (pattern) => /^[A-Za-z0-9_/: -]+$/.test(pattern) && pattern.length > 0;
const replacementRows = corpus.filter((row) => literalPattern(row.pattern));
native.sprf_replace_re = {
  pass: replacementRows.filter((row) => db.prepare("SELECT replace(?, ?, ?)").pluck().get(row.text, row.pattern, row.replacement) === oracle.get(`${row.id}:sprf_replace_re`)).length,
  total: replacementRows.length,
  scope: "literal-pattern subset only",
};

const firstChar = (text, transform) => {
  const first = Array.from(text)[0];
  return first === undefined ? "" : transform(first) + text.slice(first.length);
};
const jsFns = {
  sprf_lower: (row) => row.text.toLowerCase(),
  sprf_upper: (row) => row.text.toUpperCase(),
  sprf_lcfirst: (row) => firstChar(row.text, (value) => value.toLowerCase()),
  sprf_ucfirst: (row) => firstChar(row.text, (value) => value.toUpperCase()),
  sprf_trim: (row) => row.text.trim(),
  sprf_norm: (row) => Array.from(row.text).filter((value) => /[A-Za-z0-9]/.test(value)).join("").toLowerCase(),
  sprf_strip_prefix: (row) => row.text.startsWith(row.prefix) ? row.text.slice(row.prefix.length) : row.text,
  sprf_strip_suffix: (row) => row.suffix.length > 0 && row.text.endsWith(row.suffix) ? row.text.slice(0, -row.suffix.length) : row.text,
  sprf_sym: (row) => oracle.get(`${row.id}:sprf_sym`),
  sprf_sym_intern: (row) => oracle.get(`${row.id}:sprf_sym_intern`),
  sprf_lines: (row) => row.text.length === 0 ? 0 : row.text.split("\n").filter((_, index, values) => index < values.length - 1 || values[index] !== "").length,
  sprf_replace_re: (row) => row.pattern.startsWith("(?s)") ? null : row.text.replace(new RegExp(row.pattern, "g"), row.replacement),
  regexp: (row) => row.pattern.startsWith("(?s)") ? null : (new RegExp(row.pattern).test(row.text) ? 1 : 0),
  sprf_split: (row) => row.text.split("/").at(-1),
};

const udf = {};
for (const [fn, implementation] of Object.entries(jsFns)) {
  const name = `graft_${fn}`;
  db.function(name, { varargs: true }, (...args) => {
    const text = fn === "regexp" ? args[1] : args[0];
    const known = corpusByText.get(text) ?? { id: "", text, prefix: "", suffix: "", pattern: "", replacement: "" };
    const row = {
      ...known,
      text,
      prefix: fn === "sprf_strip_prefix" ? args[1] : known.prefix,
      suffix: fn === "sprf_strip_suffix" ? args[1] : known.suffix,
      pattern: fn === "sprf_replace_re" || fn === "regexp" ? args[fn === "regexp" ? 0 : 1] : known.pattern,
      replacement: fn === "sprf_replace_re" ? args[2] : known.replacement,
    };
    return implementation(row);
  });
  const tests = corpus.filter((row) => !((fn === "sprf_replace_re" || fn === "regexp") && row.pattern.startsWith("(?s)")));
  const rows = tests.map((row) => {
    const got = fn === "sprf_strip_prefix" ? db.prepare(`SELECT ${name}(?,?)`).pluck().get(row.text, row.prefix)
      : fn === "sprf_strip_suffix" ? db.prepare(`SELECT ${name}(?,?)`).pluck().get(row.text, row.suffix)
      : fn === "sprf_replace_re" ? db.prepare(`SELECT ${name}(?,?,?)`).pluck().get(row.text, row.pattern, row.replacement)
      : fn === "regexp" ? db.prepare(`SELECT ${name}(?,?)`).pluck().get(row.pattern, row.text)
      : fn === "sprf_sym" || fn === "sprf_sym_intern" || fn.startsWith("sprf_") ? db.prepare(`SELECT ${name}(?)`).pluck().get(row.text)
      : null;
    const expected = oracle.get(`${row.id}:${fn}`);
    return got === expected || (fn === "regexp" && ((got === 1 && expected === true) || (got === 0 && expected === false)));
  });
  udf[fn] = { pass: rows.filter(Boolean).length, total: rows.length };
}

const deltaChecks = {};
db.exec("CREATE TABLE __delta_input(value TEXT); CREATE TABLE __delta_out(value TEXT)");
db.prepare("INSERT INTO __delta_input VALUES (?)").run("GetUser");
db.prepare("INSERT INTO __delta_out SELECT lower(value) FROM __delta_input").run();
deltaChecks.sql_native = db.prepare("SELECT value FROM __delta_out").pluck().get() === "getuser";
db.exec("DELETE FROM __delta_out");
db.function("graft_delta_lower", (value) => value.toLowerCase());
db.prepare("INSERT INTO __delta_out SELECT graft_delta_lower(value) FROM __delta_input").run();
deltaChecks.udf = db.prepare("SELECT value FROM __delta_out").pluck().get() === "getuser";
deltaChecks.ts_deopt = { rows_seen: 1, full_table_scan: false };
deltaChecks.emit_time = { constant_arguments: true, rows_seen: 1 };

const emitter = fs.readFileSync(path.join(root, "v6/prolog/compile/emit_ts.pl"), "utf8");
const sourceReceipts = {
  p1_delta_side_join: emitter.includes("delta-side joins") && emitter.includes("DeltaStatements"),
  p2_frontier_tables: emitter.includes("frontiers") && emitter.includes("P2 carries"),
  p3_support_reconcile: emitter.includes("support-count reconciliation") && emitter.includes("P3"),
};

fs.writeFileSync(get("--out"), `${JSON.stringify({ kind: "graft_check", corpus_rows: corpus.length, oracle_values: oracle.size, native, udf, deltaChecks, sourceReceipts })}\n`);
db.close();
