---
created: 2026-08-21
updated: 2026-08-21
type: bug
reporter: chris
assignee: chris
status: fixed
priority: normal
epic: cheap-fast-analysis
labels: [extract, perf, lane-a]
closed: 2026-08-21
closed_by: chris
commits:
- hash: b3a3cd7e8
  summary: LineTable per document; 65 files 4.7s
---

# join_documents runs once per file: resolve over 82 files never finishes

## Description

lang/rust.rs:978, go.rs:1933, ts.rs:3168 call join_documents(index, reader) per file; 82 x 129 whole-corpus reads, killed at 506s with 0 rows. OnceLock on IndexBag. Lane A.

## Comments

### 2026-08-21T16:07:46Z · @chris

join_documents once per project landed (eeed51ff6, COUNT test 15 to 5). Resolve over 82 files STILL does not finish: sample puts 2477/2484 stacks in site_occurrence -> byte_range, which rescans each document from offset 0 per occurrence. Needs a per-document line-offset table. Ledger entry 59.
