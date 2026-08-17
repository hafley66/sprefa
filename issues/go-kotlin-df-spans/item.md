---
created: 2026-08-15
updated: 2026-08-15
type: bug
reporter: fable
status: fixed
priority: normal
labels:
- size:small
- area:extract
- pkg:extract
related: ['@df-span-identity-aliasing']
closed: 2026-08-15
commits:
- hash: d73c5c3a
  summary: 'v6/extract: go+kt df nodes carry full extents, not len 0'
---

# go.rs and kotlin.rs df nodes still carry len 0

## Description

Twin of the fixed df-span-identity-aliasing (PR #270): src/lang/go.rs and src/lang/kotlin.rs df_push still store start-only spans, and the FlatFact::DfArg/DfParam aux arms carry spans with no kinds. Mirror the rust.rs fix: full extents at every df_push call site, per-language fail-first test on the ret-covers-tail shape, regenerate affected goldens. Small: the pattern, the wire fields, and the test shape all exist from #270.
