/**
 * manifest-reason-diff.ts — the refusal-reason-level manifest diff
 * (ARCH task manifest_reason_diff).
 *
 * THE HAZARD IT NAMES. `v6/prolog/compile/out/manifest.json` records one row per
 * fixture: {name, file, bucket, reason}. Every existing gate reads the BUCKET
 * (sweep's compiled/identical/wrong counts, SCOREBOARD, the justfile expect
 * comments). Nothing read the REASON. So a change that leaves a fixture
 * `unsupported` but swaps WHICH refusal it hits — say
 * `edge_body_needs_json_destructure` becoming `level_body_goal` — moves zero
 * counters and passes every gate silently. That is a real event, not a
 * hypothetical: the fork_join lane's own first draft silently rewrote 2 edge
 * fixtures' refusal reason (same bucket, same counts, sweep green) and only a
 * manifest diff caught it.
 *
 * WHAT IT COMPARES. The working tree's manifest against `git HEAD`'s copy of the
 * same file (the manifest is tracked, so HEAD is the last committed grading
 * state). Movements are classified by how visible they already are elsewhere:
 *
 *   reason_restated    same bucket, DIFFERENT reason functor.  <-- THE HAZARD.
 *                      Invisible to every count-based gate. This is the only
 *                      class MANIFEST_DIFF_STRICT gates on.
 *   reason_args        same bucket, same functor, different arguments (a rel or
 *                      column name inside the reason moved). Informational: the
 *                      refusal is the same one, it is naming something else.
 *   bucket_moved       the bucket changed. Informational HERE because it is
 *                      already loud everywhere else — sweep's totals move, the
 *                      justfile expect comment goes stale, SCOREBOARD changes.
 *   added / removed    a fixture entered or left the corpus. Also already loud.
 *
 * WHY NOT GATE BY DEFAULT. Restating a refusal is frequently the CORRECT change
 * — the latest()-edge and keyed-divergence arcs both moved fixtures onto sharper
 * named refusals on purpose, and the struct arc moved 2 json fixtures onto
 * decode_source_not_struct. So the default is print-and-exit-0, and regen with
 * intent stays a one-command operation. `MANIFEST_DIFF_STRICT=1` is for the
 * reviewer who wants the run to FAIL on an unexplained restatement.
 *
 * Usage:
 *   node --experimental-transform-types scripts/manifest-reason-diff.ts
 *   MANIFEST_DIFF_STRICT=1 node --experimental-transform-types scripts/manifest-reason-diff.ts
 * Runs at the tail of scripts/sweep.sh, which regenerates the manifest first.
 *
 * SABOTAGE RECEIPT (mandatory per the dispatch, run 2026-07-31 against the real
 * 267-row manifest): the reason functor of ONE fixture was hand-edited in the
 * working-tree manifest, `keyed_level_head(current_value/2)` ->
 * `keyed_head_on_level_rule(current_value/2)`, leaving its bucket, its file and
 * every other row untouched -- the exact same-bucket-same-counts shape the
 * fork_join lane hit. Default mode printed
 *
 *   MANIFEST_REASON_DIFF restated=1 args=0 bucket_moved=0 added=0 removed=0 (informational)
 *     RESTATED keyed_level_head_is_refused [unsupported]
 *                HEAD keyed_level_head(current_value/2)
 *                WORK keyed_head_on_level_rule(current_value/2)
 *
 * and exited 0; MANIFEST_DIFF_STRICT=1 printed the same and exited 1. The edit
 * was reverted (`git checkout -- ...manifest.json`) and the check re-run clean
 * (restated=0, exit 0 both modes). Two negative controls were run in the same
 * sitting so the check is not merely reason-blind-to-nothing: editing a reason's
 * ARGUMENT only (`(current_value/2)` -> `(other_rel/2)`) reported args=1
 * restated=0 and exited 0 under STRICT, and flipping a row's BUCKET
 * (compiled -> unsupported) reported bucket_moved=1 restated=0 and exited 0
 * under STRICT -- both correctly outside the gated class. All three were
 * re-confirmed after normalizeVars landed.
 *
 * FINDING, from the first real `just sweep` run of this check: the manifest is
 * NOT byte-reproducible. 27 of the 267 rows quote a rule body containing unbound
 * variables, and swipl numbers those from a global counter, so a regen with zero
 * semantic change rewrites 27 rows (`_494306` -> `_489630` ...) and `git diff`
 * on the manifest is correspondingly noisy for them. That churn predates this
 * check. `normalizeVars` erases the numbering for comparison, which is what makes
 * the informational half of this output readable; a fix at the source would be
 * numbervars-ing the reason term in sweep.pl before it is written, which is a
 * compiler-file change and outside this lane's fence.
 */

import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

/** One row of v6/prolog/compile/out/manifest.json, as sweep.pl writes it. */
interface ManifestEntry {
  readonly name: string;
  readonly file: string;
  readonly bucket: string;
  readonly reason: string;
}

interface Movement {
  readonly name: string;
  readonly bucket: string;
  readonly head: string;
  readonly work: string;
}

