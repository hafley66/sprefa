# Brief: sqlite_ivm backends fully behind the trait

Repo: sqlite_ivm (+ the dl8 call site). From `2026-10-02-engine-parity.report.md` section 3.

## Goal

- Each backend opens and owns its storage in `Engine::install`; `Host` loses
  `conn() -> Option<&dyn Any>` (`ivm-engine/src/2_host.rs:9`, downcast at
  `ivm-sqlite/src/4_engine.rs:49`). dl8 `Hosted<E>` stops holding a `rusqlite::Connection`.
- Trait defaults that return `Unsupported` at runtime (`intern_snapshot`, `intern_text`, `text`,
  `intern_any`, `any_value`, `intern_terms`, `topk`, `window`, `letrec`) become required methods;
  every backend implements each one, or the method moves to a separate trait the backend opts into.
- dl8 names a concrete backend type in one place only (today `_6_eval/_9b_ir_eval.rs:174-175`).

Method, laws, validation, receipt: as `plans/v8/2026-10-02-strings.brief.md` (sqlite_ivm clone inside
the sprefa worktree, branch `refactor/engine-containment`; rebuild the extension dylib from the
clone; both engines; no merge/push).
