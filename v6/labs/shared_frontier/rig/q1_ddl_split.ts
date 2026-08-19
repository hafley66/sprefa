/**
 * Q1d: split the emitted pokeapi DDL into durable and per-relation transient
 * statements, and price the arm-B replacement (durable statements unchanged,
 * two shared CREATEs in place of every transient one).
 */

import { readFileSync } from "node:fs";

import { isTransient, markdownTable, round } from "./common.ts";
import { ARM_B_TRANSIENT_DDL } from "./schema.ts";
import { ddlTarget, extractDdl } from "./emitted.ts";

const ddl = extractDdl(readFileSync("out/pokeapi_gen.ts", "utf8"));

let durableBytes = 0;
let durableCount = 0;
let transientBytes = 0;
let transientCount = 0;
for (const statement of ddl) {
  const bytes = Buffer.byteLength(statement, "utf8");
  if (isTransient(ddlTarget(statement))) {
    transientBytes += bytes;
    transientCount += 1;
  } else {
    durableBytes += bytes;
    durableCount += 1;
  }
}

const sharedBytes = ARM_B_TRANSIENT_DDL.reduce((sum, statement) => sum + Buffer.byteLength(statement, "utf8"), 0);
const total = durableBytes + transientBytes;
const projected = durableBytes + sharedBytes;

console.log("### Q1d. pokeapi DDL split, and the arm-B projection\n");
console.log(
  markdownTable(
    ["group", "statements", "bytes", "share"],
    [
      ["durable (typed tables, dictionaries, catalog, their indexes and views)", String(durableCount), durableBytes.toLocaleString("en-US"), `${round((100 * durableBytes) / total, 1).toFixed(1)}%`],
      ["per-relation transient (__delta_, __frontier_, __next_frontier_, __support_next_, __new_ and their indexes)", String(transientCount), transientBytes.toLocaleString("en-US"), `${round((100 * transientBytes) / total, 1).toFixed(1)}%`],
      ["total emitted DDL", String(ddl.length), total.toLocaleString("en-US"), "100.0%"],
      ["arm-B shared replacement", String(ARM_B_TRANSIENT_DDL.length), sharedBytes.toLocaleString("en-US"), `${round((100 * sharedBytes) / total, 4).toFixed(4)}%`],
      ["arm-B projected DDL total", String(durableCount + ARM_B_TRANSIENT_DDL.length), projected.toLocaleString("en-US"), `${round((100 * projected) / total, 1).toFixed(1)}%`],
    ],
  ),
);
