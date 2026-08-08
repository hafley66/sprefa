import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const TIMEOUT_MS = 600_000;

function argumentValue(flag, fallback) {
  const at = process.argv.indexOf(flag);
  return at === -1 ? fallback : process.argv[at + 1];
}

const workDir = argumentValue("--work", null);
const runs = Number(argumentValue("--runs", "3"));
const armNames = argumentValue("--arms", "wor,rowid_unique,rowid_range").split(",");
const caseNames = argumentValue("--cases", "grid_10000").split(",");
if (workDir === null) {
  process.stderr.write("race: --work <dir with .tin inputs> required\n");
  process.exit(2);
}

function runOnce(caseName, armName) {
  const child = spawnSync(
    process.execPath,
    [
      join(HERE, "bench_one.mjs"),
      "--input",
      join(workDir, `${caseName}.tin`),
      "--arm",
      armName,
    ],
    { encoding: "utf8", timeout: TIMEOUT_MS },
  );
  if (child.status !== 0) return { failed: child.stderr.trim().split("\n").pop() };
  return JSON.parse(child.stdout.trim());
}

const rows = [];
for (const caseName of caseNames) {
  for (const armName of armNames) {
    let best = null;
    for (let attempt = 0; attempt < runs; attempt += 1) {
      const result = runOnce(caseName, armName);
      if (result.failed !== undefined) {
        process.stderr.write(`race: ${caseName}/${armName} ${result.failed}\n`);
        best = result;
        break;
      }
      if (best === null || result.fixpointMs < best.fixpointMs) best = result;
    }
    rows.push({ caseName, armName, ...best });
  }
}

process.stdout.write(
  `| case | arm | derived | fixpoint ms | head insert ms | intern ms | materialize ms | db MB | peak RSS MB | statements | checksum |\n`,
);
process.stdout.write(`|---|---|---|---|---|---|---|---|---|---|---|\n`);
for (const row of rows) {
  if (row.failed !== undefined) {
    process.stdout.write(`| ${row.caseName} | \`${row.armName}\` | FAILED: ${row.failed} | | | | | | | | |\n`);
    continue;
  }
  process.stdout.write(
    `| ${row.caseName} | \`${row.armName}\` | ${row.derived.toLocaleString()} | ${row.fixpointMs} | ${row.headInsertMs} | ${row.internMs} | ${row.materializeMs} | ${(row.databaseBytes / 1048576).toFixed(1)} | ${Math.round(row.peakRssKb / 1024)} | ${row.statements} | ${row.checksum === null ? "text-space" : row.checksum} |\n`,
  );
}
