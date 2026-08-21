---
created: 2026-08-21
updated: 2026-08-21
type: bug
status: open
priority: medium
---

# `extract --resolve` cannot read (blob oid, path) pairs; it walks the worktree

## Description

The engine's file surface answers `(path, digest)` where `digest` is a git blob
oid, and `files_at(rev, glob)` now answers that pair at a pinned commit
(`v6/sprefa-engine-rs/src/hosts.rs`, `SoopyFilesExecutor`). The `--resolve` door
of `sprefa-extract` cannot consume that pair. It takes paths and reads whatever
is on disk, so a rev-pinned program's resolve leg silently reads the worktree.

Filed from the soopy-rev-vs-worktree lane per its brief, which says to report
this rather than build it.

## Receipts

| site | what it does |
|---|---|
| `v6/sprefa-extract/src/project.rs:77-93` | `ResolveRequest` carries `paths: &[PathBuf]`, `project_root: Option<&Path>`. No revision field, no content-id field. |
| `v6/sprefa-extract/src/project.rs:449-464` | `read_inputs` hashes what it read (`content_id_of`); it never verifies against an id a caller handed it. |
| `v6/sprefa-extract/src/project.rs:1009-1014` | `SourceTreeBlobSource::open_files` pins `Revision::Worktree` and builds every `ReadRequest` with `expected: None`. |
| `:174`, `:259`, `:542` | the three `open_files` call sites: `resolve_project`, `scip_facts`, `read_inputs_batched`. All three take that worktree pin. |
| `v6/sprefa-extract/src/project.rs:971` | `SourceTreeBlobSource::open(root, revision, patterns)` — the rev-capable constructor already exists and no caller uses it. |
| `v6/sprefa-extract/src/bin/extract.rs:380`, `:487` | both `ResolveRequest` constructions. No `--rev` flag exists to fill. |

The in-process door is NOT affected and already does this correctly: the linked
`sprefa_extract` executor takes `digest` off the demand row and reads it through
`read_blob`, which hits the object database and falls back to the worktree file
only after re-hashing and comparing (`v6/sprefa-engine-rs/src/hosts.rs`,
`SprefaExtractExecutor::run` digest branch). So the gap is the BINARY's
`--resolve` path, not the library's per-file surface.

## Second defect in the same type

`SourceTreeBlobSource` holds two different notions of "the worktree" under one
name:

- `open_worktree` (`project.rs:962`) goes through `SourceTree::snapshot`, which
  for `Revision::Worktree` routes to soopy's FS WALK (`_4_worktree.rs`). It sees
  untracked and ignored-but-present files and mints `ContentId::Blake3`.
- `open_files` (`project.rs:1009`) reads worktree `SourceRef`s directly.

A caller cannot tell from the constructor name which enumeration it got.

## What a fix looks like

1. `ResolveRequest` gains a revision, spelled the way the rest of the tree spells
   it (`WORK` or a rev name, `change_facts::parse_revision`).
2. The three `open_files` call sites pass it through to the existing
   `SourceTreeBlobSource::open`, with `expected` set from the caller's content id
   so a mismatch is a named stop and not a silent worktree read.
3. `extract --resolve` gains the flag that fills it.
4. A fixture with a committed file and an uncommitted edit, asserting the pinned
   resolve reads the committed blob. Same shape as
   `v6/sprefa-engine-rs/tests/revision_walk.rs`.

Ledger entry: `docs/failure-modes.md` row 62, residual (b).
