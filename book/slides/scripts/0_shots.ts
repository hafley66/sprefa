import { expect, test } from "@playwright/test"
import { mkdir } from "node:fs/promises"
import { resolve } from "node:path"
import { builtIdeas } from "../src/lib/0_glossary"

const outputDirectory = resolve("review-shots")

test("capture every built step", async ({ browser }) => {
  test.setTimeout(30_000 + builtIdeas.reduce((total, { idea }) => total + idea.steps!.length, 0) * 4_000)
  await mkdir(outputDirectory, { recursive: true })

  for (const reducedMotion of ["no-preference", "reduce"] as const) {
    const context = await browser.newContext({
      viewport: { width: 1440, height: 900 },
      reducedMotion,
    })
    const page = await context.newPage()

    for (const { group, idea } of builtIdeas) {
      const stepCount = idea.steps!.length
      for (let step = 1; step <= stepCount; step += 1) {
        await page.goto(`/${group.slug}/${idea.slug}/${step}`)
        await expect(page.getByText(`step ${step} of ${stepCount}`)).toBeVisible()
        await page.waitForTimeout(800)
        await expect(page.getByText(/!focus|!mark/)).toHaveCount(0)
        const rowTextHeights = await page.locator('svg [data-row-visible="true"] [data-row-text]').evaluateAll((nodes) => nodes.map((node) => node.getBoundingClientRect().height))
        if (rowTextHeights.length > 0) {
          expect(Math.min(...rowTextHeights)).toBeGreaterThanOrEqual(16)
        }
        await page.screenshot({
          path: resolve(outputDirectory, `${idea.slug}-${step}-${reducedMotion}.png`),
          fullPage: true,
        })
      }
    }

    await context.close()
  }
})
