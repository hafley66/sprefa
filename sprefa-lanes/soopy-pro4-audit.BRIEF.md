# Soopy pro4 audit

Review the uncommitted Soopy crate in the primary checkout at:

```text
/Users/chrishafley/projects/hafley-rs/crates/soopy
```

Also review its current uncommitted consumers at:

```text
/Users/chrishafley/projects/sprefa/v6/sprefa-engine-rs
/Users/chrishafley/projects/sprefa/v6/sprefa-extract
```

This lane is an independent review. Do not edit those primary-checkout files.
Write `REPORT.md` at the root of the lane worktree.

## Intended boundary

- Soopy owns filesystem traversal, Git repository/revision/worktree mechanics,
  watching, text search, fuzzy ranking, and typed source coordinates.
- Sprefa types and runtime semantics stay outside Soopy.
- The `git` executable is an allowed subprocess backend.
- Search should bind ripgrep's Rust crates directly instead of invoking `rg`.
- Fuzzy ranking should use a Rust library such as high-level `nucleo` instead
  of invoking `fzf`.
- The CLI remains `clap`; no terminal UI framework is selected.
- Watches must cover filesystem changes plus Git repositories, linked
  worktrees/checkouts, HEAD, refs, and index state using typed deltas.

## Review questions

1. Trace all public call paths and external effects.
2. Check repository, worktree, revision, path, and content identity invariants.
3. Check enumeration-to-read round trips for every query/revision mode.
4. Check watcher completeness, cache lifetime, event loss, and Git-state
   classification.
5. Check exact parity of `files`, `files_at`, `repo_files`, and
   `repo_files_at` against their shell contracts, including cwd-relative and
   repository-relative pathspec behavior.
6. Check path safety, malformed Git output, unusual filenames, symlinks,
   linked worktrees, bare repositories, deleted tracked files, and failed Git
   commands.
7. Evaluate the proposed native Rust search and fuzzy-ranking boundary using
   existing libraries. Do not propose hand-rolled replacements.
8. Report missing tests and give minimal deterministic reproductions.

## Known findings to confirm or refute

- `repo_files*` may pass process cwd where repository root semantics are
  required.
- Public `RepoPath` plus `read_many` may permit root escape.
- Commit reads may trust an arbitrary expected blob without checking
  `commit:path`.
- `GitFilesQuery` worktree content IDs may fail to round-trip through
  `read_many`.
- `WorktreeCache` may retain deleted paths.
- Worktree and commit symlink enumeration may differ.
- `RepositoryId` may identify linked worktrees separately.
- Dirty-state resolution may ignore failed `git status`.

## Report format

1. Findings ordered by severity, each with absolute file and line references.
2. Call-path and data-shape map.
3. Confirmed and refuted known findings.
4. Additional findings.
5. Native-library boundary assessment.
6. Missing deterministic tests.
7. Recommended implementation order constrained to existing libraries.

Run relevant read-only tests where possible. Record exact commands and results.
Do not commit or modify the reviewed primary checkouts.
