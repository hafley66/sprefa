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
