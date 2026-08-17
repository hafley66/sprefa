---
created: 2026-08-16
updated: 2026-08-17
type: bug
reporter: chris
status: fixed
priority: normal
epic: soopy-full-wiring
closed: 2026-08-17
closed_by: soopy-driver
commits:
- hash: 26344cf494394c77db13be77c88f0b84f90bedd4
  summary: 'PR #335, ref memo keyed on a stat-only ref-store witness; driver added b02bb4833 so a memo hit spawns nothing'
---

# GitRefExecutor memo never invalidates: keyed on repo alone

## Description

hosts.rs:440 memoises ref snapshots per repo with no rev, name, or mtime in the key; refs move, the memo does not. Add a freshness witness to the key (or soopy RepositoryWatcher, currently dead surface). Candidate 10.

## Comments

### 2026-08-17T03:56:04Z · @soopy-driver

PR #335 posted, green, 89/0 twice. Witness measured on this repo (92 loose refs, 64KB packed-refs): stat walk 1.60 ms vs one git for-each-ref 31.15 ms, and the snapshot costs THREE such spawns, so the witness is roughly 2 percent of the work it decides whether to redo. Posted as a PR comment with the method stated. READ BEFORE MERGING: this PR REPLACES a committed test that asserted the stale behavior. the_ref_memo_settles_one_snapshot_per_repository said 'a ref added after the first demand does not appear, which is what makes the four host names one for-each-ref pass'. That rationale survives (a quiet store still answers every host name from one enumeration, pinned by the_ref_memo_settles_one_snapshot_per_quiet_repository); only the staleness assertion goes. If Chris wants the old semantics, this is the hunk to revert. Also pinned: the_ref_memo_survives_a_pack_refs, because git pack-refs DELETES the loose ref files while rewriting packed-refs, and a witness watching only loose refs would read the store as emptied.
