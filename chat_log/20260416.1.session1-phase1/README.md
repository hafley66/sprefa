# Session 1, Phase 1 — skeleton

Type surface for Session 1 of the evaluator/store/mutation redesign.
Every new type, trait, field, and variant lands here. All behavior
bodies are `todo!()`.

Execute in order: 1a → 1b → 1c → 1d.

## Source of truth
- Session design: `chat_log/20260416.0.evaluator-store-mutation-design.md`
  (read "## Refinements" and "## Zoom 3 pseudo" sections first; they
  supersede anything in the body above them).

## Status as of 2026-04-16
- 1a: landed (agent pass, stopped mid-sweep but new files are sound)
- 1b: landed (RunEvent rewrite is in)
- 1c: partial — 8 `RuntimeConfig { ... }` literal sites still need new
  fields added; `mutations.rs:89` has a wrong BufReader import
- 1d: pending

## Files
- `1a_new_modules.md` — TaskGuard, store/*, mutations.rs, Cargo deps, lib.rs wiring
- `1b_runevent_cursorexpr.md` — RunEvent rewrite, CursorExpr, Pipeline Clone
- `1c_opctx_runtimeconfig_ctors.md` — OpCtx + RuntimeConfig extensions and ctor backfill
- `1d_verify_commit.md` — cargo build --tests green, single commit
