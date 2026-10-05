# Brief: `__ir_plain_inner` settles in one epoch

User (2026-10-05): "sure". Source: `plans/v8/2026-10-03-dd-speed.findings.md` (lines ~177-184)
and `plans/v8/2026-10-02-ir-speed.findings.md` (lines ~68-72).

## Today

- `src/_6_eval/_9a_ir_lower.rs` ~`:2606-2655`: LetRec over `__ir_plain_inner(cell, plain)` and
  `plain_lists`. The recursive step joins `plain_inner` with `get_constructor(9:2:[|])` on
  `plain_inner.cell = cons.tail` (column 2, not the id), decodes the head through
  `__ir_semantic`, and `Mint`s `cons(plain_head, plain_tail)`.
- A minted cons cell becomes a constructor row of `9:2:[|]` only after the epoch/pass ends.
  The ir-speed `Rel::decode` change made joins on a constructor's id see current rows; joins on
  argument columns do not.
- Cost: ir:dd, every comptime settle has a second epoch, 18-22 ms in 3 steps (about 1.3-1.5 us
  per timely operator, 14.5k operators). ir:sqlite: the comptime main settle's extra constructor
  passes (7 passes in ir-speed findings) are attributed to the same join.

## Do

1. Findings first, both engines: epochs/passes per comptime settle, rows produced in epoch 2
   (are they only identity pairs `plain_inner(c, c)` for freshly minted cells, or new terms?),
   who reads them. Write `plans/v8/2026-10-05-plain-inner-one-epoch.findings.md`.
2. Make the recursion see its own mints within the epoch. Preferred, in lowering only: inside the
   LetRec, read cons rows as `get_constructor(cons) ∪ (id, head, tail) projected from this
   LetRec's Mint`, so a cell minted in round r is a cons row in round r+1 of the same fixpoint.
   If the data shows epoch-2 rows are unread, the alternative is to keep minted plain cells out
   of the constructor read entirely. Engine changes only if lowering cannot express it; then
   both engines, same semantics.
3. Measure before/after: epochs and settle ms per comptime settle (dd), passes and statements
   (sqlite), registry build time/RSS both engines (local `RCARGO_OFF=1`, 3+ interleaved runs,
   `uptime` load), suites. Add "How it works".

## Laws

- Byte-identical: registry `cmp`, macrotime oracle, both suites. No golden edits.
- Own worktree; if sqlite_ivm changes, clone it inside the worktree from main `3f7eb15`, branch
  `perf/plain-inner-one-epoch`, rebuild the extension dylib from the clone.
- Another lane (`perf/cheap-unit-retraction`) runs at the same time on macrotime retraction and
  ivm-sqlite DRed; stay out of `src/_1_macrotime/` and DRed code.
- `-j 4`; `--test-threads=1`; `uptime` before timing; all edits first, then test.
- No recursion in kernel Rust, no new kernel relation (stop and report), no silent fallbacks, no
  `json_*(`, no book edits, never `git commit -n`, no merge, no push.
- Commits end with
  `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_019jL2FmxduyQQ9kpmQYBKj7`.

## Receipt

```
status: done | blocked
sprefa branch + sha:
sqlite_ivm branch + sha + clone path (or "unchanged"):
commits: <sha subject>
before/after: epochs/passes, settle ms, registry time/RSS both engines, suites
validation: <command + result line each>
next: <one action>
```
