# Data-model migration — plan (2026-05-30)

Implement the pinned contract in `v5/docs/data-model.md`: make file identity
queryable as `repo` / `rev` / `content` / `file` relations, replacing the
DSL-invisible collapsed `_file` table as the *model* (not as the cache). This is
the gate in front of the cross-codebase module graph: `module_edge(a,b)` must be
able to join a candidate target against the real file set (`file(b, ...)`), which
no rule can do today.

## Why staged

The full contract has two independent pieces:
1. **structure** — four queryable relations, content separate from location.
2. **interning** — short content-derived ids + cross-repo/cross-path dedup (the
   RSS optimization; v4's `intern_*`).

Cross-codebase needs (1), not (2). (2) is an optimization that the rejected-DD
discussion says not to over-build early. So:

- **Stage 1 (this plan): structure, additive.** Create the four relations as
  built-in, DSL-queryable, populated each tick from the existing reconcile.
  Content-id = the content hash itself (no dedup yet, but the `content` relation
  EXISTS, so the content/location split is real and Stage 2 is a value change,
  not a schema change). `_file` stays as the change-detection cache untouched.
- **Stage 2 (deferred): interning.** Replace content-id with a short interned id
  (dedup identical content across paths/revs), key `_prov` retraction by file-id
  instead of path, multi-repo config. A separate plan when cross-repo is real.

Stage 1 removes nothing, so it cannot regress the LSP/ban/digest work; the risky
rip-outs all live in Stage 2.

---

## Layer 1 — target relations (built-in, reserved names)

```
rel repo(id: text, slug: text, root: path).
rel rev(id: text, repo: text, oid: text, ts: int).
rel content(id: text, hash: text).
rel file(repo: text, rev: text, path: path, content: text).   // content = content-id
```

Reserved: a `.dl` program may NOT declare these names (error if it tries). They
are registered into `self.rels` at tick start with these fixed columns, so
`lower_rule` can join body atoms like `file(_, "WORK", p, _)` against them.

Stage-1 simplification: `content.id == content.hash` (no interning). `repo` has
one row from `--root` (id = blake3(slug‖root)[..8], slug = root dir name).
`rev` rows are the distinct revs seen in `_file` (WORK + any git oids).

## Layer 2 — population (pseudo, end of reconcile, both tick and tick_paths)

```rust
// fn refresh_builtin_rels(&self) -> Result<()>
//   // file: one row per _file row; content-id = hash
//   DELETE FROM rel_file; DELETE FROM rel_content; DELETE FROM rel_rev; DELETE FROM rel_repo
//   repo_id = blake3(slug ‖ root)[..8]
//   INSERT rel_repo(repo_id, basename(root), root)
//   for (path, rev, hash, _, _) in SELECT * FROM _file:
//     INSERT OR IGNORE rel_content(hash, hash)              // id == hash, stage 1
//     INSERT OR IGNORE rel_rev(rev_id(rev), repo_id, rev, ts_of(rev))
//     INSERT rel_file(repo_id, rev, path, hash)
//   // rev_id(WORK) = "WORK"; rev_id(sha) = blake3(repo ‖ sha)[..8]
```

These are plain tables (`rel_file`, etc.), rebuilt from `_file` each tick. They
are derived-from-cache, so they wipe-and-repopulate like any derived relation;
cheap because `_file` is one row per tracked file.

`refresh_builtin_rels` runs after `reconcile_sources` (cold tick) and after the
extract loop in `tick_paths`, before queries. It does NOT participate in
`affected_derived` (it is a leaf the user's rules read, never writes).

## Layer 3 — lifetimes

| holds state | lifetime | where |
|---|---|---|
| `_file` | unchanged (change-detection cache) | SQLite, internal |
| `rel_repo/rev/content/file` | rebuilt each tick from `_file` | SQLite tables |
| reserved-name registration | per tick, in `self.rels` | RAM |
| repo identity (`--root`) | per process | `Engine.root` |

No interning state (Stage 2). No new durable structure beyond the four tables.

## Layer 4 — storage, reads/writes, uniqueness

Storage: four `rel_<name>` tables created in `declare`-time as built-ins
(reserved), schemas per Layer 1.

Uniqueness:
- `repo` PK(id); one row in Stage 1.
- `rev` PK(id); WORK + distinct git oids.
- `content` PK(id=hash); deduped by hash via INSERT OR IGNORE.
- `file` PK(repo, rev, path); one row per location.

Reads/writes per tick: `refresh_builtin_rels` wipes + repopulates the four from
`_file` (a full scan of `_file`, which is one row per tracked file — bounded by
repo size, not fact count). User rules read them as join inputs.

Collision rule: if a `.dl` declares `repo|rev|content|file`, error with
"`<name>` is a built-in relation". Checked in `declare_all`.

Interaction with `affected_derived`: the four are sources-from-cache; treat them
as always-fresh leaves. A file edit already changes `_file`, so the refresh picks
it up; derived rules that read `file` rebuild via the existing change path
(their body references `file`, a changed source rel). NOTE: wire `file` into the
changed-source set when `_file` changed, so a rule joining `file` re-derives.

## Stage 1 acceptance / test (tests/builtin_file_rel.rs)

```
rel hit(path: file).
hit(p) <- file(_, "WORK", p, _).
? hit(p).
```
- Assert `? hit` returns every scanned working-tree path (join against the
  built-in `file`, no `scan` in the rule).
- Assert a `.dl` declaring `rel file(...)` errors with the built-in message.
- Cross-file-ref shape (the real unblock): a rule
  `holds(a,b) <- ref(a,b), file(_, "WORK", b, _).` drops `holds` rows whose `b`
  is not a real file. Prove by deleting `b` and re-ticking (`--changed`): the
  `holds` row for the missing file disappears.

## Phases

| phase | deliverable |
|---|---|
| 1a | reserve+register the four rels; `refresh_builtin_rels`; populate from `_file`; wire `file` into changed-source set; collision error |
| 1b | `repo` from `--root` (single); `rev` rows; `content(id=hash)` |
| 1c | tests/builtin_file_rel.rs (queryable file, collision error, cross-file-ref drop) |
| 2 (later) | content interning (short ids, dedup); file-id-keyed `_prov`; multi-repo config |

1a → 1b → 1c sequential. This unblocks the cross-codebase module graph: with a
queryable `file`, `module_edge(src,dst) <- import_raw(src, spec), resolve(spec,
dst), file(_, _, dst, _).` becomes expressible, then `closure(module_edge)` gives
cross-language cycles/fan-in/fan-out for free.
