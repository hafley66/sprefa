/**
 * Q1c: byte size of each top-level section of the emitted pokeapi module, and
 * the static runtime it imports. Section = one top-level `const`/`export const`
 * declaration, from its first byte to the byte before the next one.
 */

import { readFileSync, readdirSync } from "node:fs";

import { markdownTable, round } from "./common.ts";

const MODULE = "out/pokeapi_gen.ts";
const source = readFileSync(MODULE, "utf8");
const lines = source.split("\n");

const offsets: number[] = [];
let cursor = 0;
for (const line of lines) {
  offsets.push(cursor);
  cursor += Buffer.byteLength(line, "utf8") + 1;
}

interface ISection {
  readonly name: string;
  readonly line: number;
  readonly start: number;
}
const sections: ISection[] = [];
for (let index = 0; index < lines.length; index += 1) {
  const match = /^(?:export )?const ([A-Za-z_][A-Za-z0-9_]*)/.exec(lines[index]);
  if (match) sections.push({ name: match[1], line: index + 1, start: offsets[index] });
}

const totalBytes = Buffer.byteLength(source, "utf8");
const sized = sections.map((section, index) => ({
  ...section,
  bytes: (index + 1 < sections.length ? sections[index + 1].start : totalBytes) - section.start,
}));
const top = [...sized].sort((left, right) => right.bytes - left.bytes).slice(0, 12);
const accounted = sized.reduce((sum, section) => sum + section.bytes, 0);

const runtimeBytes = readdirSync("../../tsv2/runtime")
  .filter((name) => name.endsWith(".ts"))
  .reduce((sum, name) => sum + Buffer.byteLength(readFileSync(`../../tsv2/runtime/${name}`, "utf8"), "utf8"), 0);
const sourceBytes = Buffer.byteLength(readFileSync("../../tsv2/gen/pokeapi_gen.dl6", "utf8"), "utf8");

console.log(`### Q1c. Emitted pokeapi_gen.ts, ${totalBytes.toLocaleString("en-US")} bytes total, largest top-level sections\n`);
console.log(
  markdownTable(
    ["section", "line", "bytes", "share of module"],
    top.map((section) => [`\`${section.name}\``, String(section.line), section.bytes.toLocaleString("en-US"), `${round((100 * section.bytes) / totalBytes, 1).toFixed(1)}%`]),
  ),
);
console.log(
  `\n${markdownTable(
    ["measure", "bytes"],
    [
      ["emitted module total", totalBytes.toLocaleString("en-US")],
      ["accounted for by top-level consts", accounted.toLocaleString("en-US")],
      ["dl6 source `v6/tsv2/gen/pokeapi_gen.dl6`", sourceBytes.toLocaleString("en-US")],
      ["static runtime `v6/tsv2/runtime/*.ts`", runtimeBytes.toLocaleString("en-US")],
      ["emitted / source", round(totalBytes / sourceBytes, 1).toFixed(1) + "x"],
      ["emitted / static runtime", round(totalBytes / runtimeBytes, 1).toFixed(1) + "x"],
    ],
  )}`,
);
