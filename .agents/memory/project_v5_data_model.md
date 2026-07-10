---
name: v5-data-model
description: the pinned file-identity data model contract for v5 (repo/rev/content/file 4-layer); anti-rewrite
metadata: 
  node_type: memory
  type: project
  originSessionId: 72e3adda-ecd3-4611-a57b-c7644b80c664
---

The file-identity data model is now PINNED as a written contract at
`v5/docs/data-model.md` (created 2026-05-26), to stop re-litigating it every
version (v3→v4→v5 each re-derived it).

The one idea: **content is separate from location**. Address bytes by hash
(`content`), let location (`repo`, `rev`, `path`) point at it. Same bytes at N
paths/revs = one content row, N location rows. Adding a "where" never
restructures.

Four layers (v5 target, as `.dl` rel decls):
- `repo(id, slug, root)`
- `rev(id, repo, oid, ts)` — WORK is a rev whose oid = literal `'WORK'`
- `content(id, hash)` — git blob OID reused for committed (free, already content-addressed)
- `file(repo, rev, path, content)` — the join hub; calls/module_edge/token all key off `file`

Three levels = which cols are sentinel (NOT a type): committed (real slug+oid) /
working tree (WORK) / RAM-only (sentinel repo, buffer). Absence = reserved id 0,
never null.

This is v4's recovered model: `_repos`/`_revs`/`_files`/`_paths` in
`v4/src/store.rs` (RepoId=blake3(slug‖remote)[..4], RevId=blake3(repo‖oid)[..4],
FileId=content hash, PathId over (repo,rev,file,path), preinsert_sentinels).

v5 originally collapsed all four into one denormalized `_file(path, rev, hash,
mtime, size)` PK(path,rev), dropping repo (implicit `--root`) and fusing
content+location. Right shortcut to ship the call graph.

REPO AXIS RE-ADDED (main, 2026-06-01, the multi-repo arc): `_file` PK is now
(repo, path, rev) + repo col; `_prov` is (rel, repo, path, src) pruned by
(repo, path); FileMeta key (repo,path,rev); rev_index (repo,rev,path);
check_type/parse_file thread the repo slug; `refresh_builtin_rels` fills
file/rev/content per ingested repo. Old dbs migrate on open (PK change = table
rebuild, gated by column_exists). resolve_repo -> (slug, root). Two repos sharing
a path no longer collide. Lazy full-clone of an un-cloned config repo with a
`url` (RepoConfig.url + ensure_cloned). `scan("*"/"all", ...)` fans one source
rule over EVERY config repo (resolve_scan_repos) = the config-folder query.
Residuals: `_where_bytes`/`ref` + module-graph reads still self-repo only (the
deferred --move-repo-aware + module-repo-aware tasks); `rev.id` folds a shared
WORK across repos. Commits 83821c3 / dfe26ee / 60f91ec.

INVARIANT (#5, the important one): never collapse the four layers again; add
columns, never remove the content/location split.

STAGE 1 LANDED (commit 67af575, main, 2026-05-30): repo/rev/content/file are now
DSL-queryable BUILT-IN relations (reserved names; declaring one errors). Refreshed
each tick wholesale from the `_file` cache via `refresh_builtin_rels` in
engine.rs; wired into changed_source_rels on any file-set change so rules joining
`file` re-derive. Stage-1 ids = raw rev string / content hash (NO interning).
`_file` stays as change-detection cache (additive, removed nothing). Any rule can
now join the file set without scan: `holds(a,b) <- ref(a,b), file(_,"WORK",b,_)`
— PROVEN in tests/builtin_file_rel.rs that deleting b drops holds(a,b) though a
was never reparsed (FS-as-facts; the cross-codebase primitive). This UNBLOCKS the
module graph. Plan: plans/2026-05-30-data-model-migration-plan.md. STAGE 2 deferred
(content interning/short dedup ids, file-id-keyed _prov retraction, multi-repo
config — when cross-repo is real).

Context: this came out of the cross-language module-graph push. User picked
"cross-language (rs+ts)" + "add path helpers" for the first slice but stopped to
nail the data model first (rewrite-fatigue). Build step 1 (the `:ts` grammar arm
+ `files` builtin + path helpers + resolvers) sits ON these relations, not on
`_file`. See [[v5-dl-engine]] and `plans/2026-05-20-cross-language-module-graph-plan.md`.
Nested datalog confusion resolved: there is none, flat relations + joins is the point.
