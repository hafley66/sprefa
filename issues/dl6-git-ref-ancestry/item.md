---
created: 2026-08-14
updated: 2026-08-16
type: feature
status: done
priority: high
epic: v5-behavioral-parity
labels: [parity, v6]
closed: 2026-08-16
closed_by: fable
---

# DL6 bindings for refs, tags, tag history, merge-base, ahead/behind, revision ancestry

## Description

## Goal
Author DL6 bindings for refs, tags, tag history, merge-base, ahead/behind, and revision ancestry. Soopy holds the mechanics; the engine must project them into authored relations through the SourceBind tick path.
## Where to put it
- v6/sprefa-engine-rs/src/hosts.rs — new soopy executor arms (follow SoopyFilesExecutor: name + execution tag, no child spawn).
- v6/sprefa-engine-rs/src/source_bind/_0_types.rs + _1_runtime.rs — relation declarations and arrivals.
- v6/sprefa-engine-rs/src/driver.rs — schedule these git hosts on the tick.
## Perf gate
- v6/justfile: just multirepo-golden (dep/repo/rev crawl both engines over the pinned four-repo corpus)
- v6/justfile: just v5-parity (built-in rel coverage table, run when you want the number)
## Implementation Notes
This is the next parity slice the plan names: expose ref/tag/ancestry/watch results as authored DL6 relations and feed them through the same SourceBind tick path.

## Resolution

### 2026-08-16T05:22:31Z · @fable

Landed PR #290: git_ref/git_tag/git_merge_base/git_ahead_behind/git_ancestor host arms on the demand path; 8_git_gate.sh 5/5 byte-identical x3. Owed: just recipe for 8_git_gate.sh (justfile was lane-forbidden).
