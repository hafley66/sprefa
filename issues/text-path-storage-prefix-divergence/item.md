---
created: 2026-08-18
updated: 2026-08-18
type: bug
status: open
priority: high
labels: [domain-v6, component-lower]
---

# Text compile path mints module-prefixed storage names, term path does not: all 349 fixtures diverge

## Description

On origin/main after 134fd8abd (test: execute module-prefixed storage names) and 7072e4c90: bash v6/prolog/compile/scripts/text_door_receipt.sh reads compiled=349 byte_identical=0 failures=349. The text path emits storage names like "module_path_in_body_reads_the_flat_rel_orchard__fruit"; the term path emits the unprefixed name. Every fixture fails, not only module-path ones. Also: cd v6 && just plunit hangs at mount_door:source_mutations_fixture_keeps_one_document_boundary_and_exact_approval_join (7m timeout, twice, on a clean base). Neither is in .github/CI-KNOWN-RED.md. Both come from the peer session's module-storage-prefix line pushed straight to main. Measured by the list(mod.rel) lane on 2026-08-18 (PR #358 body).
