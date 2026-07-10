---
name: project-lsp-maintenance-debt
description: LSP integration carries large unpaid maintenance debt; weigh against green-field work
metadata: 
  node_type: memory
  type: project
  originSessionId: b5f0ade9-540e-4fda-9f7a-284766ab6419
---

User flag 2026-05-19: significant unpaid maintenance debt in the LSP
integration. Specifics not captured yet (ask user or investigate).

Codebase pointers (verify before relying):
- ops: `LspErrorDef`, `LspWarnDef`, `LspInfoDef`, `LspHintDef`,
  `LspHoverDef`, `LspHoverAliasDef`, `ExpectZeroDef`, `ExpectMatchDef`
  (registered in `v4/src/compile/lower/mod.rs::default_registry`)
- examples touching LSP: `lsp-flow-smoke.sprf`,
  `lsp-retraction-runtime-html.sprf`, `dogfood-comment-region-lsp.sprf`,
  `dogfood-rust-doc-lsp.sprf`

Apply when planning: any plan that touches walker / lowering / cons
unification ([[project-cons-calling-unification]]) / callable-Value
([[project-callable-value]]) MUST consider LSP impact before piling on.
Don't deepen the debt for green-field reasons. If a cons-plan step
risks LSP regression, call it out at planning time, not after.

Inventoried 2026-05-19 (agent): 20 items. Big fires:
- D12+D2+D17: ~250 LoC dup IN lsp.rs (OperatorDef ×8 boilerplate, 3
  near-identical render_message bodies, 3 dup span resolvers).
  EXACTLY cons-plan step 1-2 blast radius — pay BEFORE cons kickoff.
- D3: std::sync::Mutex held across async LSP handlers through full
  re-parse (app.rs:769 holds lock through ingest). Worst perf risk.
- D4: lsp_change = full re-ingest per keystroke; no debounce, no
  version gate. Parallel-trackable.
- D5: sentinel coupling — Diag code "sprf/hover" gets filtered into
  runtime_hovers (app.rs:2035 split_runtime_hovers). Namespace
  collision risk.
- D6+D7: dual Cargo.lock + lsp-types 0.94/0.97 serde-bridge via
  crosswalk(). 30-min workspace patch closes both.

Verdict: LSP is dogfood + tower-lsp prototype, NOT load-bearing for
any shipped capability. `lsp_error`/`lsp_hint` have zero callers;
tower-lsp Backend is integration-untested. So debt isn't blocking
ship — only the items that intersect cons-plan's surface
(`value.rs`/`walk.rs`/`registry.rs`/`ctx.rs`) need fixing before
cons kickoff (= D12+D2+D17, recommended). D3/D4/D5/D7/D14
parallel-trackable. D6 cheap regardless.
