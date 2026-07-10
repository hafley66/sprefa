---
name: project-host-lsp-trait-architecture
description: "host-LSP plan got 6 FATAL from trait-design lens; needs v2 patch before BUILDABLE; type-IR plan deliberately routes around it"
metadata:
  node_type: memory
  type: project
  originSessionId: b5f0ade9-540e-4fda-9f7a-284766ab6419
---

`plans/2026-05-19-host-lsp-trait-architecture.md` proposes
`HostLspDef`/`HostLspNode` trait pair mirroring `OperatorDef`/`Component`,
15 host-concept impls, `DslBodyLsp` by composition. 3-agent feedback round
2026-05-19 (walker / concurrency / trait-design lenses):

- walker = 0 FATAL, 8 IMPORTANT (top: 6-site sig threading for nodes_out;
  no no-cost-when-off story; +1 trait surface 0 ops removed; cons-plan
  steps 0/1/3/4/5/6 each transitively force new/moved stamp calls)
- concurrency = 0 FATAL, 6 IMPORTANT (top: H1 mitigation aimed at wrong
  half — real hazard is READ-side lock-across-dispatch; H6 `Weak<Rule>`
  DEAD on arrival since `Rule` not behind `Arc<Rule>` anywhere; per-keystroke
  cost grows ~200 boxed payloads; `DocState.lsp_index` must be
  `Arc<LspIndex>` to attain lock-drop discipline)
- trait-design = **6 FATAL** (broke architecture):
  - F1 `OperatorDef:HostLspDef` analogy collapses (parser-discovered key
    vs walker-invented `kind_id`)
  - F2 walker has NO access to `LowerCtx::scope_path` (private, no
    accessor; ctx.rs:102)
  - F3 walker classifications miss bare-term short-circuit
    (walk.rs:196-212 vs classify_slot)
  - F4 `rule_decl` + `rule_call` collide on same byte range at decl site
  - F5 `enrich_diag` imports wrong `Diag` type (cst::diag vs
    `effect_runtime::v2::Diag`)
  - F6 DSL singleton story bunk; `SqlDsl::new()` is per-RPC for a reason

Trait-design agent recommended v2 direction: drop `OperatorDef` analogy
framing; refit as concept-tagging side-channel on existing walker;
payload = sealed `enum HostLspPayload` (not `Arc<dyn Any>`), `kind_id`
derivable from variant. Merge `DslBodyLsp` + `HostLspDef` into one
`Surface` trait. Fix F2 / F4 before any §9 step 1 RED.

Verdict: plan NOT BUILDABLE as written. v2 patch required.
[[project-type-ir-value-space-plan]] deliberately routes around this work
(does not depend on it; types gain hover/dot-into whenever host-LSP v2
lands). [[project-lsp-maintenance-debt]] inventory still stands; plan
as-written grows debt by 1 trait surface without retiring the 8
`LspXxxDef` runtime ops.

Sequencing: when user revisits, read type plan first (lower stakes,
forks to lock); host-LSP v2 second (higher energy, 6 FATAL to resolve).
