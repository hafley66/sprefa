---
created: 2026-08-16
updated: 2026-08-16
type: bug
reporter: fable
status: fixed
priority: high
epic: bug-mining
labels:
- bugmine
- pkg:engine-rs
closed: 2026-08-16
---

# FamilyMask parser catch-all drops unknown family names to NONE silently

_Source: v6/tsv2/goldens/scip_combo/7_door_skew_family.dl6 (pinned defect F2)_

## Description

## Comments

### 2026-08-16T05:31:17Z · @fable

Mechanism (PR #291): the in-process FamilyMask parser drops every non-mask family name on its catch-all arm, so --family diet_scip leaves FamilyMask::NONE and the host succeeds with zero rows (TS door answers 2). Same file refuses an unknown FLAG by name; families deserve the same refusal.
