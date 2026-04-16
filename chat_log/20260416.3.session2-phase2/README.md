# Session 2, Phase 2 — bodies

Fills in every `todo!()` / `unimplemented!()` stubbed during Phase 1. At
end of Phase 2 the six exprs of Golden Tests G1–G6 compile, run
end-to-end against a real `SqliteStore`, and G7–G10 pass under the
stress + LSP harness.

## Source of truth

- `chat_log/20260416.0.evaluator-store-mutation-design.md` — Zoom 3 pseudo
  (the body of every file). Slices below point at specific sections.
- `chat_log/20260416.2.system-zoom-1-plus-golden-tests.md` — acceptance set
  (G1–G10). Every slice that completes lands the capability for a
  specific subset.
- `v2/CLAUDE.md` — outer system diagram, five invariants, convention echo.

## Phase 1 exit state (recap)

Skeleton landed on `wip/kitchen-sink-react-hook-fix`. Pending commit.

- `store/` trait + `NoopStore` (every method `unimplemented!`)
- `mutations.rs` trait family + `AutoApprove` body + stub `InteractiveCli`
  (unit struct) + stub `LspPromptBridge` (`handle` = `todo!`)
- `_task_guard.rs` full
- `OpCtx` +store +mutations +cancel +expr_name +current_site
- `RuntimeConfig` +max_passes +max_claims_per_pass +max_cursors_per_root
- `RunEvent` rewrite; `CursorExpr`; `Pipeline: Clone`
- Test scaffold (`*::test_default`, `OpCtx::for_test`)

## Execution order

Run strictly 2a → 2f. Each slice ends with `cargo build --tests` green
before the next begins.

| Slice | Scope | Golden tests enabled |
|---|---|---|
| 2a | sqlite foundation (deps, migrations, UDFs, DDL builders) | infra |
| 2b | `SqliteStore` body (register/flush/query/effect/files_scanned) | G3, G8 |
| 2c | `_7_init_cursors.rs` + `_7_runner.rs` removal; topo; batch build | G1, G2, G3 |
| 2d | `InteractiveCli` + `LspPromptBridge` handler bodies | G6 (half), G10 (half) |
| 2e | `DocSession` rewire + `bin/sprefa_v2` collapse | G1–G6 end-to-end |
| 2f | stress tests, G7/G9 verify, commit | G7, G9, G10 full |

G4 + G5 xref land end-to-end once 2c exists; they are implicit in the
init_cursors topo-sort behavior carried over from Phase 1.

## Files created in Phase 2

```
v2/src/store/_2_sqlite.rs        SqliteStore impl
v2/src/store/_3_ddl.rs           build_*_ddl, schema_hash_of, extract_hash_of
v2/src/store/_4_udfs.rs          sprf_norm, fzy_score, re_extract, split_part
v2/src/store/_5_migrations.rs    init_db, run_migrations
v2/src/store/sql/*.sql           include_str!'d migration files (9)
v2/src/_7_init_cursors.rs        replaces _7_runner.rs
v2/tests/stress.rs               feature-gated, #[cfg(feature = "stress")]
```

## Files modified in Phase 2

```
v2/Cargo.toml                    + sqlx (workspace), + chrono, + blake3
v2/src/lib.rs                    pub mod _7_init_cursors; remove _7_runner;
v2/src/store/mod.rs              + pub mod _2_sqlite.._5_migrations; re-exports
v2/src/mutations.rs              bodies for InteractiveCli + LspPromptBridge
v2/src/analysis.rs               DocSession: new, on_source_change, ensure_run
v2/src/bin/sprefa_v2.rs          run_cmd uses init_cursors
```

## Files deleted in Phase 2

```
v2/src/_7_runner.rs              collapsed into _7_init_cursors.rs
```

## Deviation policy

The Z3 pseudo supersedes 8-session-roadmap text. Slices list specific
deviations only when the landing code differs from pseudo. Otherwise
clone the pseudo verbatim, adjust imports, run the test.

## Commit

One commit at end of Phase 2 with message from `2f_stress_verify_commit.md`.
Message covers all six slices.
