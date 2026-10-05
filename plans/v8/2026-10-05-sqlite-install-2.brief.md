# Brief: sqlite install round 2 — default image cache, then SQL generation

Follows `plans/v8/2026-10-05-sqlite-install-lab.findings.md` (merged: sqlite_ivm `4710596`,
sprefa `f8bfd3b15`). Main: suites 173/173, sqlite 204 s, dd 131 s.

## Do

1. dl8 uses the installed-image cache by default: a per-user cache dir (XDG / macOS cache
   convention; pick a crate such as `dirs` or `directories` already in Cargo.lock if present),
   overridable and disable-able by env/flag. Tests use a fresh temp dir per run (no cross-test
   state leaks, results byte-identical cold vs warm). Cache key must include the sqlite_ivm
   build identity so a rebuilt engine never loads a stale image. Hits/misses logged. Size bound or
   eviction policy stated.
2. Rust SQL generation (74% of an image-hit install, ledger row 4: ~6 s census): measure where
   text is built; options: key the image cache by IR structural fingerprint so a hit skips SQL
   generation entirely (plan statements cached alongside the image), cache per-node SQL templates,
   or write with `fmt::Write` into one buffer. Pick by data.
3. Before/after: install bench (`crates/ivm-sqlite/tests/12_install_bench.rs` from the lab),
   104-program census (cold dir and warm dir), suite wall both engines, registry build time/RSS
   (3+ interleaved runs, `uptime`). "How it works".

## Laws

- Byte-identical: registry `cmp`, macrotime oracle, both suites, cold vs warm cache.
- Own worktree from sprefa main `f8bfd3b15`; sqlite_ivm clone inside it from main `4710596`,
  branch `perf/sqlite-install-2`; build the extension dylib in the clone before suites.
- Parallel lane `perf/sqlite-ledger-5-10` owns settle-side code (DRed, fills, interning,
  stage/host writes); stay in install/catalog/SQL-generation code; report overlap.
- `-j 4`; `--test-threads=1`.
- No silent fallbacks, no `json_*(`, no book edits, never `git commit -n`, no merge, no push.
- Commits end with
  `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_019jL2FmxduyQQ9kpmQYBKj7`.

## Receipt

```
status: done | blocked
sprefa branch + sha:
sqlite_ivm branch + sha + clone path:
commits: <sha subject>
before/after: install bench, census cold/warm, suites, registry time/RSS
validation: <command + result line each>
next: <one action>
```
