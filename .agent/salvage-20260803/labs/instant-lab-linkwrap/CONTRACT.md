# CONTRACT: wrapped-line link detection in the terminal

Defect (user report + screenshot): when a file path hard-wraps across xterm
rows, the hover underline / ⌘-click link only covers the fragment on one row.
Cause: `provideLinks(y, cb)` at `src/terminal.ts:495-516` reads exactly one
buffer row (`term.buffer.active.getLine(y - 1)`) and scans that row's text in
isolation. A path split as `.../instant-lab-dock/` + `e2e/dock-strip...png`
never matches whole, and each fragment either fails `looksOpenable` or
resolves to a wrong path. The hover card (`wordAt`, `src/terminal.ts:316`) and
the ⌘-click miss path (`:528`, `:607`) share the per-row limitation.

## 1. The fix shape (binding)

- New file `src/termWrapJoin.ts` + `src/termWrapJoin.test.ts`, PURE functions
  only (no xterm import), so vitest covers them without a terminal:
  - `joinWrappedRows(rows: WrapRow[]): JoinedLine` where
    `WrapRow = { text: string; isWrapped: boolean }` (row 0 = the first row of
    the logical line; rows 1..n are its `isWrapped` continuations) and
    `JoinedLine = { text: string; rowStartOffsets: number[] }`
    (`rowStartOffsets[i]` = offset of row i's first character in the joined
    text). Join by plain concatenation, no separator: xterm hard wrap inserts
    nothing between rows.
  - `mapSpanToRowRanges(span: {start; end}, joined: JoinedLine): RowRange[]`
    where `RowRange = { rowIndex: number; startCol: number; endCol: number }`
    (0-based half-open cols within that row). A span crossing a row boundary
    yields one RowRange per row it touches.
  - Row text lengths come from the actual per-row strings, NEVER from
    `cols`-arithmetic: wide glyphs make cell-count != string-length.
- `src/terminal.ts` link provider:
  - From the requested row, walk BACK while the current row `isWrapped` to find
    the logical-line start, then FORWARD while the next row `isWrapped`,
    collecting row texts. Per-row text via `translateToString(true)` for the
    LAST row and `translateToString(false)` (untrimmed) for rows that have a
    continuation, so mid-line spaces survive and offsets stay true; record the
    strings you actually joined so offsets always match.
  - Scan the JOINED text with the existing `scanLineTokens` +
    `looksOpenable` (unchanged, they are pure over strings).
  - Emit `ILink`s with multi-row ranges via `mapSpanToRowRanges` (xterm ranges:
    1-based, end-inclusive; a link may span rows with `start.y != end.y`).
    Dispatch still gets the whole joined token text.
  - Guard: cap the walk at 40 rows each direction; a longer logical line
    returns the single-row behavior (record the cap in a comment as the
    constraint it is).
- `wordAt` (`:316`) joins the same way (share the walk via a small helper in
  terminal.ts that returns `WrapRow[]` + the clicked row's index) so the hover
  card and ⌘-click token equal the underlined token. The three call sites must
  keep returning the SAME text for the same target (the one-scanner law in
  termTokens.ts's header).
- `src/termTokens.ts`: DO NOT MODIFY.

## 2. Proofs

1. `termWrapJoin.test.ts` (vitest): join offsets with 2 and 3 rows; span fully
   inside one row -> one RowRange; span crossing a boundary -> two RowRanges
   with correct cols; wide-glyph row (e.g. a row text containing `日本語`)
   keeps offsets by string length; cap behavior at 40.
2. A provider-level test where feasible without a live terminal: factor the
   provider's "rows -> ILink ranges" body so a unit test drives it with fake
   rows; the xterm-facing shell stays thin.
3. e2e: extend or clone `e2e/term-cmd-hover.spec.ts` (read its mechanics
   first) with a fixture whose path is forced to wrap (narrow terminal), assert
   the underline/hover covers the full path across both rows and activation
   dispatches the WHOLE path. Mint any new snapshot with `--update-snapshots`,
   then a clean verify run. If the harness cannot force a deterministic wrap,
   STOP that item and record why in REPORT.md rather than shipping a flaky
   spec.

## 3. Gates (run all, record in REPORT.md)

| gate | command |
| --- | --- |
| install | `corepack pnpm@10.12.4 install --prefer-offline` |
| tsc | `corepack pnpm@10.12.4 exec tsc --noEmit` (must be clean at this base) |
| vitest | `corepack pnpm@10.12.4 exec vitest run src/` (new + existing) |
| e2e hover | `corepack pnpm@10.12.4 exec playwright test e2e/term-cmd-hover.spec.ts` |
| e2e new | your wrapped-path spec, mint then verify |

## 4. Laws

- No commits. Nothing outside this worktree. Never `just dev`.
- Deviations: STOP the item, record in REPORT.md, never improvise around. A
  permission denial ends that approach.
- Comments only for constraints code cannot show. No em dashes. Never the words
  provenance, substrate, load-bearing, regime. Descriptive names, never single
  letters.
- Files you own: `src/terminal.ts`, `src/termWrapJoin.ts(+test)`, your e2e
  files, `REPORT.md`. Nothing else.
- Deliverables: REPORT.md (gates table, receipts, deviations) + tests above.
