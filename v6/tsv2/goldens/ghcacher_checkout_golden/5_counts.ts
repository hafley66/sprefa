// Grades the sha gate as COUNTS over one tick log, beside the byte diff.
//
// A byte diff proves the log did not move. It does not say WHY each tick has
// the shape it has, so an edit that keeps the bytes while losing the meaning
// would pass it. Every number below is therefore named and printed.

import { readFileSync } from "node:fs";

const DEMAND_REL = "__host_demand_repo_checkout";
const RESPONSE_REL = "__host_response_repo_checkout";
const IDENTITY_COLUMN = 0;
const WITNESS_COLUMN = 1;
const SLUG_COLUMN = 2;

interface IDelta {
  readonly add?: readonly (readonly unknown[])[];
  readonly del?: readonly (readonly unknown[])[];
}

interface ITickLine {
  readonly tick: number;
  readonly deltas: Readonly<Record<string, IDelta>>;
}

interface IFinalLine {
  readonly final: Readonly<Record<string, readonly (readonly unknown[])[]>>;
}

interface IDemandExpectation {
  readonly tick: number;
  readonly slug: string;
  readonly add: number;
  readonly del: number;
  readonly why: string;
}

interface IArrivalExpectation {
  readonly tick: number;
  readonly rows: number;
  readonly why: string;
}

const EXPECTED_DEMAND: readonly IDemandExpectation[] = [
  { tick: 1, slug: "cli/cli", add: 1, del: 0, why: "first appearance, cloned once" },
  { tick: 1, slug: "gh/gh", add: 1, del: 0, why: "first appearance, cloned once" },
  { tick: 3, slug: "cli/cli", add: 0, del: 0, why: "clock moved, sha did not" },
  { tick: 3, slug: "gh/gh", add: 0, del: 0, why: "clock moved, sha did not" },
  { tick: 4, slug: "cli/cli", add: 1, del: 1, why: "sha moved, exactly one witness" },
  { tick: 4, slug: "gh/gh", add: 0, del: 0, why: "neighbour sha did not move" },
  { tick: 6, slug: "cli/cli", add: 1, del: 1, why: "sha returned, witness already answered" },
  { tick: 6, slug: "gh/gh", add: 0, del: 0, why: "neighbour sha did not move" },
];

const EXPECTED_ARRIVALS: readonly IArrivalExpectation[] = [
  { tick: 2, rows: 2, why: "both first clones answer" },
  { tick: 3, rows: 0, why: "nothing was asked" },
  { tick: 6, rows: 0, why: "the returning sha is served from the stored answer" },
];

function readLog(path: string): {
  readonly ticks: readonly ITickLine[];
  readonly final: IFinalLine;
} {
  const lines = readFileSync(path, "utf8").split("\n").filter((line) => line.length > 0);
  const ticks: ITickLine[] = [];
  let final: IFinalLine | undefined;
  for (const line of lines) {
    const parsed = JSON.parse(line) as ITickLine | IFinalLine;
    if ("tick" in parsed) ticks.push(parsed);
    else final = parsed;
  }
  if (final === undefined) throw new Error("tick log carries no final line");
  return { ticks, final };
}

function rowsFor(
  ticks: readonly ITickLine[],
  tick: number,
  rel: string,
  sign: "add" | "del",
): readonly (readonly unknown[])[] {
  const line = ticks.find((candidate) => candidate.tick === tick);
  if (line === undefined) throw new Error(`tick log has no tick ${tick}`);
  return line.deltas[rel]?.[sign] ?? [];
}

function countNaming(
  ticks: readonly ITickLine[],
  tick: number,
  sign: "add" | "del",
  slug: string,
): number {
  return rowsFor(ticks, tick, DEMAND_REL, sign).filter(
    (row) => row[SLUG_COLUMN] === slug,
  ).length;
}

function allDemandRows(
  ticks: readonly ITickLine[],
  final: IFinalLine,
): readonly (readonly unknown[])[] {
  const rows: (readonly unknown[])[] = [];
  for (const line of ticks) {
    rows.push(...(line.deltas[DEMAND_REL]?.add ?? []));
    rows.push(...(line.deltas[DEMAND_REL]?.del ?? []));
  }
  rows.push(...(final.final[DEMAND_REL] ?? []));
  return rows;
}

