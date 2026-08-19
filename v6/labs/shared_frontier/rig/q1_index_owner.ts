/**
 * Q1e: which table each explicit CREATE INDEX in the emitted pokeapi DDL
 * belongs to. Autoindexes from UNIQUE clauses are not counted; they carry no
 * statement of their own.
 */

import { readFileSync } from "node:fs";

import { markdownTable, transientFamily } from "./common.ts";
import { extractDdl } from "./emitted.ts";

const ddl = extractDdl(readFileSync("out/pokeapi_gen.ts", "utf8"));
const indexes = ddl.filter((statement) => /^CREATE (UNIQUE )?INDEX/i.test(statement));

const byOwner = new Map<string, number>();
for (const statement of indexes) {
  const owner = /ON "([^"]+)"/.exec(statement)?.[1] ?? "";
  const family = transientFamily(owner) ?? "durable tables";
  byOwner.set(family, (byOwner.get(family) ?? 0) + 1);
}

console.log(`### Q1e. Index ownership in the pokeapi DDL, ${indexes.length} explicit CREATE INDEX statements\n`);
console.log(
  markdownTable(
    ["index owner", "statements"],
    [...byOwner.entries()].sort((left, right) => right[1] - left[1]).map(([owner, count]) => [owner, String(count)]),
  ),
);
