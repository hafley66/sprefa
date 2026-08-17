---
created: 2026-08-16
updated: 2026-08-17
type: chore
reporter: chris
status: done
priority: normal
epic: soopy-full-wiring
closed: 2026-08-17
closed_by: soopy-driver
commits:
- hash: 59f391fefcbbc2148816c58eaa7bf3822bdcdcdd
  summary: 'PR #332, one 127-crate soopy closure across both lockfiles plus the lockstep rail (recipe fixed by the driver: it invoked a .sh that did not exist)'
---

# One resolution of soopy transitives across the two lockfiles

## Description

extract and engine workspaces resolve soopy's ignore to 0.4.31 vs 0.4.33 (also blake3, globset, clap): ignore::WalkBuilder decides worktree membership (_4_worktree.rs:36), so two builds of one soopy source can disagree on which files exist. Unify workspaces or pin. Candidate 13.

## Comments

### 2026-08-17T03:38:57Z · @soopy-driver

PR #332 posted, green. Base measured 44 divergent (name,version) entries between the two lockfiles' soopy closures, ignore 0.4.31 vs 0.4.33 among them, plus 12 packages one lockfile had never resolved. Both now resolve one 127-crate closure. Rail is v6/tools/soopy-lockstep.py wired as 'just soopy-lockstep'; it parses the two Cargo.lock files and walks the closure itself because cargo tree -p soopy REWRITES the lockfile it reads (measured: dirtied sprefa-engine-rs/Cargo.lock by 192 lines). Gates twice each: extract 131/0, engine 87/0. NOT in scope and deliberately untouched: merging the two workspaces, since sprefa-extract's own [workspace] table exists to keep the v5 tree out of the extraction leaf's build graph.
