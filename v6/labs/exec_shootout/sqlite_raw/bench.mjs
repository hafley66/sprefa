import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { variantNames } from "./variants.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const DEFAULT_WORK = join(HERE, "..", "dl6", ".bench");
const TIMEOUT_MS = 130_000;

const EXPECTED = {
  grid_10000: { derived: 1069200, checksum: "9d7239568960d6a8" },
  chain_10000: { derived: 9996213, checksum: "df09b2f409f8b9a8" },
  layered_10000: { derived: 9951396, checksum: "addcf85b5162b9da" },
};

function argumentValue(flag, fallback) {
  const at = process.argv.indexOf(flag);
  return at === -1 ? fallback : process.argv[at + 1];
}

const workDir = argumentValue("--work", DEFAULT_WORK);
const raceMode = process.argv.includes("--race");
const onlyCase = argumentValue("--only", null);
const caseNames = Object.keys(EXPECTED).filter(
  (name) => onlyCase === null || name === onlyCase,
);
const missing = caseNames.filter(
  (name) => !existsSync(join(workDir, `${name}.in`)),
);
if (missing.length > 0) {
  process.stderr.write(
    `bench: missing inputs ${missing.join(", ")} under ${workDir}; generate them with the shootout harness\n`,
  );
  process.exit(2);
}

function runEntrant(caseName) {
  const started = Date.now();
  const child = spawnSync(
    process.execPath,
    [join(HERE, "run.mjs"), "--input", join(workDir, `${caseName}.in`)],
    { encoding: "utf8", timeout: TIMEOUT_MS },
  );
  if (child.status !== 0) {
    return { caseName, status: `FAILED after ${Date.now() - started}ms`, stderr: child.stderr };
  }
  const events = {};
  for (const line of child.stdout.split("\n")) {
    if (line.trim().length === 0) continue;
    const event = JSON.parse(line);
    events[event.event] = event;
  }
  const foldMatch = /checksum_fold_ms=(\d+)/.exec(child.stderr);
  return {
    caseName,
    edges: events.loaded.edges,
    loadMs: events.loaded.ms,
    derived: events.fixpoint.derived,
    fixpointMs: events.fixpoint.ms,
    foldMs: foldMatch === null ? 0 : Number(foldMatch[1]),
    checksum: events.done.checksum,
    peakRssKb: events.done.peak_rss_kb,
    stderr: child.stderr.trim(),
  };
}

function runVariant(caseName, variantName) {
  const child = spawnSync(
    process.execPath,
    [
      join(HERE, "race_one.mjs"),
      "--input",
      join(workDir, `${caseName}.in`),
      "--variant",
      variantName,
    ],
    { encoding: "utf8", timeout: TIMEOUT_MS },
  );
  if (child.status !== 0) return { variant: variantName, input: caseName, status: "exceeded_130s" };
  return JSON.parse(child.stdout.trim());
}

function verdict(caseName, derived, checksum) {
  const expected = EXPECTED[caseName];
  return derived === expected.derived && checksum === expected.checksum ? "MATCH" : "MISMATCH";
}

if (raceMode) {
  process.stdout.write(`| variant | case | fixpoint ms | fold ms | rounds | statements | peak RSS MB | checksum |\n`);
  process.stdout.write(`|---|---|---|---|---|---|---|---|\n`);
  for (const variantName of variantNames) {
    for (const caseName of caseNames) {
      const result = runVariant(caseName, variantName);
      if (result.status !== undefined) {
        process.stdout.write(`| \`${variantName}\` | ${caseName} | ${result.status} | | | | | |\n`);
        continue;
      }
      process.stdout.write(
        `| \`${variantName}\` | ${caseName} | ${result.fixpointMs} | ${result.foldMs} | ${result.rounds} | ${result.statements} | ${Math.round(result.peakRssKb / 1024)} | ${verdict(caseName, result.derived, result.checksum)} |\n`,
      );
    }
  }
} else {
  process.stdout.write(`| case | edges | derived | checksum | load ms | fixpoint ms | fold ms | fp rows/sec | peak RSS |\n`);
  process.stdout.write(`|---|---|---|---|---|---|---|---|---|\n`);
  for (const caseName of caseNames) {
    const result = runEntrant(caseName);
    if (result.status !== undefined) {
      process.stdout.write(`| \`${caseName}\` | | | ${result.status} | | | | | |\n`);
      process.stderr.write(`${result.stderr}\n`);
      continue;
    }
    const perSecond = Math.round((result.derived / Math.max(result.fixpointMs, 1)) * 1000);
    process.stdout.write(
      `| \`${caseName}\` | ${result.edges.toLocaleString()} | ${result.derived.toLocaleString()} | \`${result.checksum}\` ${verdict(caseName, result.derived, result.checksum)} | ${result.loadMs} | ${result.fixpointMs} | ${result.foldMs} | ${perSecond.toLocaleString()} | ${Math.round(result.peakRssKb / 1024)} MB |\n`,
    );
    process.stderr.write(`${result.stderr}\n`);
  }
}
