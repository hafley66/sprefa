# 2f — invariant tests, verify, commit

## Revision (2026-04-16)

Stress tests recast as **invariant tests** — small N that prove the guard works
per cycle. If N=3 reparses reap cleanly, N=1000 follows by induction on the
cancel discipline. No feature gate; ungated in the regular `tests/` dir.

Four tests in `v2/tests/invariants.rs`:

1. `reparse_3x_task_count_returns_to_baseline` — record
   `Handle::current().metrics().num_alive_tasks()` before; call
   `DocSession::on_source_change(src)` three times with a yield after each;
   assert count returns to baseline (proves cancel→drop→abort per cycle).
2. `dropped_stream_aborts_inner_tasks` — start `init_cursors(...)`, drop the
   returned `BoxStream` mid-drain, poll task count until it drops or timeout
   at ~100ms.
3. `mutation_await_errs_on_cancel` — `await_approval` on a token that's been
   cancelled returns `Err(Cancelled)`, no hang.
4. `mutation_await_errs_on_handler_drop` — drop the handler before it acks;
   `await_approval` returns `Err(Cancelled)`.

Delete `[features] stress = []` from Cargo.toml (unnecessary now).

---

# Original (superseded)


Land the three stress scenarios from Z3. Hand-verify G1–G10. One commit
covers the entire Phase 2.

## Prereqs

2a–2e all landed. `cargo test -p v2` green.

## Scope

```
v2/Cargo.toml                    [features] stress = []
v2/tests/stress.rs               NEW, feature-gated
v2/examples/                     golden-test .sprf programs (6 small files)
```

## Files

### v2/Cargo.toml

```
[features]
stress = []
```

Add `tokio-metrics = "0.3"` under `[dev-dependencies]` so
`stress.rs` can count active tasks.

### v2/tests/stress.rs

Clone Z3 section `### tests/stress.rs (feature-gated)` (lines
1538–1609). Three tests:

1. `stress_1000_reparse_cycles_bounded_rss` — golden test **G7**
2. `stress_init_cursors_dropped_mid_stream_aborts_tasks` — covers
   cancel discipline; tangential to G7
3. `stress_mutation_prompt_drop_and_cancel_both_return_err` — golden
   test **G9** (cancel path)

Gate: `#![cfg(feature = "stress")]` at top of file.

Run locally:
```
cd v2 && cargo test --release --features stress --test stress -- --nocapture
```

CI does not run these by default. Document in `v2/README.md`
"Tests" section.

### v2/examples/*.sprf — golden test programs

Ship as real files so operators can `cargo run -- examples/N.sprf`.

```
v2/examples/
  g1_hooks.sprf          rule(hooks) > repo($R) > rev(main) > fs(**/*.tsx) > ast-grep(use$H($$$ARGS));
  g2_todos.sprf          rule(todos) > ... > marker(TODO:, $MSG);
  g3_pkgs.sprf           rule(pkgs)  > ... > json({name:$N, version:$V, dependencies:{$DEP:$VER}});
  g4_xref_same_repo.sprf rule(def)>...; rule(use)>... ast-grep(${def.$NAME}($$$));
  g5_xref_cross_repo.sprf
  g6_rename.sprf         rule(rename)>... ast-grep(oldName){fix:newName} > write();
```

Each file is a 1-3 line .sprf + a top-of-file comment naming the
golden test and expected output shape.

## Verify matrix

Walk the 10 golden tests manually against a fixed set of fixture
repos (use `../ext/` or a throwaway `/tmp/sprefa-golden/` with three
cloned demo repos).

