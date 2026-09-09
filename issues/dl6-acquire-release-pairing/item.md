---
created: 2026-09-08
updated: 2026-09-08
type: task
reporter: fable
status: open
priority: high
epic: extract-port-closeout
labels:
- pkg:extract
- review-field-test
---

# dl6 rule: acquire/release pairing over site + cst spans

## Description

## Description

Three findings in the 2026-09-08 field test had one shape: call X (acquire, subscribe, addEventListener) at offset a, matching call Y (release, unsubscribe, removeEventListener) only inside a `finally` whose `try` span does not contain a, or nowhere. Ship the shape as a dl6 rule over `site` + `cst` spans, parameterised by (X, Y) pairs, so the question is one query. Depends on @extract-cfg-exception-edges for the path-sensitive form; the span-containment form works today. See research/2026-09-08-fact-sources-beyond-scip.md §5 row 2.

## Acceptance Criteria

- [ ] rule file under `.dl` with pair table (acquire/release, subscribe/unsubscribe, addEventListener/removeEventListener, open/close)
- [ ] flags the 2026-09-08 `4_test.ts` acquire-before-try fixture and the `globalSetup` no-try fixture
- [ ] zero rows on the fixed package

## Tests Run

- [ ] fixture run

## Implementation Notes

- [ ] span containment first, cfg path form after the epic's cfg issue lands

## Comments

### 2026-09-08T17:48:07Z · @fable

Acceptance fixture and reference queries: research/2026-09-08-codeql-review-queries/ (CodeQL reproduces the expected rows: fixture 3/1/1/1, fixed package 0/0/1/0). Doc: research/2026-09-08-fact-sources-beyond-scip.md §8.
