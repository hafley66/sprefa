# Brief: non-monotone LetRec and `next` (wave time in the engine)

User (2026-10-05): page 3 A dropped (slower: registry sqlite 1.9 -> 3.0 s, dd 0.41 -> 0.59 s;
branch `perf/macrotime-rewrite-rules` `d3f5d9c72` stays unmerged as reference). The three
blockers for B "sound fixable": do them.

## Today (from the page 3 findings, worktree `~/projects/sprefa-wt/macrotime-rewrite-rules`,
`plans/v8/2026-10-05-macrotime-rewrite-rules.findings.md`)

- ivm-sqlite refuses Negate / Reduce / Threshold over a LetRec variable
  (`sqlite_ivm/crates/ivm-sqlite/src/3_nodes.rs:569-574`), Antijoin with a recursive right input
  (`:921-926`), and `LetRec.limit` (`:1284`).
- ivm-dd refuses `LetRec.limit` (`crates/ivm-dd/src/2_dd.rs:109`); no test runs a LetRec body
  that retracts rows.
- dl8 stratification rejects negation/aggregation inside one recursive component
  (`src/_3_check/` ... `_2_stratify.rs:139-170`).
- The findings file holds a proposed `next` syntax, 3 dl7 examples and a LetRec lowering plan.

## Do

1. Semantics first (findings file `plans/v8/2026-10-05-nonmonotone-letrec.findings.md`):
   `LetRec` = Materialize `WITH MUTUALLY RECURSIVE` semantics: iterate the bodies from empty,
   each round recomputes every variable from the previous round's values (body may negate,
   aggregate, retract), stop at a fixed point or after `limit` rounds (state the limit outcome:
   error vs return last). Cite Materialize docs / source for the exact rule. Monotone LetRecs
   keep today's semi-naive path; the non-monotone path is selected per LetRec from its body.
2. ivm-dd: `iterate` with a non-monotone body (DD supports retractions inside `iterate`);
   `limit` via the iteration coordinate. Tests: a body that retracts; limit hit; limit not hit.
3. ivm-sqlite: a non-monotone LetRec evaluation path (per round: compute the new value of each
   variable from the previous value, diff against previous, stop on empty diff or limit) for
   Negate/Reduce/Threshold/Antijoin over variables; incremental maintenance of that LetRec
   under outer deltas (state the strategy: recompute the LetRec from scratch when any input
   changes is acceptable as a first cut if measured and logged; DRed is not valid here).
   Same tests as dd, dd == sqlite.
4. dl8: `next` per the page 3 proposal. Negation/aggregation through `next` is temporally
   stratified (legal); inside one time step it stays rejected. Lowering: a component with `next`
   becomes a LetRec whose round = one time step, `limit` from the declaration or the wave limit.
   3 dl7 examples from the page 3 findings as fixtures, both engines.
5. Do not re-land page 3. Report what macrotime B would now need.

## Laws

- Own worktree from sprefa main; sqlite_ivm clone inside it from main `1074daa`, branch
  `feat/nonmonotone-letrec`; build the extension dylib in the clone before suites.
- One other lane may be running; `-j 4`; `--test-threads=1`.
- Byte-identical for every existing test and the registry `cmp`; monotone LetRecs must not slow
  down (registry build time both engines, before/after).
- New kernel feature authorized: `next` only. No recursion in kernel Rust, no silent fallbacks,
  no `json_*(`, no book edits, never `git commit -n`, no merge, no push.
- Commits end with
  `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_019jL2FmxduyQQ9kpmQYBKj7`.

## Receipt

```
status: done | blocked
sprefa branch + sha:
sqlite_ivm branch + sha + clone path:
commits: <sha subject>
semantics: <limit outcome, maintenance strategy per engine>
before/after: registry time both engines, suites
validation: <command + result line each>
next: <one action>
```
