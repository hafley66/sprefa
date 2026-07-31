/** Renders bench records and applies the reference and timing gates. */

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));

interface IRecord {
  readonly case: string;
  readonly case_index: number;
  readonly family: string;
  readonly engine: string;
  readonly verdict: string;
  /** `oracle` = swipl graded this cell; `tsv2(proven)` = the proven reference
   *  engine did; `none` = nothing graded it. A reader never has to infer the
   *  referee from the verdict spelling, though the two always agree. */
  readonly referee: string;
  readonly input_hash: string;
  readonly note: string;
  readonly detail: string;
  readonly wall_ms: number | null;
  readonly wall_samples: readonly number[];
  readonly compile_ms: number | string;
  readonly ticks: number | string;
  readonly statements: number | string;
  readonly db_bytes: number | string;
  readonly final_hash: string;
  readonly peak_rss_mb: number | string;
  readonly engine_notes: Readonly<Record<string, string>>;
  readonly external_timer: string;
  readonly runs: number;
}

interface IReferenceProof {
  readonly valid: boolean;
  readonly reason: string;
  readonly sweep_sha: string;
  readonly counts: Readonly<Record<string, number>>;
  readonly artifacts: readonly string[];
}

const CSV_HEADER =
  "case,family,engine,verdict,referee,wall_ms,compile_ms,ticks,statements,peak_rss_mb,db_bytes,final_hash,input_hash,note";

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
        r.referee,
        cell(r.wall_ms),
        cell(r.compile_ms),
        cell(r.ticks),
        cell(r.statements),
        cell(r.peak_rss_mb),
        cell(r.db_bytes),
        r.final_hash,
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
    out.push(
      "| engine | verdict | referee | wall ms | compile ms | ticks | stmts | peak RSS MB | db bytes | final-state hash |",
    );
    out.push("|---|:---:|:---:|---:|---:|---:|---:|---:|---:|---|");
    for (const r of forCase) {
      out.push(
        `| ${r.engine} | ${r.verdict} | ${r.referee} | ${cell(r.wall_ms)} | ${cell(r.compile_ms)} | ` +
          `${cell(r.ticks)} | ${cell(r.statements)} | ${cell(r.peak_rss_mb)} | ${cell(r.db_bytes)} | ` +
          `\`${r.final_hash}\` |`,
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
      seen.set(`${r.engine}.${r.case}.error`, `- \`${r.engine}\` on **${r.case}** — run failed${r.detail.length > 0 ? ` (${r.detail})` : ""}; disqualified from timing.`);
    }
    if (r.verdict === "wrong") {
      seen.set(`${r.engine}.${r.case}.wrong`, `- \`${r.engine}\` on **${r.case}** — ${r.detail.length > 0 ? r.detail : "log differs from the referee"}; **not timed** (the v1 asymmetry rule).`);
    }
    if (r.verdict === "over_budget") {
      seen.set(
        `${r.engine}.${r.case}.over_budget`,
        `- \`${r.engine}\` on **${r.case}** — ${r.detail}. This is the reference engine's scale ceiling (ARCH \`oracle_scale_ceiling\`), not a finding about the case; grading moves to the proven referee.`,
      );
    }
    if (r.verdict === "no_reference") {
      seen.set(
        `${r.engine}.${r.case}.no_reference`,
        `- \`${r.engine}\` on **${r.case}** — nothing graded this cell: ${r.detail}. A ceiling of the REFEREE, not a finding about \`${r.engine}\`; the engine was not timed.`,
      );
    }
  }
  return [...seen.values()].sort().join("\n");
}

function readProof(path: string | undefined): IReferenceProof | null {
  if (path === undefined) return null;
  try {
    return JSON.parse(readFileSync(path, "utf8")) as IReferenceProof;
  } catch {
    return null;
  }
}

