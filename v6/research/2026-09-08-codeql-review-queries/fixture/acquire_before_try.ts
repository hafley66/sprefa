// Snapshot shape of 4_test.ts fileRoot fixture: two acquires precede the try whose finally releases them.
import { acquire, context$, page$ } from "./streams.js"
export async function fileRoot(use: (v: unknown) => Promise<void>) {
  const c = await acquire(context$())
  const p = await acquire(page$())
  try { await use({ c, p }) }
  finally { await p.release(); await c.release() }
}
export async function globalSetup(ctx: { provide: (k: string, v: unknown) => void }) {
  const h = await acquire(context$())
  ctx.provide("baseURL", h)
  return () => h.release()
}
