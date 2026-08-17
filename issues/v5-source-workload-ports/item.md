---
created: 2026-08-14
updated: 2026-08-16
type: feature
status: done
priority: normal
epic: v5-behavioral-parity
labels: [parity, v6]
closed: 2026-08-16
---

# End-to-end ports of V5 source workloads (changed_line, rev_behind, tag crawl, dependency crawl)

## Description

## Goal
End-to-end ports of the V5 source workloads: changed_line, rev_behind, tag crawling, and dependency crawling, running against the V6 engine through SourceBind.
## Where to put it
- v6/dl/fixtures/*.dl6 — the authored programs.
- v6/tsv2/scripts/ — receipt scripts following the existing *_golden / _gate.sh pattern (rtkq-golden, multirepo-golden, precommit-changed are the templates).
- v6/tsv2/goldens/ — pinned corpora for the receipts.
## Perf gate
- v6/justfile: just v5-parity (coverage table — run when you want the number, run outside batteries)
- v6/justfile: just multirepo-golden (dependency crawl)
## Implementation Notes
Each port needs a byte-diffable receipt against a pinned corpus before it can be called parity, not just a passing run.

## Comments

### 2026-08-16T16:57:55Z · @codex

Verified 2026-08-16 on main 8c4d66ca6: just ghcacher-golden, precommit-changed, git-refs-golden, and dep-crawl-golden all exit 0. changed_line/change facts are covered by PR #293; ref/tag/merge-base/ahead-behind/ancestry by PR #290; dependency frontier closes byte-identically against the pinned v5 golden; ghcacher clock golden holds. The umbrella description was stale.
