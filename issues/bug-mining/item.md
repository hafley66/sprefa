---
created: 2026-08-15
updated: 2026-08-15
type: epic
owner: chris
status: open
priority: high
---

# Bug mining: find defects the goldens cannot

## Description

Goldens re-prove pinned shapes; every serious defect this week (mutual recursion, df span aliasing, topo-order DDL crash) lived in a shape no fixture had. This epic holds the generative/differential techniques. Judges already exist: oracle + TS door + Rust door byte-diff, naive emitter mode as a second oracle, dd shrinking arm.
