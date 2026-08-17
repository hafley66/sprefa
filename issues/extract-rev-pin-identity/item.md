---
created: 2026-08-16
updated: 2026-08-16
type: bug
reporter: fable
status: open
priority: high
epic: bug-mining
labels:
- bugmine
- pkg:engine-rs
- size:med
lane: engine-source-bind
lane_seq: 10
collision: [source-bind-runtime, engine-hosts]
related: ['@soopy-extract-host-reads']
---

# Extract host reads worktree disk under a rev pin; digest is freshness not identity

_Source: v6/tsv2/goldens/scip_combo/2_extract_rev_skew.dl6 (pinned defect F3)_

## Description

## Comments

### 2026-08-16T05:31:17Z · @fable

Mechanism (PR #291): the extract host reads the path off the worktree disk regardless of rev pin, and digest is freshness, so two demands under two rev digests share one response identity. Pinning the file set does not pin the extraction. Design-flavored: response identity needs the content id, not the path.
