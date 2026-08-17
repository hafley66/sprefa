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
- pkg:tsv2
closed: 2026-08-16
closed_by: fable
---

# TS door rev-pinned file feed turns rev-parse failure into a phantom row

_Source: v6/tsv2/goldens/scip_combo/1_rev_file_skew.dl6 (pinned defect F1)_

## Description

## Comments

### 2026-08-16T05:31:17Z · @fable

Mechanism (PR #291): git rev-parse echoes its argument on failure; the emitted template's [ -n "$oid" ] guard passes and absence becomes a row whose digest is the literal <rev>:<path>. added_path answers 4 rows on Rust, 0 on TS. Seven emitted declarations carry the guard; two claim the opposite in their headers.

## Resolution

### 2026-08-16T05:58:42Z · @fable

PR #296 (three scip_combo templates, lane) + PR #297 (four v6/dl/fixtures twins, coordinator). All seven verified-guard; scip-combo now 33 rels byte-identical with only F2 pinned; precommit-changed and files gates hold 3x.
