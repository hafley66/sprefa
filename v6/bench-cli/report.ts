/**
 * report.ts — turns out/records.jsonl into standings.csv + STANDINGS.md.
 *
 * Same split the house rig uses (bench/run.sh collects, bench/report.sh
 * renders): the harness measures and this file only formats, so a formatting
 * change never invalidates a measurement.
 *
 * Column vocabulary extends v6/sprefa-store/PERF-REPORT.md's -- per-case input
 * hash with the "all engines must match" check, `verdict` where PERF-REPORT
 * has `correct`, memory column, and the rule that no N/A ships without a
 * reason.
 *
 * Usage: node --experimental-transform-types report.ts <records.jsonl>
 */

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));

interface IRecord {
  readonly case: string;
  readonly family: string;
  readonly engine: string;
  readonly verdict: string;
  readonly input_hash: string;
  readonly note: string;
  readonly wall_ms: number | null;
  readonly wall_samples: readonly number[];
  readonly compile_ms: number | string;
  readonly ticks: number | string;
  readonly statements: number | string;
  readonly db_bytes: number | string;
  readonly peak_rss_mb: number | string;
  readonly engine_notes: Readonly<Record<string, string>>;
  readonly external_timer: string;
  readonly runs: number;
}

const CSV_HEADER =
  "case,family,engine,verdict,wall_ms,compile_ms,ticks,statements,peak_rss_mb,db_bytes,input_hash,note";

function cell(value: number | string | null): string {
  if (value === null) return "N/A";
  return String(value);
}

function csvField(value: string): string {
  return value.includes(",") || value.includes('"') ? `"${value.replace(/"/g, '""')}"` : value;
}

function toCsv(records: readonly IRecord[]): string {
  const lines = [CSV_HEADER];
  for (const r of records) {
    lines.push(
      [
        r.case,
        r.family,
        r.engine,
        r.verdict,
        cell(r.wall_ms),
        cell(r.compile_ms),
        cell(r.ticks),
        cell(r.statements),
        cell(r.peak_rss_mb),
        cell(r.db_bytes),
        r.input_hash,
        r.note,
      ]
        .map((v) => csvField(String(v)))
        .join(","),
    );
  }
  return `${lines.join("\n")}\n`;
}

/** The identity check PERF-REPORT states per shape: all engines, one hash. */
function hashAgreement(records: readonly IRecord[]): string[] {
  const byCase = new Map<string, Set<string>>();
  for (const r of records) {
    const seen = byCase.get(r.case) ?? new Set<string>();
    seen.add(r.input_hash);
    byCase.set(r.case, seen);
  }
  const broken: string[] = [];
  for (const [name, hashes] of byCase) {
    if (hashes.size > 1) broken.push(`${name}: ${[...hashes].join(" vs ")}`);
  }
  return broken;
}

function table(records: readonly IRecord[], family: string): string {
  const rows = records.filter((r) => r.family === family);
  if (rows.length === 0) return "";
  const names = [...new Set(rows.map((r) => r.case))];
  const out: string[] = [];
  for (const name of names) {
    const forCase = rows.filter((r) => r.case === name);
    const hash = forCase[0]?.input_hash ?? "";
    const note = forCase.find((r) => r.note.length > 0)?.note ?? "";
    out.push(`### ${name}`);
    out.push("");
    if (note.length > 0) out.push(`${note}`);
    out.push(`_input hash \`${hash}\` (all engines must match)_`);
    out.push("");
    out.push("| engine | verdict | wall ms | compile ms | ticks | stmts | peak RSS MB | db bytes |");
    out.push("|---|:---:|---:|---:|---:|---:|---:|---:|");
    for (const r of forCase) {
      out.push(
        `| ${r.engine} | ${r.verdict} | ${cell(r.wall_ms)} | ${cell(r.compile_ms)} | ${cell(r.ticks)} | ` +
          `${cell(r.statements)} | ${cell(r.peak_rss_mb)} | ${cell(r.db_bytes)} |`,
      );
    }
    out.push("");
  }
  return out.join("\n");
}

