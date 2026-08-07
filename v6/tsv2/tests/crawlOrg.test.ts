/**
 * crawlOrg.test.ts — the ORG CRAWL graded at fixture scale.
 *
 * `v6/dl/fixtures/crawl_org.dl6` is stopping-point program #5 written as ONE
 * program: an interval bind discovers the repository set through the `repos`
 * host, `repo_files_at` enumerates each repository at a pinned revision,
 * `repo_extract` runs the extractor against files in ANOTHER working tree, and
 * a rail derives a finding. No shell loop over repositories exists anywhere,
 * and this receipt is what says the engine agrees with the reference engine
 * while all three hosts really spawn.
 *
 * THE CORPUS is two real git repositories in a temp directory, deliberately
 * ASYMMETRIC (one file vs two, one `eval` call vs none) so that a fan-out that
 * silently dropped a repository, or fanned one repository's answer onto both,
 * changes the row counts rather than staying invisible.
 *
 * GRADING is total, not a prefix, by the runtime-bridge arc's replay shape
 * (tests/serveHost.test.ts states it in full): the interval's `bucket`, all
 * three hosts' stdout and every digest derived from them are LIVE values, so
 * `scheduleFromTicks` reads back per tick the rows the world actually pushed
 * and the oracle is fed exactly those. The diff below therefore covers every
 * column of every tick, live values carried across rather than excluded.
 *
 * WHAT REPLAY GRADING DOES NOT COVER, stated: a sabotage of the WORLD side (a
 * `git -C` dropped from a template, a mangled stdout) is replayed faithfully
 * into the oracle and stays green here. That half is graded by the explicit
 * row assertions at the bottom of this test -- the per-repository file counts
 * and the finding -- and, for the file hosts, by scripts/files.sh.
 *
 * SABOTAGE RECEIPT (run 2026-07-31, reverted): dropping `-C '{repo}'` from
 * crawl_org.dl6's repo_files_at template -- so the enumeration runs in the
 * SERVER's cwd while the oid lookup still runs in the repository -- goes RED at
 * a row assertion, `Error: timeout waiting for the rail to fire`, and NOT at
 * the oracle diff. That is the split the paragraph above predicts: replay
 * grading is blind to a world-side sabotage, and the row assertions are what
 * catch it.
 *
 * A SECOND DEFECT THIS TEST FOUND, fixed in the same landing
 * (1_host_expand.pl:unprobed_host_decls/3): the `gh_repos` declaration below is
 * written and never probed, and a declared-but-unprobed host used to produce a
 * host PLAN naming relations the compiler never DECLARED. POST /program
 * answered 200 and the served process then died on
 * `unknown rel '__host_demand_gh_repos'` out of the boot demand scan.
 *
 * NO WALL-CLOCK SLEEP drives the cadence: the program's literal is 86400 (a
 * daily crawl, the user's own spelling) and the interval runs on a
 * VirtualTimeScheduler, so one day passes in one flush.
 */

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { VirtualTimeScheduler } from "rxjs";

import {
  log_of_ticks,
  oracle_log,
  post_arrivals,
  post_program,
  request,
  schedule_from_ticks,
  start_served,
  tick_events,
} from "./serveHelpers.ts";

const CRAWL_ORG_DL6 = fileURLToPath(new URL("../../dl/fixtures/crawl_org.dl6", import.meta.url));
const EXTRACT_RELEASE = fileURLToPath(new URL("../../sprefa-extract/target/release/extract", import.meta.url));

/** One git repository with the given files, committed. `git -C` throughout, so
 *  this helper never changes the process's own directory. */
function make_repo(root: string, files: Readonly<Record<string, string>>): void {
  mkdirSync(root, { recursive: true });
  const git = (...args: readonly string[]): void => {
    execFileSync("git", ["-C", root, ...args], { stdio: "pipe" });
  };
  git("init", "-q");
  git("config", "user.email", "crawl-receipt@example.invalid");
  git("config", "user.name", "crawl receipt");
  for (const [name, body] of Object.entries(files)) writeFileSync(join(root, name), body, "utf8");
  git("add", ...Object.keys(files));
  git("commit", "-qm", "corpus");
}

function advance_virtual_seconds(scheduler: VirtualTimeScheduler, seconds: number): void {
  scheduler.maxFrames = scheduler.frame + seconds * 1000;
  scheduler.flush();
}

async function wait_until(predicate: () => boolean | Promise<boolean>, what: string, timeout_ms = 60_000): Promise<void> {
  const deadline = Date.now() + timeout_ms;
  while (!(await predicate())) {
    if (Date.now() >= deadline) throw new Error(`timeout waiting for ${what}`);
    await new Promise<void>((resolve) => setTimeout(resolve, 25));
  }
}

async function rows_of(port: number, rel: string): Promise<readonly (readonly (string | number)[])[]> {
  const answer = JSON.parse((await request(port, `/idb/${rel}`, "GET")).body) as {
    rows: readonly (readonly (string | number)[])[];
  };
  return answer.rows;
}

