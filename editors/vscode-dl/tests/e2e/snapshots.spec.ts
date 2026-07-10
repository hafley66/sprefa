// Visual snapshots, DOM views only. Canvas/cytoscape view is deliberately
// NOT snapshotted: its force-directed layout settles slightly differently
// run to run even with reducedMotion:'reduce' (playwright.config.ts) --
// exactly the nondeterminism toHaveScreenshot is bad at absorbing. List view
// and trace view are both pure DOM, synchronously derived from the fixture's
// canned rows, so they snapshot clean.
import { test, expect } from "./fixtures";
import type { Page } from "@playwright/test";

const FIXTURE_URL = "http://127.0.0.1:7381";
const PANEL_PATH = `/flow-panel.html?dl=${encodeURIComponent(FIXTURE_URL)}`;

// The hover card (#hoverCard) is driven by mouseover + async host.hover()/
// host.query() calls keyed on cursor position -- left on, a screenshot could
// race its fade-in. Turn the "hover card" proto chip off (persists only for
// this page) and park the mouse in empty toolbar space so nothing can be
// mid-transition when the screenshot fires.
async function settleForSnapshot(page: Page) {
  const hoverCardChip = page.locator("#protoRow .chip", { hasText: "hover card" });
  if (await hoverCardChip.count()) {
    const isOn = await hoverCardChip.evaluate((el) => el.classList.contains("on"));
    if (isOn) await hoverCardChip.click();
  }
  await page.mouse.move(2, 2);
}

test.describe("dl flow panel snapshots (fixture-backed)", () => {
  test("list view", async ({ page }) => {
    await page.goto(PANEL_PATH);
    await expect(page.locator(".lrow-node .llabel", { hasText: "build_widget" })).toBeVisible();
    await settleForSnapshot(page);
    await expect(page.locator("#listport")).toHaveScreenshot("list-view.png");
  });

  test("trace view", async ({ page }) => {
    await page.goto(PANEL_PATH);
    // build_widget -[uses]-> Widget: pinning the caller gives a deterministic
    // 2-row forward trace (depth 0 = build_widget, depth 1 = Widget).
    const callerLabel = page.locator(".lrow-node .llabel", { hasText: "build_widget" });
    await expect(callerLabel).toBeVisible();
    await callerLabel.click();

    // NOTE: #traceChip's "disabled" CSS class is a rendering snapshot from
    // the last buildProtoRow() call (page load, in this case) and does not
    // live-update on pin/unpin -- it still reads "disabled" here even though
    // pinned.size is now 1. It's cosmetic only (.chip.disabled sets
    // opacity/cursor, not pointer-events), and enterTraceMode() itself checks
    // the real pinned.size, so the click below still works. Not asserting on
    // the class -- it would be asserting a known-stale value.
    const traceChip = page.locator("#traceChip");
    await traceChip.click();

    await expect(page.locator("#modeBtn")).toHaveText("trace view");
    await expect(page.locator("#traceRows .lrow-node")).toHaveCount(2);
    await settleForSnapshot(page);
    await expect(page.locator("#tracePort")).toHaveScreenshot("trace-view.png");
  });
});
