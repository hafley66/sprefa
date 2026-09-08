---
created: 2026-09-08
updated: 2026-09-08
type: chore
reporter: fable
status: open
priority: low
epic: extract-port-closeout
labels:
- pkg:extract
- review-field-test
---

# extract: keep phase-1 records under --resolve behind --with-phase1

## Description

## Description

Under `--resolve` the per-file phase-1 records (node, edge, sig, site, specifier, unresolved) disappear, so the 2026-09-08 review lane collected them file by file in a shell loop and joined offsets by hand. Add `--with-phase1` that streams them too, each carrying a `path` field so a multi-file stream stays self-describing. See research/2026-09-08-fact-sources-beyond-scip.md §5 row 8.

## Acceptance Criteria

- [ ] `extract --resolve --with-phase1 a.ts b.ts` emits both record classes, phase-1 rows carry `path`
- [ ] `--schema` documents the field
- [ ] default output unchanged

## Tests Run

- [ ] two-file fixture

## Implementation Notes

- [ ] path field only added under the flag to keep single-file output byte-stable
