// followLens.ts — pure helpers for the "follow the user" navigation surface
// (Track B B4, plans/2026-07-10-vscode-ext-review.md). Deliberately carries
// NO `import * as vscode` (unlike extension.ts), so vitest can exercise it
// directly without an extension host: see tests/unit/follow-lens.test.ts.
//
// `isStaleSeq` backs the `dl/locate` debounce in extension.ts's
// `openFlowPanel`: each debounced cursor move gets a monotonically
// increasing sequence number before the request goes out; when the response
// comes back, a mismatch means a NEWER cursor move already fired a request
// since this one was sent, so the stale response is dropped instead of
// posted to the panel (never overwriting a fresher center with an older
// one). This is the only fact-write-free guard needed here — dl/locate is a
// pure read, so dropping a stale response costs nothing beyond the one
// wasted round trip.

/** True when `sentSeq` no longer matches `currentSeq` — a later request was
 *  dispatched after this one, so its response should be dropped rather than
 *  acted on. */
export function isStaleSeq(sentSeq: number, currentSeq: number): boolean {
  return sentSeq !== currentSeq;
}
