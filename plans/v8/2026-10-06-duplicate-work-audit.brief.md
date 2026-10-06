# Brief: duplicate-work audit (read-only), then a ranked fix list

User (2026-10-06): after hash-consing removed 353 of 1,057 nodes in one install, "anywhere else
obviously missing this". Main `a5f67af2f` (lowering shrink merged), dd default.

## Suspects (verify each with counts; add any others found)

1. Install #1 (macro library compile, 351 nodes) and #2 (macrotime expander, 379 nodes): the same
   macro rules lowered and installed twice per process.
2. `ivm_ir::compose` (`sqlite_ivm/crates/ivm-ir/src/4_compose.rs:56`) renames every non-constructor
   relation per part, so identical subgraphs across composed parts are never shared under
   `DL8_ONE_ENGINE=1`.
3. Demand (magic-set) copies: rules lowered once per binding pattern (census: HistoryV1 x3,
   Intersect x4); count identical bodies across copies.
4. Comptime rounds: each round re-lowers the whole program (`IrSession::evaluate` ->
   `ir_lower::lower`) and Rust scans the full closure (`colon_rows`, `intern_rows`,
   `assemble_generated_program`, snapshot/decode); count repeated work per round.
5. ivm-dd arrangements: the `(collection, key)` memo is per scope (`ivm-dd/src/2_dd.rs:142`,
   `:1099`); count collections arranged more than once on the same key across scopes/LetRecs.
6. Anything else: identical operators across installs in one process, repeated term interning,
   repeated decode of the same terms, repeated snapshots of unchanged relations.

## Do

- Structural fingerprints (op + canonical inputs, relation names normalized across parts) for
  every IR subgraph across all installs, composed parts and comptime rounds; dd arrangement
  census per (collection fingerprint, key). Runs: hello world, registry build, 132-program census,
  `DL8_ONE_ENGINE=0` and `=1`.
- Findings `plans/v8/2026-10-06-duplicate-work-audit.findings.md`: table location | duplicate
  count (nodes / operators / arrangements / rows / Rust scans) | evidence path:line | fix |
  count removed | safety condition. Ranked by count removed. No code changes.

## Laws

- Read-only on main; temporary instrumentation in a scratch worktree, removed after; commit only
  the findings file on branch `audit/duplicate-work`.
- Only lane running; `-j 4`. Counts only, no wall clock. No caching.
- Commits end with
  `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_019jL2FmxduyQQ9kpmQYBKj7`.

## Receipt

```
status: done | blocked
branch + sha:
duplicates: <location: count, unit>
ranked fixes: <fix: count removed, safety condition>
```
