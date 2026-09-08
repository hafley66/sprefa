import { execFileSync, spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { arch, hostname, platform, release, totalmem } from "node:os";
import { dirname, join } from "node:path";

function argument(name, fallback) {
  const index = process.argv.indexOf(`--${name}`);
  return index === -1 ? fallback : process.argv[index + 1];
}

function caseKey(testCase) {
  return `${testCase.rows}:${testCase.batch_size}:${testCase.fanout}`;
}

function buildCases(profile) {
  if (profile === "smoke") {
    return [
      { stage: "smoke", rows: 400, batch_size: 10, fanout: 10 },
      { stage: "smoke", rows: 400, batch_size: 10, fanout: 200 },
    ];
  }
  if (profile === "diagnostic") {
    return [{ stage: "diagnostic", rows: 160_000, batch_size: 10, fanout: 200 }];
  }
  const cases = [];
  for (const fanout of [10, 200]) {
    for (const rows of [400, 1_200, 4_000, 12_000]) {
      cases.push({ stage: "batch10-row-sweep", rows, batch_size: 10, fanout });
    }
    for (const batchSize of [1, 100, 1_000]) {
      cases.push({ stage: "batch-sweep", rows: 12_000, batch_size: batchSize, fanout });
    }
  }
  for (const rows of [40_000, 160_000]) {
    cases.push({ stage: "larger-batch10", rows, batch_size: 10, fanout: 200 });
  }
  return cases;
}

function descendantsRss(rootPid) {
  if (!rootPid) return null;
  try {
    const rows = execFileSync("/bin/ps", ["-axo", "pid=,ppid=,rss="], { encoding: "utf8" })
      .trim()
      .split("\n")
      .map((line) => line.trim().split(/\s+/).map(Number));
    const descendants = new Set([Number(rootPid)]);
    let changed = true;
    while (changed) {
      changed = false;
      for (const [pid, ppid] of rows) {
        if (descendants.has(ppid) && !descendants.has(pid)) {
          descendants.add(pid);
          changed = true;
        }
      }
    }
    return rows.filter(([pid]) => descendants.has(pid)).reduce((sum, row) => sum + row[2], 0);
  } catch {
    return null;
  }
}

function hostSwap() {
  try {
    return execFileSync("/usr/sbin/sysctl", ["vm.swapusage"], { encoding: "utf8" }).trim();
  } catch {
    return null;
  }
}

async function fileSha256(path) {
  return createHash("sha256").update(await readFile(path)).digest("hex");
}

const profile = argument("profile", "smoke");
const budget = argument("budget", "constrained");
const outputPath = argument("output", "out/crossover.jsonl");
const runRoot = process.env.IVM_RUN_ROOT;
if (!runRoot) throw new Error("IVM_RUN_ROOT is required");
if (!new Set(["smoke", "full", "diagnostic"]).has(profile)) throw new Error(`bad profile: ${profile}`);
const timeoutMs = Math.min(120_000, Number(argument("timeout-ms", "120000")));
const deadlineEpochMs = Number(argument("deadline-epoch-ms", String(Date.now() + 20 * 60_000)));
const warmups = Number(argument("warmups", profile === "full" ? "1" : "0"));
const repetitions = Number(argument("repetitions", profile === "full" ? "3" : "1"));
const cases = buildCases(profile);
const arms = ["query", "pg_ivm"];
const records = [];
const startedAt = Date.now();
let failed = false;

function append(record) {
  records.push(record);
  process.stdout.write(`${JSON.stringify(record)}\n`);
}

const labDir = new URL(".", import.meta.url);
const baselineFiles = ["results/smoke.jsonl", "results/scale.jsonl", "results/scale-summary.tsv"];
const baseline = [];
for (const file of baselineFiles) {
  baseline.push({ file, sha256: await fileSha256(new URL(file, labDir)) });
}

append({
  event: "run-metadata",
  status: "ok",
  profile,
  budget,
  command: process.argv.join(" "),
  cases,
  arms,
  warmups,
  repetitions,
  timeout_ms: timeoutMs,
  total_benchmark_deadline_epoch_ms: deadlineEpochMs,
  memory_control: {
    total_memory_enforcement: "UNENFORCED",
    total_memory_limit_bytes: null,
    cgroup_swap_limit_bytes: null,
    cgroup_oom_events: null,
    cgroup_page_cache_bytes: null,
    hard_cap_axis: "blocked",
    reason: "Docker daemon unavailable and no existing Podman, Colima, Lima, or other Linux runtime",
    observed_group_metric: "sum of resident KiB for the disposable PostgreSQL postmaster process tree",
  },
  tuning: {
    shared_buffers: process.env.IVM_SHARED_BUFFERS,
    work_mem: process.env.IVM_WORK_MEM,
    effective_cache_size: process.env.IVM_EFFECTIVE_CACHE_SIZE,
    maintenance_work_mem: process.env.IVM_MAINTENANCE_WORK_MEM,
    temp_file_limit: process.env.IVM_TEMP_FILE_LIMIT,
  },
  host_swap_start: hostSwap(),
  machine: { hostname: hostname(), platform: platform(), release: release(), arch: arch(), total_memory_bytes: totalmem() },
  server_startup_ms: Number(process.env.IVM_SERVER_STARTUP_MS ?? "0"),
  node_version: process.version,
  retained_baseline: baseline,
});

async function runProcess(testCase, maintenance, runKind, repetition) {
  const context = {
    arm: `native-${maintenance}`,
    maintenance,
    run_kind: runKind,
    repetition,
    profile,
    budget,
    stage: testCase.stage,
    rows: testCase.rows,
    batch_size: testCase.batch_size,
    fanout: testCase.fanout,
  };
  if (Date.now() >= deadlineEpochMs) {
    append({ event: "case-status", status: "skipped", reason: "20-minute total benchmark execution budget exhausted", ...context });
    return false;
  }
  const processStarted = process.hrtime.bigint();
  const childRoot = join(runRoot, `${budget}-${maintenance}-${caseKey(testCase)}-${runKind}-${repetition}`);
  await mkdir(childRoot, { recursive: true });
  const args = [
    "11_crossover_native.mjs",
    "--maintenance", maintenance,
    "--rows", String(testCase.rows),
    "--batch", String(testCase.batch_size),
    "--fanout", String(testCase.fanout),
    "--budget", budget,
    "--diagnostic", profile === "diagnostic" ? "1" : "0",
  ];
  const child = spawn(process.execPath, args, {
    cwd: labDir,
    env: {
      ...process.env,
      PGDATABASE: maintenance === "query" ? process.env.PGDATABASE_NATIVE_QUERY : process.env.PGDATABASE_NATIVE_IVM,
      NODE_OPTIONS: `${process.env.NODE_OPTIONS ?? ""} --max-old-space-size=1024`.trim(),
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  let stdout = "";
  let stderr = "";
  let timedOut = false;
  let observedGroupPeakRssKb = 0;
  child.stdout.setEncoding("utf8");
  child.stderr.setEncoding("utf8");
  child.stdout.on("data", (chunk) => { stdout += chunk; });
  child.stderr.on("data", (chunk) => { stderr += chunk; });
  const timer = setTimeout(() => {
    timedOut = true;
    child.kill("SIGKILL");
  }, Math.min(timeoutMs, Math.max(1, deadlineEpochMs - Date.now())));
  const memoryTimer = setInterval(() => {
    const rss = descendantsRss(process.env.IVM_POSTMASTER_PID);
    if (rss !== null) observedGroupPeakRssKb = Math.max(observedGroupPeakRssKb, rss);
  }, 50);
  const exit = await new Promise((resolve) => child.on("exit", (code, signal) => resolve({ code, signal })));
  clearTimeout(timer);
  clearInterval(memoryTimer);
  const processWallMs = Number(process.hrtime.bigint() - processStarted) / 1_000_000;
  if (timedOut) {
    append({ event: "case-status", status: "timeout", reason: `process exceeded ${timeoutMs} ms or total benchmark deadline`, postgres_group_observed_peak_rss_kb: observedGroupPeakRssKb, ...context });
    return false;
  }
  if (exit.code !== 0) {
    append({
      event: "case-status",
      status: "error",
      exit_code: exit.code,
      signal: exit.signal,
      stderr: stderr.slice(0, 8_000),
      postgres_group_observed_peak_rss_kb: observedGroupPeakRssKb,
      ...context,
    });
    return false;
  }
  for (const line of stdout.split("\n").filter(Boolean)) {
    try {
      const parsed = JSON.parse(line);
      append({ ...parsed, ...context });
      if (parsed.status !== "ok") failed = true;
    } catch {
      append({ event: "case-status", status: "error", reason: "non-JSON stdout", line: line.slice(0, 2_000), ...context });
      return false;
    }
  }
  append({
    event: "case-process",
    status: "ok",
    process_wall_ms: processWallMs,
    postgres_group_observed_peak_rss_kb: observedGroupPeakRssKb,
    ...context,
  });
  return true;
}

for (const testCase of cases) {
  for (const maintenance of arms) {
    for (let warmup = 1; warmup <= warmups; warmup += 1) {
      if (!await runProcess(testCase, maintenance, "warmup", warmup)) failed = true;
    }
    for (let repetition = 1; repetition <= repetitions; repetition += 1) {
      if (!await runProcess(testCase, maintenance, "measured", repetition)) failed = true;
    }
  }
}

if (profile !== "diagnostic") {
  for (const testCase of cases) {
    for (let repetition = 1; repetition <= repetitions; repetition += 1) {
      const matching = records.filter((record) => record.event === "case-total"
        && record.run_kind === "measured"
        && record.repetition === repetition
        && caseKey(record) === caseKey(testCase));
      const query = matching.find((record) => record.maintenance === "query");
      const ivm = matching.find((record) => record.maintenance === "pg_ivm");
      if (!query || !ivm) {
        append({ event: "paired-run", status: "unmeasured", reason: "one or both arms have no successful case-total record", ...testCase, budget, profile, repetition });
        continue;
      }
      append({
        event: "paired-run",
        status: "ok",
        ...testCase,
        budget,
        profile,
        repetition,
        full_query_update_plus_query_ms: query.update_plus_query_ms,
        ivm_update_plus_query_ms: ivm.update_plus_query_ms,
        full_query_over_ivm_speedup: query.update_plus_query_ms / ivm.update_plus_query_ms,
        query_final_checksum: query.final_checksum,
        ivm_final_checksum: ivm.final_checksum,
        checksum_match: query.final_checksum === ivm.final_checksum,
      });
    }
  }
}

append({
  event: "run-done",
  status: failed ? "error" : "ok",
  profile,
  budget,
  benchmark_wall_ms: Date.now() - startedAt,
  host_swap_end: hostSwap(),
});

await mkdir(dirname(outputPath), { recursive: true });
await writeFile(outputPath, `${records.map((record) => JSON.stringify(record)).join("\n")}\n`);
if (failed) process.exitCode = 1;
