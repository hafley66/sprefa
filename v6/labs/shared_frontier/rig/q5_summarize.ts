/**
 * Reads the newest .cpuprofile under out/prof and prints self-time by
 * function, top frames only. `--cpu-prof` samples the JS thread; libsql's
 * native SQLite work appears under the binding call frame.
 */

import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

import { markdownTable, round } from "./common.ts";

interface INode {
  readonly id: number;
  readonly callFrame: { readonly functionName: string; readonly url: string };
  readonly children?: readonly number[];
}

const dir = "out/prof";
const files = readdirSync(dir)
  .filter((name) => name.endsWith(".cpuprofile"))
  .map((name) => ({ name, mtime: statSync(join(dir, name)).mtimeMs }))
  .sort((left, right) => right.mtime - left.mtime);
if (files.length === 0) throw new Error("no .cpuprofile in out/prof");

const profile = JSON.parse(readFileSync(join(dir, files[0].name), "utf8")) as {
  nodes: readonly INode[];
  samples: readonly number[];
  timeDeltas: readonly number[];
};

const byId = new Map(profile.nodes.map((node) => [node.id, node]));
const selfMicros = new Map<number, number>();
for (let index = 0; index < profile.samples.length; index += 1) {
  const id = profile.samples[index];
  selfMicros.set(id, (selfMicros.get(id) ?? 0) + (profile.timeDeltas[index] ?? 0));
}

const byFunction = new Map<string, number>();
for (const [id, micros] of selfMicros) {
  const node = byId.get(id);
  if (!node) continue;
  const url = node.callFrame.url.replace(/^.*\/(?=[^/]+$)/, "");
  const key = `${node.callFrame.functionName || "(anonymous)"} in ${url || "(native)"}`;
  byFunction.set(key, (byFunction.get(key) ?? 0) + micros);
}

const totalMicros = [...byFunction.values()].reduce((sum, value) => sum + value, 0);
const top = [...byFunction.entries()].sort((left, right) => right[1] - left[1]).slice(0, 15);

console.log(`### Q5b. CPU profile self time, ${files[0].name}, ${round(totalMicros / 1000, 1)} ms sampled\n`);
console.log(
  markdownTable(
    ["frame", "self ms", "share"],
    top.map(([key, micros]) => [key, round(micros / 1000, 1).toFixed(1), `${round((100 * micros) / totalMicros, 1).toFixed(1)}%`]),
  ),
);
