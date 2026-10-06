# Brief: shrink the lowering (census changes 1-9)

User (2026-10-06): "do it". Source: `plans/v8/2026-10-05-lowering-census.findings.md` (ranked
changes 1-9; 3,422 of 4,263 hello-world IR nodes never carry a row).

## Do

Implement the census's ranked changes, one commit each, in this order:
1. hash-cons equal (op, canonical inputs) nodes;
2. ground terms as IR literals (term table resolved at install; late terms keep the term-map path);
3. drop the all-sources sentinel (sqlite adapter takes source columns from `Program.rels`;
   compose stops assuming output 0 reads every source);
4. project fused into Join in the IR (project-only Mfp, single consumer);
5. no `Get(__ir_unit)` when the first goal is a positive read;
6. no seed source for keys that never get rows (condition as in the census);
7. semantic decode as one join on `__ir_semantic`;
8. prewarm through `Engine::intern_terms`;
9. threshold only where duplicates can arise.
Each commit records: IR nodes and dd operators for the 3 hello-world installs and the registry
comptime install (before -> after), and keeps parity. If a change cannot be made safe, skip it
with the reason in the findings.

Parity after each commit: dd suite (default); after the last commit also `DL8_ENGINE=ir:sqlite`
suite, `DL8_ONE_ENGINE=1` dd suite, registry `cmp`, macrotime oracle, census of 132 programs
stdout+exit equal to main. Work counts only, no wall clock.

Findings: `plans/v8/2026-10-06-lowering-shrink.findings.md` with a per-change table and "How it
works". Update work-count pins in the commit that changes them.

## Laws

- Own worktree from sprefa main `08e53559d`; sqlite_ivm via symlink to main `80d416d` unless an
  engine change is needed (then clone on branch `perf/lowering-shrink`, extension dylib rebuilt).
- Shared `ivm_ir` / Engine trait stays engine-neutral; ivm-dd and ivm-sqlite both keep passing.
- Only lane running; `-j 4`; `--test-threads=1`. Features in Rust/dl7. No caching. No tree-wide
  formatting.
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
per change: <n: nodes #1/#2/#3/registry before -> after, dd ops, commit | skipped: reason>
parity: <result lines>
next: <one action>
```
