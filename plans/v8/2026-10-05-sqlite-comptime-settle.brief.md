# Brief: ir:sqlite comptime settle cost (atlas lever 1)

User (2026-10-05): close the ir:sqlite vs ir:dd gap, one lever at a time.
Atlas: `plans/v8/2026-10-05-phase-atlas.md`.

## Today

- 104 programs, `dl8 compile`: comptime phase sqlite 49.2 s vs dd 8.2 s; sqlite installs 9.5 s of
  it, so 39.6 s is settles, snapshot, decode and Rust round work. Same settle counts on both
  engines (349 settles), so sqlite pays more per settle.
- `plain_inner` receipt (`plans/v8/2026-10-05-plain-inner-one-epoch.findings.md`): the sqlite
  closure loop runs another constructor pass after any growth of `ivm_term`, even when no Get
  reads the fresh term's functor (ivm-dd already gates this). Comptime main settle: 2 passes,
  could be 1.

## Do

1. Findings first: for 3 representative programs (the registry build, one `_5_store_host`
   fixture, one `_1_compile_oracles` case), split sqlite comptime time into: lowering, install,
   statement prepare, settle passes (statements by node kind, rows), snapshot + decode, Rust round
   work (`colon_rows`, `intern_rows`, `assemble_generated_program`). Same split on dd. Table into
   `plans/v8/2026-10-05-sqlite-comptime-settle.findings.md` before code.
2. Fix the largest measured item. The closure-pass gate is the first candidate; go further down
   the table while each change is byte-identical and measured.
3. Measure before/after: the 104-program census (script below), suite wall both engines,
   registry build time/RSS both engines (3+ interleaved runs, `uptime` load). "How it works".

Census: for every `.dl7` under `fixtures/ examples/ book/src/probes/ std/`, run
`DL8_ENGINE=<e> RUST_LOG=dl8::phase=info,dl8::ir=info dl8 compile <f>` and sum the
`dl8::phase` `ms` per phase name and `ir install install_ns`; skip non-zero exits.

## Laws

- Byte-identical: registry `cmp`, macrotime oracle, both suites. No golden edits.
- Own worktree from sprefa main; sqlite_ivm clone inside it from main `1074daa`, branch
  `perf/sqlite-comptime-settle`; build the extension dylib in the clone
  (`cargo build --release --no-default-features --features extension --target-dir target/extension`)
  before suites.
- A parallel lane (`perf/macrotime-rewrite-rules`) edits `src/_1_macrotime/`, macrotime dl7, and
  possibly LetRec handling in both engines; stay out of macrotime; if you touch LetRec code,
  say so in the receipt.
- `-j 4`; `--test-threads=1`; `uptime` before timing; all edits first, then test.
- No recursion in kernel Rust, no new kernel relation, no silent fallbacks, no `json_*(`, no book
  edits, never `git commit -n`, no merge, no push.
- Commits end with
  `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_019jL2FmxduyQQ9kpmQYBKj7`.

## Receipt

```
status: done | blocked
sprefa branch + sha:
sqlite_ivm branch + sha + clone path:
commits: <sha subject>
before/after: census per phase both engines, suite wall both engines, registry time/RSS
validation: <command + result line each>
next: <one action>
```
