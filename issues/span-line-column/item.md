---
created: 2026-08-14
updated: 2026-08-16
type: feature
status: done
priority: normal
epic: v5-behavioral-parity
labels: [parity, v6]
closed: 2026-08-16
closed_by: fable
---

# Span-derived text, line, and column relations

## Description

## Goal
Author span-derived text, line, and column relations: from a byte span owned by a file, project the containing line, column, and the slice text.
## Where to put it
- v6/sprefa-engine-rs/src/text_plane.rs — currently the text-intern plane only (102 lines); add the byte->line/col/slice projection here or a sibling text_project.rs if it crosses ~500 lines.
- SourceBind relation declarations in source_bind/_0_types.rs if these ride the tick path.
## Perf gate
- v6/justfile: just scale-floor (line/column projection at 10k arrivals, stmts/tick flat)
## Implementation Notes
Line/column derive from file bytes, not stored spans; keep the projection read-only and off the id columns (text_plane runs before apply_arrivals).

## Resolution

### 2026-08-16T05:22:31Z · @fable

Landed PR #288: LineOffsetIndex byte-span to line/col/slice in text_plane, 7 tests x3.
