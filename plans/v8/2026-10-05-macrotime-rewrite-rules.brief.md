# Brief: macrotime graph rewrite as dl7 rules (page 3, option A)

User (2026-10-05): option A (Rust keeps the wave loop; the rewrite moves into the engine),
written so option B (waves as time inside the engine) only replaces the loop later.

## Today

- `src/_1_macrotime/_4_expand.rs` ~`:383-480`: per wave the engine settles, Rust reads claims/outputs
  (`macro_results`), `rewrite_active_graph` (`src/_1_macrotime/_3_rewrite.rs`, 273 lines, port
  of v7 `0b_syntax_rewriter.pl`) computes removed/added rows, Rust hashes the graph for
  `expansion_cycle`, and sends signed changes back.
- `Pass::node` (`_3_rewrite.rs:111`) is recursive kernel Rust.

## Do

1. Measure first: per wave, ms in `macro_results`, `rewrite_active_graph`, `graph.apply`, and
   settle, both engines, registry build. Findings file
   `plans/v8/2026-10-05-macrotime-rewrite-rules.findings.md`.
2. dl7 rules in `macrotime/` (or `prelude/`, follow where the protocol rules live) for one wave:
   - `replaced(N, Out, Ord)` from claims and outputs; diagnostics `conflicting_macro_claims`,
     `non_dense_expansion_ordinals`, `unknown_expansion_output` as rules.
   - Child overrides with flat ordinals: `slot(Owner, ChildOrd, Size)` (1 for an unclaimed child,
     output count for a claimed one), start = running sum of earlier slots, new edge ordinal =
     start + output ordinal. No path-list ordinals.
   - `reach(N)` from the new frontier over overridden edges, old items, and derived items
     (the `derived_item` / `available.identity` conditions in `apply_rewrite`).
   - Removed/added as stratified set difference; `expansion_claim` / `expansion_output`
     origin rows as heads.
   - Every rule takes the wave from a one-row relation `cur_wave(W)` and carries `W` in its
     head, so option B only replaces the Rust loop with `next(W+1)`.
3. Rust keeps the `for wave` loop, the `seen` hash cycle check, and forwards the engine-derived
   diff as signed changes. Delete `rewrite_active_graph`, `Pass`, `apply_rewrite`.
4. Measure after; add "How it works".

## Laws

- Byte-identical: macrotime oracle (`every_macrotime_case_matches_v7`), registry `cmp`, both
  suites, diagnostics included. No golden edits.
- New kernel relation or kernel feature needed: stop and report. No recursion in kernel Rust.
- Own worktree from current sprefa main; sqlite_ivm clone only if needed.
- `-j 4`; `--test-threads=1`; `uptime` before timing; all edits first, then test.
- No silent fallbacks, no `json_*(`, no book edits, never `git commit -n`, no merge, no push.
- Commits end with
  `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_019jL2FmxduyQQ9kpmQYBKj7`.

## Receipt

```
status: done | blocked
sprefa branch + sha:
sqlite_ivm branch + sha + clone path (or "unchanged"):
commits: <sha subject>
before/after: per-wave ms split, registry time/RSS both engines, suites
validation: <command + result line each>
next: <one action>
```
