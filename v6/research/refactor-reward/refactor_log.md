# Refactor log — manual Q-learning table

The reward signal (validated across sprefa/ripgrep/serde: structural beats LOC
97% vs 50% for consolidations) applied to sprefa's own src. Each entry is a
(state, action, measured reward) tuple. Moves only count if tests stay green.

Reward abbreviations: `lines` = net LOC of the target fn; `dup` = verbatim
duplicated block count; `fns` = distinct fn count in the target.

## Iteration 1 — consolidate verbatim span-id block in parse_file

- date: 2026-06-26
- target: `engine.rs::parse_file` (god-fn, 401 lines / 108 locals — top of
  param-fan-out ranking)
- smell: the "whole-match span id" block (captures' min-lo/max-hi → WhereBytes
  id + span push) was verbatim-duplicated in the `ast` and `sg` arms
  (engine.rs ~5454 and ~5510, ~18 lines each). Genuine copy-paste, not
  trait-factored.
- state-shape: within-file verbatim block dup (the reward-positive
  consolidation class).
- action: extracted `fn bind_whole_match_span(...)`; both arms call it.
- reward:
  - lines: 401 → 359 (**-42**)
  - dup: 2 → 1
  - fns: +1 (the helper), 2 call sites
- tests: 385 pass (262 integration + 123 unit), unchanged.
- verdict: **WIN**. God-fn shrunk, verbatim dup removed, single source of
  truth, behavior preserved. Policy rule "consolidate verbatim block dup"
  correctly predicted reward-positive — 1/1 on our own code.

## Iteration 2 — extract Match arm of parse_file into bind_match_op

- date: 2026-06-26
- target: `engine.rs::parse_file` (still 359 lines after iter 1).
- smell: the `BodyItem::Match` arm (~49 lines: regex compile, per-line scan,
  per-capture bind, 5-arg id-span binding) was the largest arm of the dispatch.
  God-fn split class (param-fan-out ranked parse_file #1 at 108 locals).
- state-shape: oversized dispatch arm in a god-fn (reward-positive split class;
  LOC-reward validated 96% on splits).
- action: extracted `fn bind_match_op(binds, regex, mlv, idv, content,
  where_file, re_cache, where_bytes, repo, path) -> Result<Vec<Bind>>`; the arm
  parses `mlv`/`idv` then calls it. The `push_span` closure's guard was inlined
  faithfully (push only when `where_file` is Some and text non-empty).
- reward:
  - parse_file lines: 359 → 310 (**-49**); cumulative 401 → 310 (**-91, -23%**)
  - arm count in parse_file unchanged (still 9); one arm now delegates
  - new testable unit: `bind_match_op` (regex-scan binding in isolation)
- tests: 385 pass, unchanged.
- verdict: **WIN**. God-fn decomposing, behavior preserved. Split policy
  correctly predicted reward-positive — 2/2 on our own code.

## Iteration 3 — extract bind_captures closure (3-site caps loop)

- date: 2026-06-26
- target: `engine.rs::parse_file` (309 lines after iter 2).
- smell: the per-capture binding loop (`for (n, t, lo, hi) in caps { ext.insert;
  push_span }`, 4 lines) was verbatim **triplicated** across the `ast`, `sg`,
  and `ast_yaml` arms (engine.rs 5525, 5560, 5586). The block finder's top
  signal was the enclosing 10-line Ast↔Sg verbatim block (bind_whole_match_span
  call + caps loop + `next.push` + outer-loop tail).
- state-shape: within-file verbatim block dup, 3 sites (reward-positive
  consolidation class; structural signal validated 97% across 3 repos).
- action: extracted a `bind_captures` closure directly after `push_span`
  (captures it, so `push_span` stays the single source of truth for the
  span-push logic across all 7 of its call sites). All three arms now call
  `bind_captures(&mut ext, caps, &mut where_bytes)`. Chose closure-over-fn
  because `push_span` is itself a closure capturing `where_file`; a standalone
  fn would have had to inline push_span's body, moving the dup rather than
  removing it.
- reward:
  - parse_file lines: 310 → 309 (**-1**); cumulative 401 → 309 (**-92, -23%**)
  - dup: 10-line Ast↔Sg verbatim block GONE (no longer in finder output); caps
    loop 3 → 1 source
  - the block finder's former #1 signal cleared; remaining parse_file blocks
    (5532/5564, 5556/5580) are the wider per-hit scaffolding whose
    optional-var inserts differ per arm — the "wide bet", needs a design call
- tests: 385 pass (262 integration + 123 unit), unchanged.
- verdict: **WIN**. Consolidation class correctly predicted reward-positive —
  3/3 on our own code. LOC delta near-zero as expected for consolidations
  (LOC = 23% reward class); the reward is dup removal, which cleared.

## Open iteration candidates (ranked by expected gain)

1. **Split parse_file's remaining arms** into `bind_<op>` helpers (Match is
   the largest at ~55 lines). parse_file → thin dispatch. Biggest remaining
   god-fn reduction (~-200 lines), higher risk (many params to thread).
2. **tick_paths (76 locals)** and **reconcile_sources (60 locals)** — other
   god-fns from the param-fan-out ranking.
3. **Wide bet — per-hit scaffolding** (engine.rs 5532/5564, 5556/5580): the
   `let mut next / for b in &binds / for ... in &hits / let mut ext = b.clone()`
   loop frame is verbatim across the arms, but the optional-var inserts differ
   per arm (`alv/elv` vs `slv/clv/ellv/eclv`). Needs a design call: helper takes
   the optional vars as params, or takes a closure that inserts them. Medium
   risk, ~-12 lines. This is the residue left after iter3's narrow bet.
4. Consolidation detector (trait-aware) found little else genuine in src —
   the parallel Kotlin/Rust impls are already trait-unified (`TypeLang`,
   `ModuleResolver`), i.e. the reward-positive collapse already happened.