interface IManifestReasonDiff {
  /** `foo(a,b)` -> `foo`; a bare atom is its own functor; "" stays "". */
  reasonFunctor(reason: string): string;
  /**
   * Erase SWI's variable NUMBERING (`_494306` -> `_`) before comparing reason
   * arguments. Reasons that quote a rule body carry unbound variables, and swipl
   * numbers those from a global counter, so 27 of the 267 rows come out with
   * different numbers on every regen for identical input. Without this, `args`
   * reports those 27 on every single run and the informational half of the output
   * is unreadable. It does not touch the gated class: `restated` compares
   * functors, which never contained a variable.
   */
  normalizeVars(reason: string): string;
  /** Parse a manifest's JSON text into name -> entry. A repeated name with a
   *  DIFFERENT row is two fixtures sharing a name, and throws. */
  index(json: string, origin: string): Map<string, ManifestEntry>;
  /** Classify every difference between two indexed manifests. */
  classify(
    head: Map<string, ManifestEntry>,
    work: Map<string, ManifestEntry>,
  ): {
    restated: Movement[];
    args: Movement[];
    bucketMoved: Movement[];
    added: string[];
    removed: string[];
  };
}

export const ManifestReasonDiff: IManifestReasonDiff = {
  reasonFunctor(reason) {
    const open = reason.indexOf("(");
    return (open === -1 ? reason : reason.slice(0, open)).trim();
  },

  normalizeVars(reason) {
    return reason.replace(/_\d+/g, "_");
  },

  index(json, origin) {
    const rows = JSON.parse(json) as ManifestEntry[];
    const byName = new Map<string, ManifestEntry>();
    for (const row of rows) {
      const seen = byName.get(row.name);
      if (seen !== undefined) {
        if (seen.file === row.file && seen.bucket === row.bucket && seen.reason === row.reason) continue;
        throw new Error(`${origin}: duplicate fixture name ${row.name}`);
      }
      byName.set(row.name, row);
    }
    return byName;
  },

  classify(head, work) {
    const restated: Movement[] = [];
    const args: Movement[] = [];
    const bucketMoved: Movement[] = [];
    const added: string[] = [];
    const removed: string[] = [];

    for (const [name, workRow] of work) {
      const headRow = head.get(name);
      if (headRow === undefined) {
        added.push(name);
        continue;
      }
      if (headRow.bucket !== workRow.bucket) {
        bucketMoved.push({
          name,
          bucket: `${headRow.bucket} -> ${workRow.bucket}`,
          head: headRow.reason,
          work: workRow.reason,
        });
        continue; // a bucket move is already loud; do not double-report its reason
      }
      if (this.normalizeVars(headRow.reason) === this.normalizeVars(workRow.reason)) continue;
      const movement: Movement = { name, bucket: workRow.bucket, head: headRow.reason, work: workRow.reason };
      if (this.reasonFunctor(headRow.reason) === this.reasonFunctor(workRow.reason)) args.push(movement);
      else restated.push(movement);
    }
    for (const name of head.keys()) if (!work.has(name)) removed.push(name);

    return { restated, args, bucketMoved, added, removed };
  },
};

// ── driver ───────────────────────────────────────────────────────────────────

const MANIFEST_PATH = "../prolog/compile/out/manifest.json";
const MANIFEST_GIT_PATH = "v6/prolog/compile/out/manifest.json";

function headManifest(): string | undefined {
  try {
    return execFileSync("git", ["show", `HEAD:${MANIFEST_GIT_PATH}`], { encoding: "utf8" });
  } catch {
    return undefined; // not tracked at HEAD (first landing, or a detached tree)
  }
}

function printMovements(label: string, movements: readonly Movement[]): void {
  for (const m of movements) {
    console.log(`  ${label} ${m.name} [${m.bucket}]`);
    console.log(`             HEAD ${m.head === "" ? "(none)" : m.head}`);
    console.log(`             WORK ${m.work === "" ? "(none)" : m.work}`);
  }
}

const strict = process.env["MANIFEST_DIFF_STRICT"] === "1";

const headText = headManifest();
if (headText === undefined) {
  console.log("MANIFEST_REASON_DIFF skipped: no HEAD copy of the manifest to compare against");
  process.exit(0);
}

const head = ManifestReasonDiff.index(headText, "HEAD manifest");
const work = ManifestReasonDiff.index(readFileSync(MANIFEST_PATH, "utf8"), "working-tree manifest");
const { restated, args, bucketMoved, added, removed } = ManifestReasonDiff.classify(head, work);

const mode = strict ? "STRICT" : "informational";
console.log(
  `MANIFEST_REASON_DIFF restated=${restated.length} args=${args.length} ` +
    `bucket_moved=${bucketMoved.length} added=${added.length} removed=${removed.length} (${mode})`,
);
printMovements("RESTATED", restated);
printMovements("ARGS    ", args);
printMovements("BUCKET  ", bucketMoved);
for (const name of added) console.log(`  ADDED    ${name} [${work.get(name)?.bucket}]`);
for (const name of removed) console.log(`  REMOVED  ${name} [${head.get(name)?.bucket}]`);

if (restated.length > 0) {
  console.log(
    "  ^ a refusal REASON changed while its bucket did not: no count-based gate sees this.",
  );
  console.log(
    "    Intended? regenerate and commit. Unintended? the compiler now refuses for a different cause.",
  );
}
if (strict && restated.length > 0) process.exit(1);
