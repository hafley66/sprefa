// Snapshot 13:09 shape of 8_around.ts: nested try/finally, cleanup duplicated in both finally clauses.
import { acquire, context$, page$ } from "./streams.js"
export async function around(runTest: () => Promise<void>, log: { $: { subscribe(): { unsubscribe(): void } } }) {
  let ctxH: { release(): Promise<void> } | undefined
  let pageH: { release(): Promise<void> } | undefined
  const connection = log.$.subscribe()
  try {
    ctxH = await acquire(context$())
    pageH = await acquire(page$())
    try {
      await runTest()
    } finally {
      connection.unsubscribe()
      try {
        assertLog()
      } finally {
        await pageH?.release()
        await ctxH?.release()
      }
    }
  } finally {
    connection.unsubscribe()
    await pageH?.release()
    await ctxH?.release()
  }
}
function assertLog() {}