function proofSection(proof: IReferenceProof | null, records: readonly IRecord[]): string {
  const promoted = records.filter((r) => r.referee === "tsv2(proven)");
  const lost = records.filter((r) => r.verdict === "no_reference");
  if (proof === null) {
    return "**No reference-validity proof was recorded for this run.** Every cell the swipl oracle could not reach is `no_reference`.";
  }
  const counts = proof.counts;
  return [
    `- verdict: **${proof.valid ? "VALID" : "INVALID"}** — ${proof.reason}`,
    `- sweep sha: \`${proof.sweep_sha}\` (sha256 over \`manifest.json\` ‖ NUL ‖ \`run-results.json\`, first 16 hex)`,
    `- corpus: ${counts["swept"] ?? 0} fixtures swept, ${counts["compiled"] ?? 0} compiled, ` +
      `**${counts["identical"] ?? 0} tick-log identical**, ${counts["wrong"] ?? 0} wrong, ` +
      `${counts["emitted_crash"] ?? 0} emitted crashes, ${counts["rejection"] ?? 0} rejection, ` +
      `${counts["unsupported"] ?? 0} named refusals`,
    `- final-state leg of the same sweep: ${counts["final_identical"] ?? 0} identical, ${counts["final_wrong"] ?? 0} wrong`,
    `- cells graded by the proven referee in this run: **${promoted.length}**; cells nothing could grade: **${lost.length}**`,
  ].join("\n");
}

