#!/usr/bin/env node
// nolisten-text-audit.mjs — RAIL A: every SQL naming a skipped rel's staging
// must be DDL, writer INSERT, clear DELETE, or its own boundary SELECT, or exit 2. §6.

import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const DEFAULT_DIR = new URL("../gen_emitted/", import.meta.url).pathname;

const escapeRegex = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

// A staging table name must not match as a prefix of another rel's (e.g.
// `__frontier_head` inside `__frontier_head_move`).
const hasTable = (sql, name) =>
  new RegExp(`${escapeRegex(name)}(?![A-Za-z0-9_])`).test(sql);

// Strip TS comments first so a doc comment can't read as a statement, then
// pull every backtick string that carries SQL.
function sqlStrings(text) {
  const stripped = text.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/.*$/gm, "");
  const out = [];
  const re = /`((?:[^`\\]|\\.)*)`/g;
  let m;
  while ((m = re.exec(stripped))) {
    if (/CREATE|SELECT|INSERT|DELETE|UPDATE|FROM|JOIN|EXISTS/i.test(m[1])) out.push(m[1]);
  }
  return out;
}

function stagingNames(rel) {
  return [`__delta_${rel}`, `__frontier_${rel}`, `__next_frontier_${rel}`];
}

const BOUNDARY = /"_sign"\s+IN\s+\(-1,\s*1\)/;

// The rel's own boundarySql read: SELECT over its OWN delta with the boundary
// marker. Anything else that reads a skipped rel's staging is a find.
function allowed(sql, rel) {
  const stmt = sql.trimStart();
  if (/^CREATE\b/i.test(stmt)) return true; // DDL
  if (/^INSERT\b/i.test(stmt)) return true; // writer copy
  if (/^DELETE\b/i.test(stmt)) return true; // whole-table clear
  if (/^SELECT\b/i.test(stmt)) {
    return hasTable(sql, `__delta_${rel}`) && BOUNDARY.test(sql);
  }
  return false;
}

function auditModule(path, findings, counters) {
  const text = readFileSync(path, "utf8");
  const name = path.split("/").pop();
  const block = /const INCREMENTAL_RELATIONS[\s\S]*?=\s*\[([\s\S]*?)\n\];/.exec(text);
  if (!block) return;
  // Each relation entry is one non-nested brace block. Capture the whole inner
  // text, then pull rel and ruleObservers out of it.
  const entryRe = /\{([^{}]*)\}/g;
  const unobserved = [];
  let mm;
  while ((mm = entryRe.exec(block[1]))) {
    const entry = mm[1];
    const rel = /\brel:\s*"([^"]+)"/.exec(entry)?.[1];
    if (!rel) continue;
    counters.rels++;
    if (/ruleObservers:\s*\[\s*\]/.test(entry)) unobserved.push(rel);
  }
  if (unobserved.length === 0) return;
  const statements = sqlStrings(text);
  counters.modules++;
  for (const rel of unobserved) {
    counters.unobserved++;
    const names = stagingNames(rel);
    for (const sql of statements) {
      const touched = names.filter((n) => hasTable(sql, n));
      if (touched.length === 0) continue;
      for (const table of touched) {
        counters.refs++;
        if (!allowed(sql, rel)) {
          findings.push({ name, rel, table, sql });
        }
      }
    }
  }
}

function collectPaths(args) {
  if (args.length === 0) {
    return readdirSync(DEFAULT_DIR)
      .filter((f) => f.endsWith(".ts"))
      .map((f) => join(DEFAULT_DIR, f));
  }
  const paths = [];
  for (const arg of args) {
    const stat = statSync(arg);
    if (stat.isDirectory()) {
      for (const f of readdirSync(arg).filter((x) => x.endsWith(".ts"))) {
        paths.push(join(arg, f));
      }
    } else {
      paths.push(arg);
    }
  }
  return paths;
}

const paths = collectPaths(process.argv.slice(2));
const findings = [];
const counters = { modules: 0, rels: 0, unobserved: 0, refs: 0 };
for (const path of paths) auditModule(path, findings, counters);

for (const f of findings) {
  const line = f.sql.trimStart().split("\n")[0];
  console.log(
    `[violation] module=${f.name} rel=${f.rel} table=${f.table}`,
  );
  console.log(`  ${line.slice(0, 160)}`);
}

console.log(
  `nolisten text audit: ${counters.modules} modules, ${counters.unobserved} unobserved rels (of ${counters.rels}), ${counters.refs} staging refs, ${findings.length} violation${findings.length === 1 ? "" : "s"}`,
);

process.exit(findings.length === 0 ? 0 : 2);
