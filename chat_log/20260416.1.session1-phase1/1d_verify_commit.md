# 1d — Verify + commit

## Prereqs
1a + 1b + 1c complete; tree compiles.

## Steps

1. `cd v2 && cargo build --tests 2>&1 | tail -40` — zero errors.
2. `cd v2 && cargo test --lib -p v2 2>&1 | tail -20` — existing tests
   still pass (new stubs aren't exercised; NoopStore is reachable only
   through construction).
3. `cd v2 && cargo clippy --lib 2>&1 | grep -E "warning:|error:" | head -30`
   — review. Unused imports in `mutations.rs` (broadcast / oneshot
   pending Phase 2 use) are acceptable.
4. `git status` + `git diff --stat` — scope check. Expected surface:
   - New: `v2/src/_task_guard.rs`, `v2/src/mutations.rs`, `v2/src/store/` (3 files)
   - Modified: `v2/Cargo.toml`, `v2/Cargo.lock`, `v2/src/lib.rs`,
     `v2/src/_0_types.rs`, `v2/src/_2_config.rs`, `v2/src/_5_op.rs`,
     `v2/src/_14_scan_loop.rs`, `v2/src/analysis.rs`,
     `v2/src/bin/sprefa_v2.rs`, `v2/src/ops/_0_rule.rs`,
     `v2/src/ops/_3_fs.rs`, `v2/src/ops/_2_rev.rs`,
     `v2/src/readers/_0_mem.rs`
   - Also: `chat_log/20260416.0.evaluator-store-mutation-design.md`
     (session file, currently untracked; include in commit)
   - Also: `chat_log/20260416.1.session1-phase1/` (this plan folder)
5. Commit via `/hafley:git-commit`.

## Commit message

```
feat(v2): session 1 skeleton — store, mutations, task_guard, OpCtx extension

Type surface for the Session-1 evaluator/store/mutation redesign. No
behavior lands; every new trait impl body is todo!() or unimplemented!().

- src/store/{mod,_0_types,_1_trait}.rs: Store trait + data shapes +
  NoopStore bootstrap helper
- src/mutations.rs: MutationEffect dyn trait + MutationRequest +
  MutationHandler + three unit-struct handler impls (AutoApprove,
  InteractiveCli, LspPromptBridge) + await_approval/spawn_handler
  signatures
- src/_task_guard.rs: JoinHandle wrapper that aborts on drop
- OpCtx gains store/mutations/cancel/expr_name/current_site fields
- Op trait gains expansion_mode() with Exhaustive default
- RuntimeConfig gains max_passes/max_claims_per_pass/max_cursors_per_root
- RunEvent rewritten: Cursor/ExprDone/Diag/MutationPrompt/Done
  (deletes SkipReason and RunStatus; keeps RewriteKind)
- CursorExpr type added
- Pipeline/ForkBranch derive Clone
- 5 existing OpCtx construction sites stubbed with NoopStore + closed
  mpsc to keep compile

Phase 2 adds SqliteStore impl + MutationHandler bodies + await_approval
biased-select body.
```

## Exit state
Single commit on `wip/kitchen-sink-react-hook-fix` extending `8298afa`.
Tree clean. Ready for Phase 2 (SqliteStore impl + mutation handler
bodies — good agent delegation candidate).
