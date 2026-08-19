---
created: 2026-08-19
updated: 2026-08-19
type: task
status: open
priority: high
epic: productionize-rust-door
size: M
---

# CI-KNOWN-RED legs fixed or deleted; green-all means green

## Description

## Description
`just green-all` is red by design; `.github/CI-KNOWN-RED.md` allowlists: `1_extraction-clock-golden.sh` (`62 !== 59`), `just typecheck` (golden-flex.ts union too complex, relation_id_access), `flagship-flow.sh` (needs v5 release binary), tsv2-test 4 failures, memory-soak, lsp-diags. Nobody else can run CI until a failing leg means something.
## Acceptance Criteria
- [ ] Each allowlisted leg: fixed, or deleted with the reason in the commit, or moved to a named `optional` group; the allowlist file ends empty or lists only `optional` legs.
- [ ] `1_extraction-clock-golden.sh` 62 vs 59 diagnosed to its source (extractor count vs fixture expectation) and fixed at the source, never by editing the number.
- [ ] `just typecheck` green: golden-flex.ts union shape addressed in the TS type emitter (`7_emit_ts_types.pl`), not by a tsconfig flag.
- [ ] flagship-flow: either runs on the v6 Rust door or is deleted (Chris: "I DO NOT WANT TO RUN V5 ANYTHING ANYMORE").
- [ ] Three back-to-back whole-gate runs on one tree agree (CLAUDE.md: measure three times).
