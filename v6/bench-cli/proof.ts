/**
 * proof.ts — the reference-validity proof the promotion rule consumes.
 *
 * Ruling `bench_reference = proven_engine_reference` (rulings.pl, user
 * 2026-07-31): the big-scale referee is a pinned engine that EARNS reference
 * status by being byte-proven against the swipl oracle over the entire
 * oracle-reachable corpus. This file reads that proof; it does not create it.
 *
 * WHY IT READS RATHER THAN RUNS. The proof already exists on disk as the
 * sweep's own artifacts -- `v6/prolog/compile/out/manifest.json` (compile
 * bucket per fixture, written by sweep.pl) and `out/run-results.json` (tick-log
 * bucket + final-state bucket per compiled fixture, written by
 * v6/tsv2/scripts/sweep.ts). Re-running 191 fixtures inside a bench run would
 * be a second, slower copy of `just sweep` whose only new information is that
 * the sweep still passes. So the breadth half of the proof is consumed, with
 * the exact bytes it was read from identified by `sweep_sha`.
 *
 * WHAT THIS FILE DOES NOT PROVE, and who does. Artifacts are a claim about the
 * tree AT SWEEP TIME. Nothing here can tell whether the compiler or the runtime
 * moved since. That half of the promotion rule is proved by the bench run
 * itself and lives in bench.sh: every case where the swipl oracle DID reach is
 * graded against swipl in the same run, by the same adapter that would act as
 * referee at scale, and a single `wrong` there refuses the whole reference
 * tier. Breadth from the artifacts, currency from the run. Neither alone is
 * the promotion rule; CONTRACT.md section 7 states both halves together.
 *
 * Exit code is 0 whether the proof is valid or not: validity is DATA the
 * harness reads (`.valid`), not a signal that this script failed. The gate
 * that turns an invalid proof into a red run is report.ts's, so that the
 * standings are written first and the reader can see WHICH cells were lost.
 *
 * Usage: node --experimental-transform-types proof.ts [outPath]
 */

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

/** Identity of the exact proof consumed, not a checksum to compare against a
 *  stored value: SCOREBOARD.md records that manifest.json is NOT byte-stable
 *  across sweeps (27 lines carry raw prolog variable names that shift run to
 *  run), so this sha changes whenever the sweep is re-run and that is correct
 *  -- it names WHICH sweep these standings leaned on. */
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