function main(): void {
  const path = process.argv[2];
  if (path === undefined) {
    process.stderr.write("usage: report.ts <records.jsonl> [reference-proof.json]\n");
    process.exitCode = 2;
    return;
  }
  // A FILTERED run must not overwrite the committed standings: `BENCH_CASES=x
  // just bench-cli` otherwise replaces a 16-case table with a 1-case one, and
  // the loss is silent because both files are valid. Caught by doing exactly
  // that while verifying the recipe. Partial runs land in out/ instead.
  const partial = (process.env["BENCH_CASES"] ?? "").length > 0;
  const outDir = partial ? join(HERE, "out") : HERE;
  const records = readFileSync(path, "utf8")
    .split("\n")
    .filter((line) => line.trim().length > 0)
    .map((line) => JSON.parse(line) as IRecord)
    // Pass 2 appends the deferred cells' rows after every pass-1 row, so the
    // records file is in EXECUTION order and the table wants CASE order.
    .sort((a, b) => a.case_index - b.case_index || a.engine.localeCompare(b.engine));

  const proof = readProof(process.argv[3]);

  writeFileSync(join(outDir, "standings.csv"), toCsv(records));

  const timer = records[0]?.external_timer ?? "unknown";
  const runs = records[0]?.runs ?? 0;
  const broken = hashAgreement(records);
  const timedVsOracle = records.filter((r) => r.verdict === "identical").length;
  const timedVsReference = records.filter((r) => r.verdict === "identical_vs_reference").length;
  const disqualified = records.filter((r) => ["wrong", "error", "refused"].includes(r.verdict)).length;
  const ungraded = records.filter((r) => r.verdict === "no_reference").length;

  const doc = `# bench-cli standings

Generated by \`v6/bench-cli/bench.sh\`; contract and the build-vs-buy verdict
behind these numbers: [CONTRACT.md](CONTRACT.md). Raw records are
\`out/records.jsonl\` (gitignored); the committed record is this file and
[standings.csv](standings.csv).

Machine: Apple M2 Pro, 16 GB, macOS 23.6.0, Node v24.15.0, SWI-Prolog 10.0.2 arm64-darwin.

- repeats per timed engine: **${runs}** (\`wall_ms\` is the median)
- external timer: **${timer}**
- peak RSS: \`/usr/bin/time -l maximum resident set size\`
- timed against swipl (\`identical\`): **${timedVsOracle}**; against the proven referee (\`identical_vs_reference\`): **${timedVsReference}**
- disqualified rows: **${disqualified}**; ungraded (\`no_reference\`) rows: **${ungraded}**
- input-hash agreement: **${broken.length === 0 ? "OK, every case agrees across engines" : `BROKEN — ${broken.join("; ")}`}**

## Reference validity (ruling \`bench_reference = proven_engine_reference\`)

${proofSection(proof, records)}

**Which referee graded a cell is never implicit.** \`identical\` means the cell
was byte-diffed against the swipl oracle, the semantic authority. \`identical_vs_reference\`
means swipl exceeded its budget there and the cell was byte-diffed against the
tsv2 adapter, which earns reference status only under the proof above. The
\`referee\` column says the same thing a second way. CONTRACT.md section 7 states
the promotion rule and the residual risk this tier accepts.

**Reading rule.** \`wall_ms\` is engine-reported and measures different spans on
different engines: tsv2's excludes node startup and compile, the oracle's is
wrapper-measured and includes swipl startup. They are NOT comparable head to
head; see CONTRACT.md section 6. What is comparable across engines today is
the *verdict*, \`ticks\`, \`statements\`, \`final_hash\` and \`peak_rss_mb\`.

Where that caveat bites hardest is the small program cases, whose engine work
(4-30 ms) is the same order as the ~30 ms floor separating the two engines'
measurement spans. On the scale rows the floor is a rounding error against the
numbers, so the ordering there is the robust part of this table. Closing the
gap properly means timing inside the oracle rather than around it, which edits
a file this lane is fenced out of — priced in CONTRACT.md section 6.

**Only an engine that reproduced the referee's log is timed.** A \`wrong\`,
\`refused\` or \`error\` row carries N/A timings on purpose — the v1 asymmetry
from SCALE.md (ranked ~10x faster while emitting no delta log) cannot recur
under this harness.

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
    `BENCH-CLI timed=${timedVsOracle + timedVsReference} (swipl ${timedVsOracle} / reference ${timedVsReference}) ` +
      `disqualified=${disqualified} ungraded=${ungraded} hash-agreement=${broken.length === 0 ? "OK" : "BROKEN"}\n`,
  );
  process.stdout.write(
    `wrote ${join(outDir, "standings.csv")} and ${join(outDir, "STANDINGS.md")}` +
      `${partial ? " (PARTIAL run: BENCH_CASES was set, so the committed standings were left alone)" : ""}\n`,
  );

  // ── the gate ──────────────────────────────────────────────────────────────
  // Extends the floor gate rather than replacing it (bench.sh still checks
  // that SOMETHING got timed). Three ways to be red, all about the referee:
  const failures: string[] = [];
  if (broken.length > 0) failures.push(`input-hash disagreement: ${broken.join("; ")}`);
  // (1) A promoted cell with no valid proof behind it. Cannot happen through
  //     bench.sh, which refuses to promote first; checked anyway because the
  //     standings are the artifact a reader trusts, and this is the one claim
  //     in them that a reader cannot verify by eye.
  const promoted = records.filter((r) => r.referee === "tsv2(proven)");
  if (promoted.length > 0 && (proof === null || !proof.valid)) {
    failures.push(
      `${promoted.length} cell(s) graded against the proven referee while the sweep proof is ${proof === null ? "MISSING" : "INVALID"}`,
    );
  }
  // (2) The reference tier was NEEDED and could not be used. The cells exist,
  //     the harness reached them, and the standings cannot say what happened
  //     in them -- a bench that quietly stops grading at scale is the v1
  //     asymmetry wearing a different hat (CONTRACT.md section 7).
  if (ungraded > 0 && (proof === null || !proof.valid)) {
    failures.push(
      `${ungraded} cell(s) are ungraded because the sweep proof is ${proof === null ? "MISSING" : `INVALID: ${proof.reason}`}` +
        " — run `just sweep` to refresh it",
    );
  }
  // (3) A cell that failed its referee. Read as a defect, never as one more
  //     column: the referee is the reason this harness exists, and a run that
  //     printed a `wrong` row and exited 0 would be reporting a divergence to
  //     a reader who is not there.
  const wrong = records.filter((r) => r.verdict === "wrong");
  for (const r of wrong) {
    failures.push(`${r.engine} on ${r.case} did not reproduce its referee (${r.referee}): ${r.detail}`);
  }
  for (const failure of failures) process.stdout.write(`BENCH-CLI GATE: ${failure}\n`);
  if (failures.length > 0) process.exitCode = 1;
}

main();