/** Every N/A in the tables gets its reason printed, once, deduplicated. */
function reasons(records: readonly IRecord[]): string {
  const seen = new Map<string, string>();
  for (const r of records) {
    for (const [field, why] of Object.entries(r.engine_notes)) {
      seen.set(`${r.engine}.${field}`, `- \`${r.engine}\` **${field}** — ${why}`);
    }
    if (r.verdict === "refused") {
      seen.set(`${r.engine}.${r.case}.refused`, `- \`${r.engine}\` on **${r.case}** — named refusal (exit 2); not timed by contract.`);
    }
    if (r.verdict === "error") {
      seen.set(`${r.engine}.${r.case}.error`, `- \`${r.engine}\` on **${r.case}** — run failed; disqualified from timing.`);
    }
    if (r.verdict === "wrong") {
      seen.set(`${r.engine}.${r.case}.wrong`, `- \`${r.engine}\` on **${r.case}** — log differs from the oracle; **not timed** (the v1 asymmetry rule).`);
    }
    if (r.verdict === "no_reference") {
      seen.set(
        `${r.engine}.${r.case}.no_reference`,
        `- \`${r.engine}\` on **${r.case}** — the ORACLE produced no reference log here, so nothing on this case can be graded. This is a ceiling of the reference engine, not a finding about \`${r.engine}\`; the engine was not run.`,
      );
    }
  }
  return [...seen.values()].sort().join("\n");
}

function main(): void {
  const path = process.argv[2];
  if (path === undefined) {
    process.stderr.write("usage: report.ts <records.jsonl>\n");
    process.exitCode = 2;
    return;
  }
  // A FILTERED run must not overwrite the committed standings: `BENCH_CASES=x
  // just bench-cli` otherwise replaces a 14-case table with a 1-case one, and
  // the loss is silent because both files are valid. Caught by doing exactly
  // that while verifying the recipe. Partial runs land in out/ instead.
  const partial = (process.env["BENCH_CASES"] ?? "").length > 0;
  const outDir = partial ? join(HERE, "out") : HERE;
  const records = readFileSync(path, "utf8")
    .split("\n")
    .filter((line) => line.trim().length > 0)
    .map((line) => JSON.parse(line) as IRecord);

  writeFileSync(join(outDir, "standings.csv"), toCsv(records));

  const timer = records[0]?.external_timer ?? "unknown";
  const runs = records[0]?.runs ?? 0;
  const broken = hashAgreement(records);
  const timed = records.filter((r) => r.verdict === "identical").length;
  const disqualified = records.filter((r) => ["wrong", "error", "refused"].includes(r.verdict)).length;

  const doc = `# bench-cli standings

Generated by \`v6/bench-cli/bench.sh\`; contract and the build-vs-buy verdict
behind these numbers: [CONTRACT.md](CONTRACT.md). Raw records are
\`out/records.jsonl\` (gitignored); the committed record is this file and
[standings.csv](standings.csv).

Machine: Apple M2 Pro, 16 GB, macOS 23.6.0, Node v24.15.0, SWI-Prolog 10.0.2 arm64-darwin.

- repeats per timed engine: **${runs}** (\`wall_ms\` is the median)
- external timer: **${timer}**
- peak RSS: \`/usr/bin/time -l maximum resident set size\`
- timed rows: **${timed}**; disqualified rows: **${disqualified}**
- input-hash agreement: **${broken.length === 0 ? "OK, every case agrees across engines" : `BROKEN — ${broken.join("; ")}`}**

**Reading rule.** \`wall_ms\` is engine-reported and measures different spans on
different engines: tsv2's excludes node startup and compile, the oracle's is
wrapper-measured and includes swipl startup. They are NOT comparable head to
head; see CONTRACT.md section 6. What is comparable across engines today is
the *verdict*, \`ticks\`, \`statements\`, and \`peak_rss_mb\`.

Where that caveat bites hardest is the small program cases, whose engine work
(4-30 ms) is the same order as the ~30 ms floor separating the two engines'
measurement spans. On the scale rows the floor is a rounding error against the
numbers, so the ordering there is the robust part of this table. Closing the
gap properly means timing inside the oracle rather than around it, which edits
a file this lane is fenced out of — priced in CONTRACT.md section 6.

**Only a byte-identical engine is timed.** A \`wrong\`, \`refused\` or \`error\`
row carries N/A timings on purpose — the v1 asymmetry from SCALE.md (ranked
~10x faster while emitting no delta log) cannot recur under this harness.

## Real programs

${table(records, "program")}
## Scale shapes

${table(records, "scale")}
## N/A and disqualification reasons

Per CONTRACT.md section 2.4 no \`N/A\` ships bare.

${reasons(records)}
`;

  writeFileSync(join(outDir, "STANDINGS.md"), doc);
  process.stdout.write(
    `BENCH-CLI timed=${timed} disqualified=${disqualified} hash-agreement=${broken.length === 0 ? "OK" : "BROKEN"}\n`,
  );
  process.stdout.write(
    `wrote ${join(outDir, "standings.csv")} and ${join(outDir, "STANDINGS.md")}` +
      `${partial ? " (PARTIAL run: BENCH_CASES was set, so the committed standings were left alone)" : ""}\n`,
  );
  if (broken.length > 0) process.exitCode = 1;
}

main();
