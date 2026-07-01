#!/usr/bin/env node
// Headless proof: load the running app, screenshot each frame while stepping
// with the same ArrowRight key a viewer uses. Writes shots/frame-NN.png.
import { chromium } from 'playwright'
import { mkdirSync } from 'node:fs'

const target = process.env.URL || 'http://localhost:5173/'
const N = Number(process.env.N || 8)
mkdirSync(new URL('../shots/', import.meta.url), { recursive: true })

const browser = await chromium.launch()
const page = await browser.newPage({ viewport: { width: 1280, height: 720 } })
await page.goto(target, { waitUntil: 'networkidle' })
await page.waitForSelector('.shiki-magic-move-container, pre', { timeout: 10000 })

for (let i = 0; i < N; i++) {
  await page.waitForTimeout(1600) // let the token tween settle
  const path = new URL(`../shots/frame-${String(i).padStart(2, '0')}.png`, import.meta.url).pathname
  await page.screenshot({ path })
  console.log('shot', path)
  await page.keyboard.press('ArrowRight')
}

await browser.close()
console.log('done')
