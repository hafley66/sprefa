# Brief: macrotime waves inside the engine, behind an env switch

User (2026-10-05): run macrotime waves in the engine, with an env split point so both paths can
be tested side by side.

## Context

- Today: Rust drives the wave loop; `rewrite_active_graph` (`src/_1_macrotime/_3_rewrite.rs`)
  computes removed/added rows per wave; the engine settles them.
- Merged since the dropped rule attempt: `next` (wave relations lower to one limited LetRec,
  non-monotone bodies) and nested LetRecs (recursion inside one wave).
- Reference rules: `~/projects/sprefa-wt/macrotime-rewrite-rules/macrotime/3_rewrite.dl7`
  (branch `perf/macrotime-rewrite-rules`, unmerged; rules joined a current-wave relation and
  were slower: registry sqlite 1.9 -> 3.0 s, so read its findings for why).

## Do

1. `DL8_MACROTIME=rust|engine`, read once in the binary config, default `rust`. `rust` = today's
   path untouched. `engine` = the whole wave loop is one `next` LetRec in the macro program with
   walk / reach / slot_start nested; Rust enters a unit's rows and reads the settled result once.
   Cycle detection (`expansion_cycle`) and the round limit (`expansion_round_limit`) keep their
   diagnostics in `engine` mode (state how).
2. Lift the "intern readback in a wave rule" refusal if the rewrite needs it.
3. Parity: both suites x both engines x both modes; macrotime oracle (24 cases) both modes;
   registry `cmp`. Any difference between modes is a bug in `engine` mode.
4. Work counts (sqlite work counter; dd operators/arrangements/records), `rust` vs `engine`, for
   the registry build and the census. No wall clock. Findings
   `plans/v8/2026-10-05-macrotime-waves-engine.findings.md` with "How it works".

## Laws

- Own worktree from sprefa main `1a615c617`; sqlite_ivm clone from main `14675e8` (only if
  engine changes are needed), branch `feat/macrotime-waves-engine`; build the extension dylib
  before suites.
- Parallel lane `feat/rule-interpreter` touches `_2_stratify.rs`, `_9b_ir_eval.rs`,
  `bin/dl8.rs`, config; coordinate the env/config site by keeping your switch in its own field
  and report overlap.
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
sqlite_ivm branch + sha + clone path (or "unchanged"):
parity: <suite x engine x mode result lines>
counts rust vs engine: registry, census
next: <one action>
```
