---
created: 2026-09-08
updated: 2026-09-08
type: feature
reporter: fable
status: open
priority: normal
epic: extract-port-closeout
labels:
- pkg:extract
- review-field-test
---

# extract: LSP door (callHierarchy, typeHierarchy, inlayHint, diagnostics) behind the thin-client daemon

## Description

## Description

Every language with a server can answer callers/callees (LSP 3.16 callHierarchy), super/sub types (3.17 typeHierarchy), inferred types as strings (3.17 inlayHint), references and diagnostics. Request-shaped, so a corpus dump is N round trips; that is the trade for coverage on languages with no SCIP indexer. Plans already sketch the daemon: plans/2026-06-02-lsp-via-state-research-plan.md, plans/2026-07-10-lsp-thin-client-daemon.md. See research/2026-09-08-fact-sources-beyond-scip.md §2.8, §5 row 6.

## Acceptance Criteria

- [ ] `extract --family lsp FILE` emits `lsp_call_edge`, `lsp_type_edge`, `lsp_inlay`, `lsp_diagnostic` rows from a running server
- [ ] server chosen by language id; absent server emits a skip row with the install hint
- [ ] warm per-file cost recorded

## Tests Run

- [ ] typescript-language-server fixture

## Implementation Notes

- [ ] one server process per language per run; reuse the daemon if present