test(
  "the org crawl is ONE program: repos on a clock -> repo_files_at -> repo_extract, graded against the oracle",
  { skip: existsSync(EXTRACT_RELEASE) ? false : `no in-tree release extractor at ${EXTRACT_RELEASE}` },
  async () => {
    const source = readFileSync(CRAWL_ORG_DL6, "utf8");
    const corpus = mkdtempSync(join(tmpdir(), "tsv2-crawl-org-"));
    // Asymmetric on purpose: alpha has one file and one banned call, beta has
    // two files and none. A fan-out bug cannot produce these counts by luck.
    make_repo(join(corpus, "alpha"), {
      "one.ts": "export const run = () => eval('1 + 1');\n",
    });
    make_repo(join(corpus, "beta"), {
      "two.ts": "export const twice = (n: number) => n * 2;\n",
      "three.ts": "export const thrice = (n: number) => twice(n) + n;\n",
    });
    // A directory that is NOT a repository, so the host's `.git` test is doing
    // work rather than agreeing with `ls` by accident.
    mkdirSync(join(corpus, "not-a-repo"), { recursive: true });

    const scheduler = new VirtualTimeScheduler();
    const previous_extract_bin = process.env.DL_EXTRACT_BIN;
    process.env.DL_EXTRACT_BIN = EXTRACT_RELEASE;
    const served = await start_served(0, scheduler);
    try {
      const loaded = await post_program(served.port, source);
      assert.equal(loaded.statusCode, 200, loaded.body);
      const plans = JSON.parse(loaded.body) as {
        readonly hosts: readonly string[];
        readonly binds: readonly { readonly name: string; readonly literals: readonly (string | number)[] }[];
        readonly arrival_targets: readonly string[];
      };
      // Four distinct host NAMES, which is the ruling made visible: the
      // repo-scoped hosts are not modes of the unscoped ones.
      assert.deepEqual([...plans.hosts].sort(), ["gh_repos", "repo_extract", "repo_files_at", "repos"]);
      // The cadence came from the program's own rule literal, nowhere else.
      assert.deepEqual(plans.binds, [{ name: "interval", literals: [86400] }]);

      await post_arrivals(served.port, [{ rel: "want_org", sign: "add", row: [corpus] }]);
      await wait_until(() => scheduler.actions.length >= 1, "the interval to register on the injected scheduler");
      advance_virtual_seconds(scheduler, 86400);

      // Three files across two repositories is the last thing the file hosts
      // produce; the extractor then runs once per file.
      await wait_until(async () => (await rows_of(served.port, "repo_file")).length >= 3, "the file fan-out to settle");
      await wait_until(async () => (await rows_of(served.port, "banned_call")).length >= 1, "the rail to fire");
      // One more window: a count reaching its target does not by itself prove
      // no further row is in flight, and a premature schedule replay would
      // grade a truncated run.
      await wait_until(async () => {
        const before = (await rows_of(served.port, "repo_call")).length;
        await new Promise<void>((resolve) => setTimeout(resolve, 500));
        return (await rows_of(served.port, "repo_call")).length === before;
      }, "repo_call to stop growing");

      // ── the engine half: byte identity against the reference engine ───────
      const outcomes = tick_events(served.events);
      const replayed = schedule_from_ticks(outcomes, plans.arrival_targets);
      assert.equal(log_of_ticks(outcomes), oracle_log(source, replayed));

      // ── the world half: the fan-out really happened, per repository ───────
      const repos = await rows_of(served.port, "repo");
      assert.deepEqual(
        repos.map((row) => String(row[0])).sort(),
        [join(corpus, "alpha"), join(corpus, "beta")],
        "the repos host answered the two repositories and skipped not-a-repo",
      );

      const repo_files = await rows_of(served.port, "repo_file");
      const files_per_repo = new Map<string, number>();
      for (const row of repo_files) files_per_repo.set(String(row[0]), (files_per_repo.get(String(row[0])) ?? 0) + 1);
      assert.equal(files_per_repo.get(join(corpus, "alpha")), 1, "repo_file rows per repository (alpha)");
      assert.equal(files_per_repo.get(join(corpus, "beta")), 2, "repo_file rows per repository (beta)");

      // repo_files_at pins a REVISION, so its digests are blob oids out of each
      // repository's own object database -- not a hash of a file this process
      // read, and not the other repository's.
      const alpha_oid = execFileSync("git", ["-C", join(corpus, "alpha"), "rev-parse", "HEAD:one.ts"], {
        encoding: "utf8",
      }).trim();
      assert.ok(
        repo_files.some((row) => row[0] === join(corpus, "alpha") && row[1] === "one.ts" && row[2] === alpha_oid),
        `repo_files_at should report alpha's one.ts at its committed blob oid ${alpha_oid}`,
      );

      // Extraction really crossed into both repositories: alpha's `eval` and
      // beta's `twice`, and nothing from beta's callee-free two.ts.
      const calls = await rows_of(served.port, "repo_call");
      assert.deepEqual(
        calls.map((row) => [String(row[0]).slice(corpus.length + 1), String(row[1]), String(row[2])]).sort(),
        [
          ["alpha", "one.ts", "eval"],
          ["beta", "three.ts", "twice"],
        ],
        "repo_extract ran per file, in the file's own repository",
      );

      const banned = await rows_of(served.port, "banned_call");
      assert.deepEqual(
        banned.map((row) => [String(row[0]), String(row[1])]),
        [[join(corpus, "alpha"), "one.ts"]],
        "the rail found the one banned call in the org, and only that one",
      );
    } finally {
      await served.stop();
      if (previous_extract_bin === undefined) delete process.env.DL_EXTRACT_BIN;
      else process.env.DL_EXTRACT_BIN = previous_extract_bin;
      rmSync(corpus, { recursive: true, force: true });
    }
  },
);
