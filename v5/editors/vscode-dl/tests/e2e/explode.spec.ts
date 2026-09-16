// e2e coverage for the "exploded" list mode (Track C, C3): tier-major
// stratum reorder + welded-cluster cards, driven against a DEDICATED fixture
// port (BOM_FIXTURE_PORT, see playwright.config.ts) carrying only
// rel_bom_node/rel_bom_edge (tests/e2e/bom-fixture.mjs) -- kept off the
// shared FIXTURE_PORT panel.spec.ts's layer-discovery count depends on (see
// bom-fixture.mjs's own header comment for why).
import { test, expect } from "./fixtures";

const BOM_FIXTURE_URL = "http://127.0.0.1:7383";
const PANEL_PATH = `/flow-panel.html?dl=${encodeURIComponent(BOM_FIXTURE_URL)}`;

test.describe("dl flow panel exploded stratum view (fixture-backed)", () => {
  test("bomTable preset loads plain (non-exploded) rows by default", async ({ page }) => {
    await page.goto(PANEL_PATH);
    await page.selectOption("#presetSel", "bomTable");
    // all 5 fixture functions render as plain node rows, no bands yet.
    await expect(page.locator(".lrow-node")).toHaveCount(5);
    await expect(page.locator(".lrow-band")).toHaveCount(0);
  });

  test("toggling exploded regroups rows tier-major with a band per stratum", async ({ page }) => {
    await page.goto(PANEL_PATH);
    await page.selectOption("#presetSel", "bomTable");
    await expect(page.locator(".lrow-node")).toHaveCount(5);

    await page.locator("#protoRow .chip", { hasText: "exploded" }).click();

    // three strata (tier 0/1/2 in the fixture): one band row each.
    const bands = page.locator(".lrow-band");
    await expect(bands).toHaveCount(3);
    await expect(bands.nth(0)).toContainText("stratum 0");
    await expect(bands.nth(1)).toContainText("stratum 1");
    await expect(bands.nth(2)).toContainText("stratum 2");
    // tier 0 carries the welded 2-file cycle alongside foundation.rs -- the
    // band header notes the weld count.
    await expect(bands.nth(0)).toContainText("welded");

    // tier-major: foundation.rs (tier 0) precedes middle.rs (tier 1) which
    // precedes top.rs (tier 2), regardless of alphabetical file order.
    const fRow = page.locator(".lrow-node .llabel", { hasText: /^f$/ });
    const gRow = page.locator(".lrow-node .llabel", { hasText: /^g$/ });
    const hRow = page.locator(".lrow-node .llabel", { hasText: /^h$/ });
    await expect(fRow).toBeVisible();
    await expect(gRow).toBeVisible();
    await expect(hRow).toBeVisible();
    const fTop = await fRow.evaluate((el) => el.closest(".lrow")!.getBoundingClientRect().top);
    const gTop = await gRow.evaluate((el) => el.closest(".lrow")!.getBoundingClientRect().top);
    const hTop = await hRow.evaluate((el) => el.closest(".lrow")!.getBoundingClientRect().top);
    expect(fTop).toBeLessThan(gTop);
    expect(gTop).toBeLessThan(hTop);
  });

  test("a 2-cycle welds into one collapsible card, members nested under it", async ({ page }) => {
    await page.goto(PANEL_PATH);
    await page.selectOption("#presetSel", "bomTable");
    await page.locator("#protoRow .chip", { hasText: "exploded" }).click();

    const weldCard = page.locator(".lrow-node .llabel", { hasText: "welded cycle" });
    await expect(weldCard).toBeVisible();
    await expect(weldCard).toContainText("2 files");

    // members (functions p, q from cycle_a.rs/cycle_b.rs) are visible,
    // nested under the card -- expanded by default.
    const pRow = page.locator(".lrow-node .llabel", { hasText: /^p$/ });
    const qRow = page.locator(".lrow-node .llabel", { hasText: /^q$/ });
    await expect(pRow).toBeVisible();
    await expect(qRow).toBeVisible();

    // collapsing the weld card (its own .ltri) folds the members away --
    // the same generic collapse machinery every other hasChildren row uses.
    const weldRow = page.locator(".lrow-node", { hasText: "welded cycle" });
    await weldRow.locator(".ltri").click();
    await expect(pRow).toHaveCount(0);
    await expect(qRow).toHaveCount(0);

    // re-expanding brings them back.
    await weldRow.locator(".ltri").click();
    await expect(pRow).toBeVisible();
    await expect(qRow).toBeVisible();
  });

  test("switching back off exploded restores the plain per-file listing", async ({ page }) => {
    await page.goto(PANEL_PATH);
    await page.selectOption("#presetSel", "bomTable");
    const explodedChip = page.locator("#protoRow .chip", { hasText: "exploded" });
    await explodedChip.click();
    await expect(page.locator(".lrow-band")).toHaveCount(3);

    await explodedChip.click();
    await expect(page.locator(".lrow-band")).toHaveCount(0);
    await expect(page.locator(".lrow-node")).toHaveCount(5);
  });
});
