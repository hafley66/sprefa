// e2e coverage for media/flow-panel.html driven entirely against the
// hermetic fixture-bridge (tests/fixture-bridge.mjs) -- no dl binary, no
// daemon, no VS Code. The panel's own browser-bridge dlHost fallback (the
// `if (!window.dlHost)` branch in flow-panel.html) is what makes this
// possible: `?dl=<url>` picks the RPC endpoint.
import { test, expect } from "./fixtures";

const FIXTURE_URL = "http://127.0.0.1:7381";
const PANEL_PATH = `/flow-panel.html?dl=${encodeURIComponent(FIXTURE_URL)}`;

test.describe("dl flow panel (fixture-backed)", () => {
  test("loads and discovers layer checkboxes from the fixture schema", async ({ page }) => {
    await page.goto(PANEL_PATH);
    // discoverLayers() finds rel_demo_node/_edge (a plain discovered pair)
    // plus rel_type_entity/rel_type_link (the BUILTIN_LAYERS "type" pair) --
    // two checkboxes, sorted alphabetically: demo, type.
    const layerChips = page.locator("#layerList .layer-chip");
    await expect(layerChips).toHaveCount(2);
    await expect(layerChips.nth(0)).toContainText("demo");
    await expect(layerChips.nth(1)).toContainText("type");
  });

  test("default typeAll preset renders type-entity rows in the list", async ({ page }) => {
    await page.goto(PANEL_PATH);
    await expect(page.locator(".lrow-node .llabel", { hasText: "build_widget" })).toBeVisible();
    // exact match: hasText is a case-insensitive substring test, and "Widget"
    // is a substring of "build_widget" -- an unanchored match here would hit
    // both rows and throw a strict-mode violation.
    await expect(page.locator(".lrow-node .llabel", { hasText: /^Widget$/ })).toBeVisible();
  });

  test("toggling the demo layer renders its rows in the list", async ({ page }) => {
    await page.goto(PANEL_PATH);
    await expect(page.locator(".lrow-node .llabel", { hasText: "build_widget" })).toBeVisible();

    await page
      .locator("#layerList label.layer-chip", { hasText: "demo" })
      .locator("input[type=checkbox]")
      .check();

    await expect(page.locator(".lrow-node .llabel", { hasText: "Demo Node A" })).toBeVisible();
    await expect(page.locator(".lrow-node .llabel", { hasText: "Demo Node B" })).toBeVisible();
    // layers REPLACE the query (they don't merge with whatever preset was
    // showing) -- the prior typeAll rows must be gone.
    await expect(page.locator(".lrow-node .llabel", { hasText: "build_widget" })).toHaveCount(0);
    await expect(page.locator("#presetSel")).toHaveValue("layers");
  });

  test("graphChanged with the auto toggle off does not re-query", async ({ page }) => {
    const rpcRequests: string[] = [];
    page.on("request", (req) => {
      const url = req.url();
      if (url.startsWith(FIXTURE_URL) && url.endsWith("/rpc")) rpcRequests.push(url);
    });

    await page.goto(PANEL_PATH);
    await expect(page.locator(".lrow-node .llabel", { hasText: "build_widget" })).toBeVisible();
    await expect(page.locator("#autoRefresh")).not.toBeChecked();

    const before = rpcRequests.length;
    await page.evaluate(() => window.postMessage({ type: "graphChanged" }, "*"));
    // the debounce in flow-panel.html is 250ms; give it clear margin.
    await page.waitForTimeout(450);
    expect(rpcRequests.length).toBe(before);
  });
});
