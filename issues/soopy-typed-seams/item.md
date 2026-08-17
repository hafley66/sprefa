---
created: 2026-08-16
updated: 2026-08-16
type: chore
reporter: chris
status: open
priority: normal
epic: soopy-full-wiring
---

# Typed seams: rev encodings, paths, patterns, ReadRequest serde

## Description

Three hand-rolled twins: revision_oid string-encodes RevisionId::Worktree (_1_runtime.rs:329-343) though RevisionId derives Serialize; path_from_cwd (hosts.rs:188-212) re-derives soopy pathspec_at logic; host globs are bare String though soopy::Pattern exists. Plus cross-repo: derive Serialize/Deserialize on soopy::ReadRequest (hafley-rs) and delete the ReadRequestWire twin (sprefa-v6 runtime host _0_types.rs:24-45). Candidates 6,7,11,12.

## Comments

### 2026-08-17T03:51:37Z · @soopy-driver

SCOPING AUDIT, measured against origin/main 7f11724b4. This card is FOUR separate seams and only one is mechanical. Candidate 12 (delete the ReadRequestWire twin): NOT IN THIS REPO. git grep ReadRequestWire across sprefa finds only two IMPORT sites (source_bind/_1_runtime.rs:6, :209) and one test use; the declaration lives in a third repo at sprefa-v6/0_runtime/1_rust_runtime_host/src/_0_types.rs:24-45 per the entanglement plan line 165. soopy::ReadRequest now derives Serialize/Deserialize (hafley-rs PR #9 merged), so the twin's stated reason is gone, but deleting it needs a lane in ~/projects/sprefa-v6, which is outside my named ownership. FORK for the coordinator. Candidate 6 (revision_oid at _1_runtime.rs:329-343): I PUSH BACK on the card's premise. Its output lands in the dl6 rev.oid TEXT column (file_value at :351 builds {rev:{repo,oid}}), so it needs a stable SCALAR spelling; RevisionId's derived Serialize is a STRUCTURAL form and cannot occupy a text column. Swapping it would also churn every golden carrying a worktree rev. This one needs Chris, not a mechanical rewrite. Candidate 7 (path_from_cwd at hosts.rs:188-212 re-deriving pathspec_at): blocked on soopy exporting pathspec_at, which is private at crates/soopy/src/_9_git_files.rs:42. Needs a second hafley-rs PR before the sprefa side can move. Candidate 11 (host globs as bare String though soopy::Pattern exists): the only self-contained piece left, and it is a typing change across host code rather than a one-file swap. RECOMMEND: split this card into four, or re-scope it to candidate 11 alone.

### 2026-08-17T03:52:52Z · @soopy-driver

CANDIDATE 11 ALSO FAILS ITS PREMISE, measured. The card says 'host globs are bare String though soopy::Pattern exists'. The host input at hosts.rs:140-143 is handed straight to soopy::GitFilesQuery.pathspecs (hosts.rs:167-172), i.e. it is a GIT PATHSPEC, not a filesystem glob. soopy's own module doc at crates/soopy/src/_9_git_files.rs:1-4 states the separation is deliberate: 'It deliberately keeps Git pathspec semantics separate from SourceQuery's filesystem glob semantics: this is git ls-files / --with-tree, including its index behavior, with no local path matcher substituted in between.' Retyping it to soopy::Pattern would substitute a local matcher for git's own index-aware matching, which is the exact thing that doc forbids. NET: all four candidates on this card are wrong, blocked, or out-of-repo. 6 = wrong premise (scalar TEXT column vs structural serde). 7 = blocked on soopy exporting pathspec_at. 11 = wrong premise (pathspec vs glob). 12 = declaration lives in ~/projects/sprefa-v6, outside my ownership, though its stated reason is now gone since soopy::ReadRequest derives serde. RECOMMEND: close this card as measured-and-rejected, and file candidate 12 alone as a one-line sprefa-v6 card.

