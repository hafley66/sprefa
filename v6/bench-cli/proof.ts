/** Reads the sweep artifacts used by the reference-promotion gate. */

import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const SWEEP_OUT = join(HERE, "..", "prolog", "compile", "out");
const MANIFEST = join(SWEEP_OUT, "manifest.json");
const RUN_RESULTS = join(SWEEP_OUT, "run-results.json");

/** sweep.pl's per-fixture compile record. */
interface IManifestEntry {
  readonly name: string;
  readonly bucket: string;
  readonly reason: string;
}

/** sweep.ts's per-fixture run record, both grading legs. */
interface IRunEntry {
  readonly name: string;
  readonly bucket: string;
  readonly finalBucket: string;
}

interface IProofCounts {
  readonly swept: number;
  readonly compiled: number;
  readonly unsupported: number;
  readonly compiler_crash: number;
  readonly identical: number;
  readonly rejection: number;
  readonly wrong: number;
  readonly emitted_crash: number;
  readonly no_oracle_log: number;
  readonly final_identical: number;
  readonly final_wrong: number;
  readonly no_oracle_final: number;
}

interface IReferenceProof {
  readonly valid: boolean;
  readonly reason: string;
  readonly sweep_sha: string;
  readonly counts: IProofCounts;
  readonly artifacts: readonly string[];
}

function tally(buckets: readonly string[]): Readonly<Record<string, number>> {
  const counts: Record<string, number> = {};
  for (const bucket of buckets) counts[bucket] = (counts[bucket] ?? 0) + 1;
  return counts;
}

/** Identifies the exact manifest and run-results pair consumed. */
function sweepSha(): string {
  const hash = createHash("sha256");
  hash.update(readFileSync(MANIFEST));
  hash.update(Buffer.from([0]));
  hash.update(readFileSync(RUN_RESULTS));
  return hash.digest("hex").slice(0, 16);
}

const EMPTY_COUNTS: IProofCounts = {
  swept: 0, compiled: 0, unsupported: 0, compiler_crash: 0,
  identical: 0, rejection: 0, wrong: 0, emitted_crash: 0, no_oracle_log: 0,
  final_identical: 0, final_wrong: 0, no_oracle_final: 0,
};

function build(): IReferenceProof {
  let manifest: readonly IManifestEntry[];
  let runResults: readonly IRunEntry[];
  try {
    manifest = JSON.parse(readFileSync(MANIFEST, "utf8")) as readonly IManifestEntry[];
    runResults = JSON.parse(readFileSync(RUN_RESULTS, "utf8")) as readonly IRunEntry[];
  } catch (error) {
    return {
      valid: false,
      reason: `sweep artifacts missing or unreadable (${error instanceof Error ? error.message : String(error)})`,
      sweep_sha: "N/A",
      counts: EMPTY_COUNTS,
      artifacts: [MANIFEST, RUN_RESULTS],
    };
  }

  const compileBuckets = tally(manifest.map((entry) => entry.bucket));
  const runBuckets = tally(runResults.map((entry) => entry.bucket));
  const finalBuckets = tally(runResults.map((entry) => entry.finalBucket));
  const counts: IProofCounts = {
    swept: manifest.length,
    compiled: compileBuckets["compiled"] ?? 0,
    unsupported: compileBuckets["unsupported"] ?? 0,
    compiler_crash: compileBuckets["crash"] ?? 0,
    identical: runBuckets["identical"] ?? 0,
    rejection: runBuckets["rejection"] ?? 0,
    wrong: runBuckets["wrong"] ?? 0,
    emitted_crash: runBuckets["emitted_crash"] ?? 0,
    no_oracle_log: runBuckets["no_oracle_log"] ?? 0,
    final_identical: finalBuckets["final_identical"] ?? 0,
    final_wrong: finalBuckets["final_wrong"] ?? 0,
    no_oracle_final: finalBuckets["no_oracle_final"] ?? 0,
  };

  // Every condition below is a way the corpus-wide agreement could be less
  // than total. A referee is promoted on the STRENGTH of that agreement, so
  // any hole in it refuses the promotion rather than being averaged away.
  const failures: string[] = [];
  if (counts.identical === 0) failures.push("no fixture graded identical");
  if (counts.wrong > 0) failures.push(`${counts.wrong} fixture(s) WRONG vs the oracle`);
  if (counts.emitted_crash > 0) failures.push(`${counts.emitted_crash} emitted module(s) crashed`);
  if (counts.no_oracle_log > 0) failures.push(`${counts.no_oracle_log} fixture(s) have no oracle log`);
  if (counts.final_wrong > 0) failures.push(`${counts.final_wrong} fixture(s) FINAL-STATE wrong`);
  if (counts.compiler_crash > 0) failures.push(`${counts.compiler_crash} compiler crash(es)`);
  // Cross-file consistency: run-results must cover exactly the set manifest
  // calls compiled. A truncated or half-regenerated pair reads as a clean
  // sweep of a smaller corpus otherwise, which is the quiet way a proof
  // shrinks without anyone deciding to shrink it.
  if (runResults.length !== counts.compiled) {
    failures.push(`run-results covers ${runResults.length} fixtures, manifest calls ${counts.compiled} compiled`);
  }
  if (counts.identical + counts.rejection !== runResults.length) {
    failures.push(
      `identical(${counts.identical}) + rejection(${counts.rejection}) does not account for all ${runResults.length} run records`,
    );
  }

  return {
    valid: failures.length === 0,
    reason:
      failures.length === 0
        ? `sweep artifacts record total oracle agreement: ${counts.identical} identical + ${counts.rejection} rejection over ${counts.compiled} compiled of ${counts.swept} swept`
        : failures.join("; "),
    sweep_sha: sweepSha(),
    counts,
    artifacts: [MANIFEST, RUN_RESULTS],
  };
}

function main(): void {
  const proof = build();
  const outPath = process.argv[2];
  const text = `${JSON.stringify(proof, null, 2)}\n`;
  if (outPath !== undefined) writeFileSync(outPath, text);
  process.stdout.write(text);
}

main();
