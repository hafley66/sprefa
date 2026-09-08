import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { basename, join } from "node:path";

if (process.argv.length !== 5) {
  throw new Error("usage: node 28_public_ivm_summarize.mjs full.jsonl semantic.jsonl output-directory");
}
const [, , fullPath, semanticPath, outputDirectory] = process.argv;
const load = async (path) => (await readFile(path, "utf8")).trim().split("\n").filter(Boolean).map(JSON.parse);
const [full, semantic] = await Promise.all([load(fullPath), load(semanticPath)]);
const median = (values) => {
  const sorted = [...values].sort((left, right) => left - right);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
};
const fixed = (value) => Number(value).toFixed(6);
const key = (row) => `${row.rows}:${row.batch_size}:${row.fanout}`;
const triples = full.filter((row) => row.event === "three-way-run" && row.status === "ok");
const cells = Map.groupBy(triples, key);
const performance = [["rows", "batch_size", "fanout", "successful_triples", "states_per_arm",
  "pg_ivm_median_ms", "sqlite_plan_refresh_median_ms", "sqlite_durable_commit_median_ms", "dd_volatile_median_ms"]];
for (const rows of cells.values()) {
  const first = rows[0];
  performance.push([first.rows, first.batch_size, first.fanout, rows.length, first.state_count_per_arm,
    fixed(median(rows.map((row) => row.pg_ivm_ms))),
    fixed(median(rows.map((row) => row.sqlite_ms))),
    fixed(median(rows.map((row) => row.sqlite_durable_commit_ms))),
    fixed(median(rows.map((row) => row.dd_ms)))]);
}

const processes = full.filter((row) => row.event === "case-process" && row.run_kind === "measured");
const totals = full.filter((row) => row.event === "case-total" && row.run_kind === "measured");
const armRows = [
  ["pg_ivm", "postgres_group_observed_peak_rss_kb"],
  ["sqlite-plan-refresh", "sqlite_process_observed_peak_rss_kb"],
  ["dd", "dd_process_observed_peak_rss_kb"],
];
const memory = [["arm", "observed_peak_rss_kb", "process_reported_peak_rss_bytes", "database_bytes_min", "database_bytes_max", "durability"]];
for (const [arm, rssField] of armRows) {
  const armProcesses = processes.filter((row) => row.maintenance === arm).map((row) => row[rssField]).filter(Number.isFinite);
  const armTotals = totals.filter((row) => row.maintenance === arm);
  const processPeaks = armTotals.map((row) => row.process_peak_rss_bytes
    ?? (row.rss_units === "bytes" ? row.process_peak_rss_platform_units : row.process_peak_rss_platform_units * 1024)).filter(Number.isFinite);
  const bytes = armTotals.map((row) => Number(row.disk.database_bytes)).filter(Number.isFinite);
  memory.push([arm, armProcesses.length ? Math.max(...armProcesses) : "",
    processPeaks.length ? Math.max(...processPeaks) : "", Math.min(...bytes), Math.max(...bytes),
    arm === "dd" ? "volatile-no-commit" : arm === "pg_ivm" ? "fsync=on;synchronous_commit=on;full_page_writes=on" : "WAL;synchronous=FULL"]);
}

const semanticTriple = semantic.find((row) => row.event === "three-way-run");
const semanticSummary = {
  status: semanticTriple?.status,
  arms: ["pg_ivm", "sqlite-plan-refresh", "dd"],
  states_per_arm: semanticTriple?.state_count_per_arm,
  exact_state_checks: (semanticTriple?.state_count_per_arm ?? 0) * 3,
  exact_mismatches: semanticTriple?.all_input_output_states_match ? 0 : null,
  final_input_hash: semanticTriple?.final_input_hash,
  final_checksum: semanticTriple?.final_checksum,
};
const metadata = full.find((row) => row.event === "run-metadata");
const sqliteSetups = full.filter((row) => row.event === "case-setup" && row.maintenance === "sqlite-plan-refresh");
const configuration = {
  runner: {
    profile: metadata.profile, budget: metadata.budget, cases: metadata.cases,
    arms: metadata.arms, warmups: metadata.warmups, repetitions: metadata.repetitions,
    timing_contract: metadata.timing_contract, memory_control: metadata.memory_control,
    postgres_tuning: metadata.tuning, machine: metadata.machine,
  },
  sqlite: sqliteSetups[0]?.sqlite_configuration,
  sqlite_algorithm: sqliteSetups[0]?.algorithm,
  sqlite_plan_sha256: sqliteSetups[0]?.plan_sha256,
};

await mkdir(outputDirectory, { recursive: true });
const outputs = {
  "performance.tsv": performance.map((row) => row.join("\t")).join("\n") + "\n",
  "memory.tsv": memory.map((row) => row.join("\t")).join("\n") + "\n",
  "semantic.json": JSON.stringify(semanticSummary, null, 2) + "\n",
  "configuration.json": JSON.stringify(configuration, null, 2) + "\n",
};
for (const [name, body] of Object.entries(outputs)) await writeFile(join(outputDirectory, name), body, { flag: "wx" });
const hashes = [];
for (const path of [fullPath, semanticPath]) {
  hashes.push(`${createHash("sha256").update(await readFile(path)).digest("hex")}  private/${basename(path)}`);
}
for (const [name, body] of Object.entries(outputs)) {
  hashes.push(`${createHash("sha256").update(body).digest("hex")}  ${name}`);
}
await writeFile(join(outputDirectory, "SHA256SUMS"), hashes.join("\n") + "\n", { flag: "wx" });
console.log(JSON.stringify({ event: "public-ivm-summary", status: "ok", outputDirectory,
  cells: cells.size, triples: triples.length, measured_state_checks: triples.length * 3 * 5 }));
