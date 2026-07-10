import { defineConfig, devices } from "@playwright/test";

// Three hermetic servers, no dl binary/daemon involved:
//  - fixture-bridge.mjs answers the panel's query_sql RPCs from a canned
//    in-memory table map (tests/fixture-bridge.mjs).
//  - panel-server.mjs serves media/flow-panel.html with DL_CYTOSCAPE filled
//    in, DL_HOST left alone so the panel's browser-bridge dlHost fallback
//    talks to the fixture bridge via ?dl=.
//  - BOM_FIXTURE_PORT: a SECOND fixture-bridge instance (--bom) carrying
//    ONLY the rel_bom_node/rel_bom_edge pair (tests/e2e/bom-fixture.mjs),
//    for explode.spec.ts (Track C, C3). Kept off FIXTURE_PORT deliberately:
//    rel_bom_node/rel_bom_edge would register as a 3rd discoverable
//    _node/_edge layer pair there and break panel.spec.ts's "exactly 2
//    layer chips" assertion. explode.spec.ts points its own ?dl= at this
//    port; every other spec keeps using FIXTURE_PORT untouched.
const FIXTURE_PORT = 7381;
const PANEL_PORT = 7382;
const BOM_FIXTURE_PORT = 7383;

export default defineConfig({
  testDir: "./tests/e2e",
  // Single worker: both webServers are fixed-port singletons this suite
  // owns for the run, and the list/trace/graphChanged assertions are
  // ordering-sensitive enough (shared fixture state, deterministic click
  // sequences) that parallel workers buy nothing here.
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: [["list"]],
  snapshotPathTemplate: "{testDir}/snapshots/{testFilePath}/{arg}{ext}",
  use: {
    baseURL: `http://127.0.0.1:${PANEL_PORT}`,
    viewport: { width: 1280, height: 800 },
    reducedMotion: "reduce",
    trace: "retain-on-failure",
  },
  expect: {
    // DOM-only snapshots (list view, trace view) -- canvas/cytoscape view is
    // deliberately NOT snapshotted (layout is force-directed and settles
    // slightly differently run to run even with reducedMotion). A small
    // nonzero ratio absorbs font-antialiasing drift between runs on the
    // same machine; see tests/README.md.
    toHaveScreenshot: { maxDiffPixelRatio: 0.02 },
  },
  webServer: [
    {
      command: `node tests/fixture-bridge.mjs --port ${FIXTURE_PORT}`,
      port: FIXTURE_PORT,
      reuseExistingServer: !process.env.CI,
    },
    {
      command: `node tests/panel-server.mjs --port ${PANEL_PORT}`,
      port: PANEL_PORT,
      reuseExistingServer: !process.env.CI,
    },
    {
      command: `node tests/fixture-bridge.mjs --port ${BOM_FIXTURE_PORT} --bom`,
      port: BOM_FIXTURE_PORT,
      reuseExistingServer: !process.env.CI,
    },
  ],
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] } },
  ],
});
