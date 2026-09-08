import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { graphFixture } from "./engines/1_postgres_reach.mjs";

const source = dirname(fileURLToPath(import.meta.url));
const header = "engine,nodes,edges,killed,setup_ms,retract_ms,ops,rss_mb,host_peak_mb,sqlite_hw_mb,db_mb\n";
const numeric = "swi-incr,402,667,200,1,1,0,10,N/A,N/A,N/A\n";

test("shared harness preserves failures and refuses receipt replacement", async (t) => {
  const fixture = await mkdtemp(join(tmpdir(), "sprefa-bench-harness-test-"));
  t.after(() => rm(fixture, { recursive: true, force: true }));
  const crate = join(fixture, "store");
  const bench = join(crate, "bench");
  await mkdir(join(bench, "engines"), { recursive: true });
  await mkdir(join(fixture, "tools"));
  await copyFile(join(source, "run.sh"), join(bench, "run.sh"));
  await copyFile(join(source, "engines/1_postgres_reach.mjs"), join(bench, "engines/1_postgres_reach.mjs"));
  await copyFile(join(source, "../../tools/run-capped.sh"), join(fixture, "tools/run-capped.sh"));
  for (const name of ["chart.sh", "report.sh"]) {
    await writeFile(join(bench, name), "#!/usr/bin/env bash\nexit 0\n", { mode: 0o755 });
  }
  await writeFile(join(bench, "engines/swi_incr.sh"), `#!/usr/bin/env node
const { output, exit, timeout } = JSON.parse(process.env.FIXTURE_CASE);
process.stderr.write(output);
if (timeout) setInterval(() => {}, 1000);
else process.exit(exit);
`, { mode: 0o755 });

  const cases = [
    { name: "success", output: `CSV,${numeric}`, exit: 0, expectedExit: 0, csv: numeric, status: "" },
    { name: "exit-after-csv", output: `CSV,${numeric}late failure\n`, exit: 7, expectedExit: 1, csv: "", status: "error\tprocess exited with status 7; see logs/swi-incr-2x200.log" },
    { name: "reported-error", output: `CSV,${numeric}STATUS|swi-incr|error|validation failed|unavailable|unknown\n`, exit: 0, expectedExit: 1, csv: "", status: "error\tvalidation failed" },
    { name: "timeout-after-csv", output: `CSV,${numeric}`, timeout: true, exit: 124, expectedExit: 1, csv: "", status: "timeout\tcase exceeded 1 seconds" },
    { name: "empty", output: "", exit: 0, expectedExit: 1, csv: "swi-incr,402,,,,WALL,,,N/A,N/A,N/A\n", status: "error\tno CSV or adapter status; see logs/swi-incr-2x200.log" },
    { name: "input-match", output: `INPUT|swi-incr|${graphFixture(2,200).input_hash}\nCSV,${numeric}`, exit: 0, expectedExit: 0, csv: numeric, status: "", requireHash: true },
    { name: "input-mismatch", output: `INPUT|swi-incr|wrong\nCSV,${numeric}`, exit: 0, expectedExit: 1, csv: "", status: `error\tinput hash mismatch: actual=wrong expected=${graphFixture(2,200).input_hash}`, requireHash: true },
  ];
  for (const sample of cases) {
    await t.test(sample.name, async () => {
      const output = join(crate, sample.name);
      const env = {
        ...process.env, LC_ALL: "C", LANG: "C", POSTGRES_SHOOTOUT: "0", DD_SHOOTOUT: "0",
        BENCH_ENGINE_FILTER: "swi-incr", SCALES: "2x200", BENCH_OUT: output,
        BENCH_SKIP_CELLS: "", BENCH_CELL_BUDGET_S: "1",
        SQLITE_SHOOTOUT: "0", BENCH_REQUIRE_INPUT_HASH: sample.requireHash ? "1" : "0", BENCH_BACK_STRIDE: "0",
        FIXTURE_CASE: JSON.stringify(sample),
      };
      const run = spawnSync("bash", [join(bench, "run.sh")], { env, encoding: "utf8", timeout: 10_000 });
      assert.equal(run.status, sample.expectedExit, run.stderr + run.stdout);
      const csv = await readFile(join(output, "results.csv"), "utf8");
      assert.equal(csv, header + sample.csv);
      const status = await readFile(join(output, "adapter-status.tsv"), "utf8");
      if (sample.status) assert.ok(status.includes(`swi-incr\t2x200\t${sample.status}\t`), status);
      const log = await readFile(join(output, "logs/swi-incr-2x200.log"), "utf8");
      assert.equal(log, sample.output + `EXIT_STATUS|${sample.exit}\n`);
      const rerun = spawnSync("bash", [join(bench, "run.sh")], { env, encoding: "utf8", timeout: 10_000 });
      assert.equal(rerun.status, 2, rerun.stderr);
      assert.equal(await readFile(join(output, "results.csv"), "utf8"), csv);
      assert.equal(await readFile(join(output, "adapter-status.tsv"), "utf8"), status);
      assert.equal(await readFile(join(output, "logs/swi-incr-2x200.log"), "utf8"), log);
    });
  }
});
