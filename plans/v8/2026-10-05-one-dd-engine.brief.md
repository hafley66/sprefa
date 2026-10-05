# Brief: one dd engine across phases (stage 1)

User (2026-10-05): sqlite_ivm paused; dd is the default engine (`DL8_ENGINE` unset = `ir:dd`,
main `105532294`). Goal: prove the IVM-all-the-way design moves. Constraint from the user:
do not over-couple to dd and do not break out of the shared IR layer: all work goes through
`ivm_ir` + the `ivm_engine::Engine` trait; no dd-specific code in sprefa; sqlite must still build
and pass behind the trait even though it is not being optimized.

## Today (hello world, `dl8 compile`, from this session's trace)

1. read; 2. install engine 1 = macro library compiled as a full program (1,057 nodes) + settle its
seeds; 3. install engine 2 = macrotime expander (1,069 nodes) + settle base; 4. enter the unit
(48 rows); 5. Rust lower + check (batch); 6. install engine 3 = comptime (2,137 nodes, prelude +
user rules) + settle (75% of the work). User file: 48 rows; everything else is dl8's own stdlib,
re-installed and re-settled per process.

## Do

1. Design section first in `plans/v8/2026-10-05-one-dd-engine.findings.md`: type signatures,
   instance timelines, storage and the read/write sequence per edit (user planning order) for the
   target: one installed program per process = macro library + macrotime + comptime stdlib rules;
   a source file enters as signed rows; user rules ride as rows through the merged rule
   interpreter (`DL8_RULES=interpret`) or the compiled path where they fall back; lower and check
   as delta steps (say which parts can be rules, which stay Rust over deltas, and what that
   Rust must keep as state). Name each place the shared IR or Engine trait needs a new
   capability, engine-neutral.
2. Stage 1 implementation (behind a switch, default off, e.g. `DL8_ONE_ENGINE=1`):
   - engines 1 and 2 become one install (macro library compile and macrotime expansion in one
     program), and
   - the stdlib comptime rules install once and settle once; a program's own rules and rows enter
     after that as rows (interpreter) or as the compiled fallback, without reinstalling the stdlib.
3. Proof by work counts only (no wall clock, no caching): hello world and the registry build,
   switch off vs on: installs, dd operators built, arrangements, arranged records, settles, rows
   entered. Then two programs compiled in one process (a test), second program's counts: the
   stdlib must not be re-installed or re-settled.
4. Parity: suite (dd default) and `DL8_ENGINE=ir:sqlite` suite, both with the switch off and on;
   registry `cmp`; macrotime oracle.

## Laws

- Own worktree from sprefa main `105532294`; sqlite_ivm main `9d928c2` via the existing symlink
  unless the trait/IR needs a change (then clone inside the worktree, branch `feat/one-dd-engine`).
- Only this lane runs. `-j 4`; `--test-threads=1`. Features in Rust/dl7 only. No caching. No
  wall-clock timing. No tree-wide formatting.
- No recursion in kernel Rust, no silent fallbacks, no `json_*(`, no book edits, never
  `git commit -n`, no merge, no push.
- Commits end with
  `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_019jL2FmxduyQQ9kpmQYBKj7`.

## Receipt

```
status: done | blocked
sprefa branch + sha:
sqlite_ivm branch + sha (or "unchanged"):
new trait/IR capabilities: <list, engine-neutral>
counts switch off -> on: hello world, registry, second program in one process
parity: <result lines>
next: <one action>
```
