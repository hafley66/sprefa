# Brief: recursion inside one wave (nested LetRec)

Follows `plans/v8/2026-10-05-nonmonotone-letrec.findings.md` (merged: `next` lowers wave
relations to one limited LetRec). Its "next": macrotime waves in the engine need walk, reach and
slot_start, which recurse inside one wave, i.e. a fixpoint nested inside each wave round. Both
engines refuse a LetRec nested in a LetRec today.

## Do

1. Findings first (`plans/v8/2026-10-05-nested-letrec.findings.md`): semantics of a LetRec inside
   a LetRec body (inner runs to its fixpoint per outer round; inner limit vs outer limit);
   Materialize / DD prior art (DD nested `iterate` scopes with product timestamps); where each
   engine refuses today (path:line).
2. ivm-dd: nested scopes. ivm-sqlite: inner LetRec evaluated per outer round on the recompute
   path, monotone inner bodies keep semi-naive. Tests on both engines, dd == sqlite: transitive
   closure inside a wave over a relation the wave rewrites; inner limit hit; outer limit hit.
3. dl8: recursion inside one wave is accepted when its component sits inside a `next` component;
   lowering emits the nested LetRec. Fixtures: the page 3 walk/reach/slot examples
   (`~/projects/sprefa-wt/macrotime-rewrite-rules/macrotime/3_rewrite.dl7` for shapes; do not
   re-land page 3).
4. Work counts (sqlite work counter, dd operators/records) for the new fixtures; registry and
   census counts must not change for programs without nested recursion.

## Laws

- Own worktree from sprefa main (current head); sqlite_ivm clone from main `aa72853`, branch
  `feat/nested-letrec`; build the extension dylib in the clone before suites.
- Parallel lane `feat/rule-interpreter` may touch lowering and IR; report overlap.
- `-j 4`; `--test-threads=1`. No caching. No wall-clock timing.
- No recursion in kernel Rust, no silent fallbacks, no `json_*(`, no book edits, never
  `git commit -n`, no merge, no push.
- Commits end with
  `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_019jL2FmxduyQQ9kpmQYBKj7`.

## Receipt

```
status: done | blocked
sprefa branch + sha:
sqlite_ivm branch + sha + clone path:
semantics:
counts: new fixtures both engines; registry/census unchanged (yes/no)
validation: <command + result line each>
next: <one action>
```
