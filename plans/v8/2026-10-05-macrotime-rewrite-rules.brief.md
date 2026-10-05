# Brief: macrotime graph rewrite as dl7 rules, waves inside the engine (page 3 A + B)

User (2026-10-05): "combo 3 into 4". Option A (rewrite rules in the engine) and option B (the
wave loop itself in the engine, Dedalus-style `next`) in one lane. Land A as its own commit
first (suites green), then B on top.

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
3. Commit A: Rust keeps the `for wave` loop, the `seen` hash cycle check, and forwards the
   engine-derived diff as signed changes. Delete `rewrite_active_graph`, `Pass`, `apply_rewrite`.
4. B: the wave becomes time in the engine. `graph(R)@next :- graph(R), !removed(R).`
   `graph(R)@next :- added(R).` Lower to the IR's `LetRec` (Materialize-style iterate, body may
   retract) if both engines evaluate a non-monotone LetRec body correctly; check ivm-dd
   `iterate` and ivm-sqlite's DRed/SCC path for that first and report which one cannot. The
   `next` surface syntax in dl7 is a kernel/language change: write the proposed syntax and 3
   dl7 examples into the findings before implementing it. Termination: no claims at W.
   `expansion_cycle` and `expansion_round_limit` keep their diagnostics (wave bound through
   `LetRec.limit` or a counter; repeated-state detection may stay in Rust, report the choice).
5. Measure after A and after B; add "How it works".

## Laws

- Byte-identical: macrotime oracle (`every_macrotime_case_matches_v7`), registry `cmp`, both
  suites, diagnostics included. No golden edits.
- Authorized kernel feature: `next` (wave time) only. Any other new kernel relation or feature:
  stop and report. No recursion in kernel Rust.
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
