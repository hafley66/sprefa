#!/usr/bin/env node
// A10 panel perf harness. Drives the real media/flow-panel.html against the
// hermetic fixture-bridge (tests/fixture-bridge.mjs), seeded per run with a
// big rel_perf_node/rel_perf_edge layer (tests/perf/big-graph-fixture.mjs) at
// 5k/20k/50k rows (tunable via --sizes), and runs one fixed interaction
// script per size:
//
//   load (select the perf layer) -> toggle linked-only -> collapse a group
//   -> switch view (list -> canvas) -> follow a jump/center
//
// Two measurements per step:
//   - wall-clock ms (Playwright-side Date.now(), action to settle) -- always
//     present, even for steps that don't trigger a data re-render.
//   - internal perf marks, if any fired during that step: flow-panel.html's
//     run()/renderCanvas()/renderList()/refreshListView() each
//     window.postMessage({type:'perf', phase, ms, rows, drawnArcs}) (see the
//     "perf marks (A10 harness)" comment block in media/flow-panel.html).
//     These ride the SAME window-message channel graphChanged already uses
//     -- no new host coupling, nothing routed through window.dlHost.
//
// This is a plain Node script driving Playwright's browser API directly
// (`chromium.launch()`), not a `playwright test` spec -- the three fixture
// sizes each need their OWN fixture-bridge process (a fresh --perf-count),
// which doesn't fit the single static webServer array `playwright.config.ts`
// declares for the hermetic unit/e2e suite. That suite (tests/e2e/) and its
// snapshots are untouched by anything here.
//
//   node tests/perf/run-perf.mjs                    # default sizes 5000,20000,50000
//   node tests/perf/run-perf.mjs --sizes 5000,20000
//   npm run test:perf                                # wired in package.json
import { chromium } from "@playwright/test";
import { spawn } from "node:child_process";
import { setTimeout as delay } from "node:timers/promises";
import fs from "node:fs";
import http from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const testsDir = path.join(here, "..");
const resultsDir = path.join(here, "results");
const baselinePath = path.join(here, "baseline.md");

const FIXTURE_PORT = 7391;
const PANEL_PORT = 7393;
const FIXTURE_URL = `http://127.0.0.1:${FIXTURE_PORT}`;
const PANEL_URL = `http://127.0.0.1:${PANEL_PORT}`;

function parseSizes(argv) {
  const idx = argv.indexOf("--sizes");
  if (idx < 0) return [5000, 20000, 50000];
  return argv[idx + 1].split(",").map((s) => Number(s.trim())).filter(Boolean);
}

function waitForPort(port, timeoutMs = 15000) {
  const deadline = Date.now() + timeoutMs;
  return new Promise((resolve, reject) => {
    const tryOnce = () => {
      const req = http.get({ host: "127.0.0.1", port, path: "/", timeout: 1000 }, () => resolve());
      req.on("error", () => {
        if (Date.now() > deadline) reject(new Error(`timed out waiting for port ${port}`));
        else setTimeout(tryOnce, 100);
      });
    };
    tryOnce();
  });
}

function spawnFixtureBridge(count) {
  const child = spawn(
    process.execPath,
    [path.join(testsDir, "fixture-bridge.mjs"), "--port", String(FIXTURE_PORT), "--perf-count", String(count)],
    { stdio: ["ignore", "pipe", "pipe"] },
  );
  child.stdout.on("data", () => {});
  child.stderr.on("data", () => {}); // swallow "unmatched SQL" noise from gateways probe etc.
  return child;
}

function spawnPanelServer() {
  const child = spawn(
    process.execPath,
    [path.join(testsDir, "panel-server.mjs"), "--port", String(PANEL_PORT)],
    { stdio: ["ignore", "pipe", "pipe"] },
  );
  return child;
}

async function killChild(child) {
  if (!child || child.killed) return;
  child.kill();
  await delay(50);
}

// ── page-side perf log plumbing ─────────────────────────────────────────────
async function installPerfLog(page) {
  await page.addInitScript(() => {
    window.__perfLog = [];
    window.addEventListener("message", (ev) => {
      const m = ev.data;
      if (m && m.type === "perf") window.__perfLog.push(m);
    });
  });
}
async function drainPerfLog(page) {
  return page.evaluate(() => {
    const entries = window.__perfLog || [];
    window.__perfLog = [];
    return entries;
  });
}

