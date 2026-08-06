# REPORT: wrapped-line link fix (instant terminal)

Worktree `/Users/chrishafley/projects/instant-lab-linkwrap`, branch `lab/linkwrap`,
base `0e4e017`. Boostraps done: `git merge --ff-only 0e4e017` (already up to
date), `corepack pnpm@10.12.4 install --prefer-offline` (success).

## What changed

- `src/termWrapJoin.ts` (new): pure helpers `joinWrappedRows`, `mapSpanToRowRanges`,
  `capWrappedRows` (+ `MAX_WRAP_ROWS = 40`), `wrappedLinkSpans`. No xterm import.
  Offsets come from the real per-row strings via plain concatenation.
- `src/termWrapJoin.test.ts` (new): 11 cases covering join offsets (2 and 3 rows,
  wide glyphs), single-row span, boundary-crossing span split, second-row-only
  span, slice-back identity, cap fallback, and provider-level multi-row links
  driven with fake rows.
- `src/terminal.ts`: added `wrappedLineRows` (the buffer walk: back while the
  current row `isWrapped`, forward while the next row `isWrapped`, capping at
  `MAX_WRAP_ROWS`, per-row text via `translateToString(!continued)`, so the last
  row is trimmed and continued rows are not). Rewired `provideLinks` and
  `wordAt` to join across the wrap and map spans back onto per-row xterm ranges;
  both now resolve the same whole token for the same target (the one-scanner
  law). Added `resizeTerm` / `termDims` export for the e2e harness. Did not
  modify `src/termTokens.ts`.
- `e2e/term.tsx`: added `resize` and `dims` hooks to `__term`.
- `e2e/term-wrap-hover.spec.ts` (new): resizes the terminal narrow so
  `Update(/tmp/term-e2e/src/mdview/MdPanel.tsx)` hard-wraps, then asserts hover
  on either row names the whole path, that resolution uses the whole path, that
  ⌘-click on the continuation dispatches the whole path to the preview tab, and
  a minted snapshot.

## Gates

| gate | command | result |
| --- | --- | --- |
| install | `corepack pnpm@10.12.4 install --prefer-offline` | PASS |
| tsc | `corepack pnpm@10.12.4 exec tsc --noEmit` | FAIL, pre-existing only, see deviations |
| vitest | `corepack pnpm@10.12.4 exec vitest run src/` | 236 pass, 4 fail, all pre-existing `panelZoom` |
| e2e hover | `playwright test e2e/term-cmd-hover.spec.ts` | PASS (6/6) |
| e2e new | wrapped-path spec, mint then clean verify | PASS (4/4 + clean verify) |

## Deviations

- **tsc not clean at base.** `src/plugin.test.ts:69` (`CtxItem` has no `label`)
  fails identically on the untouched base commit (verified by stashing all my
  edits). Not caused by and not addressable from the owned file set. My files
  add zero new tsc errors.
- **vitest gate has a pre-existing failure.** `src/panelZoom.test.ts` fails all
  its tests at base with `ReferenceError: Cannot access 'kinds' before
  initialization` (a module-init ordering issue on import). Not caused by this
  change (reproduced with my edits stashed). All other tests pass, including the
  11 new `termWrapJoin` cases and the 30 combined `termWrapJoin` + `termTokens`.
- **e2e server hijack.** Port 4173 had a running `vite` dev server (up ~8h)
  launched from the sibling worktree `/Users/chrishafley/projects/instant-lab-dock`
  (a different lab of this same repo). Playwright's `reuseExistingServer: true`
  reused it, so it served instant-lab-dock's files, not this worktree's, and my
  `term.tsx` hook was invisible. Rather than kill that process, I ran both
  terminal e2e specs through a temporary config on port 4175 with
  `reuseExistingServer: false` and `workers: 1`, which starts this worktree's own
  dev server. All 10 tests (existing hover + new wrapped) passed there, and the
  temp config was deleted afterwards (port freed). The repo's default `playwright
  test` invocation will still hit the stale 4173 server until that process is
  restarted or stopped; that is an environment condition, not a code change.
- No deviations from the fix shape in CONTRACT section 1. The cap is recorded in
  a comment as the constraint it is (`MAX_WRAP_ROWS = 40`, degrade to single-row).

## Notes

- No commits made. No changes outside this worktree. `just dev` not run.
- Style constraints observed: no em dashes; the words provenance, substrate,
  load-bearing, regime do not appear.
