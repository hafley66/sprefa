---
created: 2026-08-31
updated: 2026-08-31
type: task
status: open
priority: high
epic: dl7-type-algebra
labels:
- size:med
size: M
lane: dl7-type-algebra
lane_seq: 13
collision: [v7-reader, v7-compiler, v7-test]
blocked_by: ['@dl7-type-algebra-oracle']
---

# Port DL6 module semantics into DL7

## Description

## Description

Port the reusable module ownership and import semantics from v6/prolog/use_resolve.pl and v6/prolog/0_dot_expand.pl into the DL7 prefix reader and checker. Preserve file-owned identities, aliases, mounted paths, dotted type references, and cycle diagnostics.

## Acceptance Criteria

- [ ] V7 files compile as separate module-owned units.
- [ ] Imports preserve declaring-module type identity.
- [ ] Alias and dotted reference fixtures reach the same owner.
- [ ] Import cycles and ambiguous unqualified names are diagnosed.
- [ ] Reused DL6 predicates are listed with source signatures and adaptations.

## Tests Run

- [ ] Consolidated V7 module fixture passes.
- [ ] Existing V7 SWI and Tree-sitter gates pass.