// ── the fixed interaction script ─────────────────────────────────────────────
// Returns [{ step, wallMs, marks: [{phase, ms, rows, drawnArcs}, ...] }, ...]
async function runInteractionScript(page, size) {
  const steps = [];

  async function step(name, timeoutMs, action) {
    await drainPerfLog(page); // clear anything left over from the previous step
    const t0 = Date.now();
    await action();
    const wallMs = Date.now() - t0;
    const marks = await drainPerfLog(page);
    steps.push({ step: name, wallMs, marks });
  }

  // 1. load: navigate, wait for the default (tiny, canned) render, then
  // select the "perf" discovered layer -- the same UI path panel.spec.ts's
  // "toggling the demo layer" test exercises, just against the big fixture.
  await step("load", 30000, async () => {
    await page.goto(`${PANEL_URL}/flow-panel.html?dl=${encodeURIComponent(FIXTURE_URL)}`);
    await page.locator(".lrow-node .llabel").first().waitFor({ state: "visible", timeout: 10000 });
    const perfChip = page.locator("#layerList label.layer-chip", { hasText: "perf" });
    await perfChip.waitFor({ state: "visible", timeout: 10000 });
    await perfChip.locator("input[type=checkbox]").check();
    await page.waitForFunction(
      () => document.getElementById("countPill").textContent.includes("nodes"),
      { timeout: 30000 },
    );
    // settle: the countPill text updates synchronously with refreshListView,
    // so this is just a small margin for the last paint/microtask flush.
    await page.waitForTimeout(50);
  });

  // 2. toggle linked-only (client-side re-render from cache, no re-query).
  await step("toggle-linked-only", 10000, async () => {
    await page.locator("#linkedBtn").click();
    await page.waitForTimeout(50);
  });

  // 3. collapse a group: the SECOND foldable directory row's triangle, not
  // the first -- every generated node's file path shares the "perf/" prefix,
  // so buildRows' single-child trie compaction (media/flow-panel.html) folds
  // that whole common prefix into ONE top-level dir row ("perf") whose
  // subtree IS the entire dataset. Row index 1 is the first real per-
  // directory group underneath it (e.g. "dir0"), a representative
  // subassembly-sized fold instead of collapsing the whole tree to nothing.
  await step("collapse-a-group", 10000, async () => {
    const triangle = page.locator(".lrow-dir .ltri").nth(1);
    await triangle.waitFor({ state: "visible", timeout: 10000 });
    await triangle.click();
    await page.waitForTimeout(50);
  });

  // 4. switch view: list -> canvas (renderCanvas).
  await step("switch-view-canvas", 30000, async () => {
    await page.locator("#modeBtn").click();
    await page.waitForFunction(() => document.getElementById("modeBtn").textContent === "canvas view", {
      timeout: 15000,
    });
    await page.waitForTimeout(100); // cytoscape layout settle
  });

  // 5. follow a jump/center: turn on "follow cursor", then drive
  // window.__dlCursor (the same hook extension.ts wires for real editor
  // cursor moves) at a node this fixture generated, which centerOnNode()
  // pans/zooms the cy viewport to.
  await step("follow-jump-center", 10000, async () => {
    await page.locator("#followCursor").check();
    await page.evaluate(() => {
      window.__dlCursor({ file: "perf/dir0/file0.rs", line: 1, word: "node_0" });
    });
    await page.waitForTimeout(250); // cy.animate({...}, {duration: 220})
  });

  return { size, steps };
}

// ── main ─────────────────────────────────────────────────────────────────
async function main() {
  const sizes = parseSizes(process.argv.slice(2));
  fs.mkdirSync(resultsDir, { recursive: true });

  const panelServer = spawnPanelServer();
  await waitForPort(PANEL_PORT);

  const runs = [];
  try {
    for (const size of sizes) {
      const fixtureBridge = spawnFixtureBridge(size);
      try {
        await waitForPort(FIXTURE_PORT);
        const browser = await chromium.launch();
        try {
          const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
          await installPerfLog(page);
          const run = await runInteractionScript(page, size);
          runs.push(run);
          console.log(`[perf] size=${size} done`);
        } finally {
          await browser.close();
        }
      } finally {
        await killChild(fixtureBridge);
      }
    }
  } finally {
    await killChild(panelServer);
  }

  const stamp = new Date().toISOString().replace(/[:.]/g, "-");
  const jsonPath = path.join(resultsDir, `${stamp}.json`);
  fs.writeFileSync(jsonPath, JSON.stringify(runs, null, 2));
  console.log(`[perf] raw results: ${jsonPath}`);

  const md = renderMarkdown(runs);
  fs.writeFileSync(baselinePath, md);
  console.log(`[perf] baseline table: ${baselinePath}`);
  console.log("\n" + md);
}