| Test | Command | Pass criterion |
|---|---|---|
| G1 | `cargo run -- examples/g1_hooks.sprf` | ≥1 row per useX call in tsx |
| G2 | `cargo run -- examples/g2_todos.sprf` | rows contain every `TODO:` marker in md files |
| G3 | `cargo run -- examples/g3_pkgs.sprf` | rows = Σ (package × dep) pairs |
| G4 | `cargo run -- examples/g4_xref_same_repo.sprf` | use rows only for $NAMEs bound by def |
| G5 | `cargo run -- examples/g5_xref_cross_repo.sprf` | scan-pointer columns populated |
| G6 | `cargo run -- examples/g6_rename.sprf` + AutoApprove | write effect fires; second run Skip |
| G7 | `cargo test --features stress --release --test stress stress_1000_reparse` | green |
| G8 | edit g3_pkgs.sprf to add `$V`, rerun | schema_hash drift → DROP+rebuild, new columns |
| G9 | rerun g6 unchanged; then edit a file; rerun | Skip → Stale → Emit visible in logs |
| G10 | open LSP, hover over `$NAME` and `write()` | capture value shown; effect preview shown |

Document any gap between expected and actual in a short "known issues"
section of the commit message. Do not silently skip.

## Commit

Single commit on `wip/kitchen-sink-react-hook-fix` covering P1 + P2,
since P1 was never committed. Message template:

```
feat(v2): evaluator + sqlite store + mutation plumbing (S1 P1+P2)

Phase 1 (skeleton):
- store/ trait + NoopStore
- mutations.rs trait family + AutoApprove; stubs for InteractiveCli + LspPromptBridge
- _task_guard.rs; RuntimeConfig knobs; OpCtx store/mutations/cancel/expr_name/current_site
- RunEvent rewrite; CursorExpr; Pipeline: Clone
- Test scaffold: Config/RuntimeConfig::test_default, OpCtx::for_test; 16 tests collapsed

Phase 2 (bodies):
- store/_2_sqlite + _3_ddl + _4_udfs + _5_migrations; SQL files rehomed from v1
- store/sql/mutations.sql (new; effect cache: kind_sigil, fingerprint, outcome, when)
- _7_init_cursors replaces _7_runner; topo sort; SqliteStore flush post-drain
- mutations.rs: InteractiveCli stdin loop, LspPromptBridge mpsc forward, spawn_handler loop
- DocSession rewired (store + mutations_handler on constructor; on_source_change respawns handler)
- bin/sprefa_v2 + bin/sprefa_v2_lsp rewired
- tests/stress.rs feature-gated (G7, G9 cancel path)
- examples/g*.sprf shipped for CLI smoke

Acceptance:
- cargo test green (except pre-existing doc_session_completion_fs_filter_glob_double_star)
- G1-G6 manually verified against fixture repos
- G7 green under `cargo test --features stress --release`
- G8/G9 verified by rerun + file-mutation scenario
- G10 verified in VS Code via LSP binary

Known deviations from design doc Zoom 3:
- LspPromptBridge uses mpsc not broadcast (RunEvent carries non-Clone oneshot::Sender)
- sqlx 0.8 conn.lock_handle for UDF registration (pseudo gestured at raw FFI)
- StoreErr::Sql carries String not sqlx::Error (Phase 1 dep-ordering choice; kept)
```

Author: human-first per `hafley:git-commit` skill — no Claude co-author line.

## Exit state

- Single commit landed, tree clean
- `wip/kitchen-sink-react-hook-fix` ready for merge review or rebase
- S1 complete; S2 (config + discovery) ready to start

## Post-commit memory updates

Land these in `~/.claude/projects/-Users-chrishafley-projects-sprefa/memory/`:

- **`project_v2_evaluator_shape.md`** (new) — init_cursors is the single
  entry point; RunEvent stream out; three consumers (CLI / LSP /
  stress); TaskGuard + cancel discipline enforced at DocSession
  boundary.
- **`project_v2_sqlite_store.md`** (new) — SqliteStore with
  classify_change DDL drift handling; mutations cache table schema;
  UDF registration via lock_handle.
- **`project_cursor_ref_plan.md`** — mark landed if not already.
