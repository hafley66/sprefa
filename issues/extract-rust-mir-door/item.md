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

# extract: Rust door on rustc_public (stable MIR) with unwind edges as cfg_edge kind=unwind

## Description

## Description

MIR `Call` terminators carry an unwind target: the real exception edge. `rustc_public` is the supported crate for reading MIR from outside rustc. Emit basic blocks and terminators into `cfg_*` with `kind=unwind` edges, typed locals into the type family, and call targets into `resolved_edge`. rust-analyzer HIR (`ra_ap_*`) is the lighter, less precise fallback when a nightly-compatible driver is not acceptable. See research/2026-09-08-fact-sources-beyond-scip.md §2.9, §5 row 5.

## Acceptance Criteria

- [ ] fixture crate with a panicking call inside a scope guard emits a `cfg_edge kind=unwind` to the drop glue
- [ ] toolchain requirement documented; a run without it emits a `scip_skip`-style row, never an empty stream
- [ ] cold time recorded

## Tests Run

- [ ] fixture crate

## Implementation Notes

- [ ] driver binary pinned to a toolchain; decide nightly vs stable_mir crate availability first