// One row per (size, step): wall-clock ms, plus the single most relevant
// internal mark for that step (renderList/refreshListView/renderCanvas carry
// rows+drawnArcs, which is the useful scaling signal; "run" is folded into
// "load" since that's the only step that goes through run()). Picks the
// LAST occurrence of the preferred phase, not the first -- the "load" step
// fires renderList/refreshListView TWICE (once for the tiny default typeAll
// query on page load, again for the perf-layer switch), and the second is
// the one whose rows/drawnArcs describe the actual fixture size under test.
function pickHeadlineMark(marks) {
  const order = ["renderCanvas", "refreshListView", "renderList", "run"];
  for (const phase of order) {
    for (let i = marks.length - 1; i >= 0; i--) {
      if (marks[i].phase === phase) return marks[i];
    }
  }
  return marks[marks.length - 1] || null;
}

function renderMarkdown(runs) {
  const lines = [];
  lines.push("# Panel perf baseline");
  lines.push("");
  lines.push(
    "Generated by `npm run test:perf` (tests/perf/run-perf.mjs) against a seeded " +
      "`rel_perf_node`/`rel_perf_edge` layer (tests/perf/big-graph-fixture.mjs) at each " +
      "size below. Fixed interaction script per size: load (select perf layer) -> " +
      "toggle linked-only -> collapse a group -> switch view (list -> canvas) -> " +
      "follow a jump/center. `wall` = Playwright-side wall-clock ms for the whole " +
      "step; `mark` = the most relevant internal `performance.now()` span " +
      "flow-panel.html emits via `postMessage({type:'perf', ...})` during that step " +
      "(phase / ms / rows / drawnArcs), blank when the step fired none.",
  );
  lines.push("");
  lines.push(
    "**Render cap note**: `render()` in flow-panel.html caps actual list/canvas " +
      "rendering at `NODE_RENDER_CAP`=2000 nodes / `EDGE_RENDER_CAP`=4000 edges " +
      "regardless of dataset size (the browser-bridge `dlHost.query()` used here " +
      "doesn't forward a server-side `limit`, so the fixture bridge always returns " +
      "the full table). At 5k/20k/50k rows every size exceeds the cap, so " +
      "`renderList`/`renderCanvas`/`refreshListView`'s `rows`/`drawnArcs` plateau at " +
      "the same capped counts across all three sizes below -- that's the cap doing " +
      "its job, not a harness bug.",
  );
  lines.push("");
  lines.push(
    "**Wall-clock flatness note**: the `load` step's wall-clock time is also " +
      "essentially flat across 5k/20k/50k in this run (see the table). Layer " +
      "selection routes through `runLayers()`, which doesn't call `run()` and so " +
      "emits no `run`-phase mark for the big fetch -- the query+JSON.parse cost for " +
      "the full uncapped payload is folded into the `load` step's wall-clock number, " +
      "not broken out on its own. At these sizes over a loopback hermetic fixture, " +
      "that cost is apparently dwarfed by `discoverLayers()`'s several sequential " +
      "schema round-trips (fixed per-layer-switch overhead, independent of row " +
      "count) -- i.e. this harness did not observe wire-time scaling in this range. " +
      "A larger fixture, a real daemon over a unix socket, or breaking `runLayers()` " +
      "out with its own `emitPerf('run', ...)` call would be the next things to try " +
      "before concluding wire time never matters.",
  );
  lines.push("");
  lines.push("| size | step | wall (ms) | mark phase | mark ms | rows | drawnArcs |");
  lines.push("|---:|---|---:|---|---:|---:|---:|");
  for (const run of runs) {
    for (const s of run.steps) {
      const mark = pickHeadlineMark(s.marks);
      lines.push(
        `| ${run.size} | ${s.step} | ${s.wallMs} | ${mark ? mark.phase : ""} | ` +
          `${mark ? mark.ms.toFixed(1) : ""} | ${mark && mark.rows != null ? mark.rows : ""} | ` +
          `${mark && mark.drawnArcs != null ? mark.drawnArcs : ""} |`,
      );
    }
  }
  lines.push("");
  return lines.join("\n");
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
