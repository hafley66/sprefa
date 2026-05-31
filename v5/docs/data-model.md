# The data model (the contract)

This file pins the file-identity model so it stops getting re-litigated every
version. v3, v4, and v5 each re-derived it; v4 got it right; v5 collapsed it to
ship the call graph fast. This is the stable form. Build on these relations, not
on the collapsed `_file` table.

## Implementation status

- **Stage 1 (landed, commit 67af575):** `repo` / `rev` / `content` / `file` are
  DSL-queryable built-in relations, refreshed each tick from the `_file` cache.
  Any rule joins the file set without a `scan`. Reserved names (declaring one
  errors). Stage-1 ids are the raw rev string / content hash (no interning).
  `_file` remains the change-detection cache. See
  `plans/2026-05-30-data-model-migration-plan.md`.
- **Stage 2a (landed on `codex/v5-refresh-type-edge`):** v5 has native spine
  primitives for `StringId`, `FileId`, `Coord`, `RefId`, and `WhereBytesId`,
  plus internal `_strings`, `_files`, and `_where_bytes` tables with zero
  sentinels. The tables are not queryable built-ins yet.
- **Stage 2 remaining:** populate real strings/spans, expose queryable `ref`,
  add file-id-keyed `_prov` retraction, and wire multi-repo config. Do the
  cross-repo pieces when cross-repo is real.

## The one idea: content is separate from location

A file's bytes and where you found those bytes are two different things.

- Address bytes by **hash** (content).
- Let **location** (repo, rev, path) point at content.

The same bytes at ten paths or ten revs is one content row and ten location
rows. That separation is the whole reason the model is stable: a new repo, a new
rev, or an unsaved editor buffer is a new location row pointing at existing or
new content. Nothing restructures. Adding a "where" never changes the shape.

## The four layers

```
rel repo(id: text, slug: text, root: path).
rel rev(id: text, repo: text, oid: text, ts: int).
rel content(id: text, hash: text).
rel file(repo: text, rev: text, path: text, content: text).
```

| relation | what it is | one row per |
|---|---|---|
| `repo` | a repository | repo (slug + on-disk root) |
| `rev` | a revision inside a repo | commit / working tree |
| `content` | the bytes, deduped | distinct file content |
| `file` | a location pointing at content | (repo, rev, path) |

`file` is the join hub. Every higher fact (`calls`, `module_edge`, `token`, ...)
keys off `file`, so cross-repo and cross-language are edges between `file` rows
with no new graph code.

## The three levels of "where bytes live"

The level is not a type. It is which columns are sentinel.

| level | repo | rev.oid | content source |
|---|---|---|---|
| committed in a repo | real slug | real git oid | git blob OID (already content-addressed, free) |
| on disk, working tree | local repo | `WORK` | blake3 of disk bytes |
| RAM only (editor buffer / synthetic) | sentinel | `WORK` or buffer id | blake3 of buffer bytes, never on disk |

One schema, three levels. A query that wants "all three" filters on nothing; a
query that wants "committed only" filters `rev.oid <> 'WORK'`.

## Sentinels (how "absent" is represented)

No nulls. Absence is a reserved id, so joins never special-case.

| layer | sentinel | meaning |
|---|---|---|
| `repo` | id `0` | no repo (loose file, not under version control) |
| `rev` | id `0`, oid `''` | no revision |
| `content` | id `0` | synthetic / empty |
| `WORK` rev | oid `'WORK'` | the working tree (a rev that is not a commit) |

This is v4's `preinsert_sentinels` discipline ([store.rs:214](../../v4/src/store.rs#L214)).

## Interning keys (recovered from v4)

Content-derived ids, so the same thing in two runs gets the same id and dedups.

| id | derivation | source |
|---|---|---|
| `RepoId` | `blake3(slug ‖ remote)[..4]` | [store.rs:511](../../v4/src/store.rs#L511) |
| `RevId` | `blake3(repo ‖ oid)[..4]` | [store.rs:559](../../v4/src/store.rs#L559) |
| `FileId` (content) | content hash; git blob OID when committed, blake3 otherwise | [store.rs:336](../../v4/src/store.rs#L336) |
| `PathId` | over `(repo, rev, file, path)` | [store.rs:268](../../v4/src/store.rs#L268) |

The git blob OID is reused directly for committed content. It is already
`blake3`-free, already content-addressed, already in the object store, so
committed files cost no extra hashing.

## v4 ↔ v5-today ↔ v5-target

| concern | v4 | v5 today | v5 target |
|---|---|---|---|
| repo | `_repos(id, slug, remote)` | implicit (`--root` flag) | `repo(id, slug, root)` relation |
| rev | `_revs(id, repo, oid, ts)` | `rev` column on `_file` | `rev(id, repo, oid, ts)` relation |
| content | `_files(id, hash, path)` | `hash` column on `_file` | `content(id, hash)` relation |
| location | `_paths(id, repo, rev, file, path)` | `_file(path, rev, hash, mtime, size)` PK(path,rev) | `file(repo, rev, path, content)` relation |
| interning | content-derived ids, LRU + cold storage | none (denormalized) | content-derived ids |
| cross-repo | yes | no (no repo dimension) | yes |

v5's [`_file`](../src/engine.rs#L422) fused location and content and dropped
repo. That was the right shortcut to ship the call graph. It is also exactly why
cross-codebase does not fit yet.

## Invariants (keep these or the model rots)

1. **Content is addressed by hash, never by path.** Two paths with identical
   bytes share one `content` row.
2. **`file` is the only place repo, rev, path, and content meet.** Higher facts
   reference `file`, not raw path strings.
3. **Absence is a sentinel id, never a null.** Joins stay uniform.
4. **`WORK` is a rev, not a special case.** The working tree is a revision whose
   oid is the literal `'WORK'`.
5. **Never collapse the four layers again.** Columns may be added; the
   content/location split may not be removed.

## Why this is not nested datalog

There is no nesting here, and that is the point. The model is flat relations plus
joins. "A repo has revs, a rev has files" is expressed as three relations joined
on ids, not as a nested record. Flatness is what lets Tarjan/SCC run over the
edge set and what keeps the storage a plain set of rows. Glean's `{}` is surface
sugar that desugars to exactly this.

## What builds on top

| fact | keys off | gives |
|---|---|---|
| `token(file, line, col, kind)` | `file` | AST highlight as span queries |
| `calls(caller, callee)` over `file` | `file` | call graph |
| `module_edge(src_file, dst_file)` | `file` | cross-file / cross-repo / cross-language import graph |
| `closure(module_edge)` | edge relation | cycles, fan-in, fan-out, reaches, all for free |

Highlighting driven by the graph (color a function by `unused(f)`, by SCC
membership, by `reaches`) is the extension v3/v4 could not do: the highlighter
becomes a query over the same edges.