function main(): void {
  const [logFile, label] = process.argv.slice(2);
  if (logFile === undefined || label === undefined) {
    process.stderr.write("usage: node 5_counts.ts <tick.jsonl> <door-label>\n");
    process.exitCode = 2;
    return;
  }
  const { ticks, final } = readLog(logFile);
  const failures: string[] = [];

  process.stdout.write(`-- ${label}: ${DEMAND_REL} rows per tick --\n`);
  process.stdout.write("tick  repo_slug  add  del  reading\n");
  for (const expectation of EXPECTED_DEMAND) {
    const add = countNaming(ticks, expectation.tick, "add", expectation.slug);
    const del = countNaming(ticks, expectation.tick, "del", expectation.slug);
    const verdict = add === expectation.add && del === expectation.del ? "" : "  <-- MISMATCH";
    process.stdout.write(
      `${String(expectation.tick).padStart(4)}  ${expectation.slug.padEnd(9)}  ` +
        `${String(add).padStart(3)}  ${String(del).padStart(3)}  ${expectation.why}${verdict}\n`,
    );
    if (verdict !== "") {
      failures.push(
        `tick ${expectation.tick} ${expectation.slug}: expected add=${expectation.add} ` +
          `del=${expectation.del}, measured add=${add} del=${del}`,
      );
    }
  }

  process.stdout.write(`-- ${label}: ${RESPONSE_REL} arrivals per tick --\n`);
  for (const expectation of EXPECTED_ARRIVALS) {
    const rows = rowsFor(ticks, expectation.tick, RESPONSE_REL, "add").length;
    const verdict = rows === expectation.rows ? "" : "  <-- MISMATCH";
    process.stdout.write(
      `tick ${expectation.tick}  rows=${rows}  ${expectation.why}${verdict}\n`,
    );
    if (verdict !== "") {
      failures.push(
        `tick ${expectation.tick} arrivals: expected ${expectation.rows}, measured ${rows}`,
      );
    }
  }

  // The clone-once property: the directory a checkout lands in is a pure
  // function of the identity columns, so one repository must never present a
  // second identity digest however often its branch moves.
  const identityBySlug = new Map<string, Set<string>>();
  for (const row of allDemandRows(ticks, final)) {
    const slug = String(row[SLUG_COLUMN]);
    const identities = identityBySlug.get(slug) ?? new Set<string>();
    identities.add(String(row[IDENTITY_COLUMN]));
    identityBySlug.set(slug, identities);
  }
  process.stdout.write(`-- ${label}: distinct identity digests per repo --\n`);
  for (const slug of [...identityBySlug.keys()].sort()) {
    const count = identityBySlug.get(slug)!.size;
    const verdict = count === 1 ? "" : "  <-- MISMATCH";
    process.stdout.write(`${slug.padEnd(9)}  identities=${count}${verdict}\n`);
    if (count !== 1) {
      failures.push(`${slug}: expected 1 identity digest, measured ${count}`);
    }
  }

  // The returning sha re-asks a witness the host already answered, byte for
  // byte, which is what makes tick 6 need no arrival.
  const firstWitness = rowsFor(ticks, 1, DEMAND_REL, "add").find(
    (row) => row[SLUG_COLUMN] === "cli/cli",
  )?.[WITNESS_COLUMN];
  const returnedWitness = rowsFor(ticks, 6, DEMAND_REL, "add").find(
    (row) => row[SLUG_COLUMN] === "cli/cli",
  )?.[WITNESS_COLUMN];
  const witnessMatch = firstWitness !== undefined && firstWitness === returnedWitness;
  process.stdout.write(
    `-- ${label}: tick 6 witness equals tick 1 witness: ${witnessMatch}\n`,
  );
  process.stdout.write(`   ${String(returnedWitness)}\n`);
  if (!witnessMatch) {
    failures.push(
      `tick 6 witness ${String(returnedWitness)} does not equal tick 1 witness ${String(firstWitness)}`,
    );
  }

  if (failures.length > 0) {
    for (const failure of failures) process.stderr.write(`COUNT FAIL ${failure}\n`);
    process.exitCode = 1;
    return;
  }
  process.stdout.write(`SHA_GATE_COUNTS_HOLD door=${label}\n`);
}

main();
