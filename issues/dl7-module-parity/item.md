---
created: 2026-08-31
updated: 2026-08-31
type: task
status: open
priority: high
epic: dl7-module-system
labels:
- size:med
size: M
lane: dl7-module-system
lane_seq: 0
collision: [v7-reader, v7-compiler, v7-test]
blocked_by: ['@dl7-type-algebra-oracle']
---

# Port DL6 module semantics into DL7

## Description

## Description

Port the reusable module ownership and import semantics from v6/prolog/use_resolve.pl and v6/prolog/0_dot_expand.pl into the DL7 prefix reader and checker. Preserve file-owned identities, aliases, mounted paths, dotted type references, and cycle diagnostics.

## Acceptance Criteria

- [x] V7 files compile as separate module-owned units.
- [x] Imports preserve declaring-module type identity.
- [ ] Alias and dotted reference fixtures reach the same owner.
- [x] Import cycles and ambiguous unqualified names are diagnosed.
- [x] Reused DL6 predicates are listed with source signatures and adaptations.

## Tests Run

- [x] Consolidated V7 module fixture passes.
- [x] Existing V7 SWI and Tree-sitter gates pass.

## Agent Runs

### 2026-08-31T06:20:41Z · @codex

Implementation receipts on feature/dl7-module-system: 9f192e498 plan, b844f6da2 separate-unit loader, 7c82520cb stable source owners, a429ef1a4 independent merge, cd0071d90 alias identity, 86b81c064 separate prelude compilation, 4029fc865 path proofs/collisions/cycles. Gates: SWI 34/34; Tree-sitter 1/1. Prefix import/export spelling and source-position wiring remain open.
