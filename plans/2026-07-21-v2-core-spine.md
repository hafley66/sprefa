# v2 core spine: repo / rev / file / occurrence dimension layer

Motivation, measured: the root db is **865 MB** against a **~24 MB** corpus (~36x).
**60% is indexes.** The dataflow family alone is **248 MB / ~30%** of the file, and
it is one 297k-node set materialized four times. The root cause is NOT the sym
names (the dense `_sym_dict` arc already took −17.4% off those). It is that
**repo / rev / file / coordinate have no dimension tables** — every fact family
re-stamps them as interned-string sids on every row, and indexes each one.

This is the v2 spine: five small dimension tables every family references by a
narrow integer, so a fact row carries ids, not re-interned coordinates.

## 0. What exists today (the anti-pattern, confirmed 2026-07-21)

- `_strings(id, content)` — the ONE clean surrogate. Keep.
- `_df_node_dict(id, file, line, col, kind)` — the ONE real dimension table
  (dataflow only). The model to generalize.
- `_repo(slug TEXT PK, root, url)` — **0 rows, unused**. `_ref`, `_rev_log` —
  empty skeletons. rev/repo were never wired in.
- `_file` + `_files` — two competing file models, both TEXT-keyed, no int id.
- Every fact (`_call_def`, `_call_raw_site`, `df_node_*`, `scip_occurrence`,
  `scip_binding`, `_where_bytes`, …) carries `repo_sid`, `rev_sid`, `file_sid`
  as separate `REFERENCES _strings(id)` columns + their indexes. There are
  **1 repo and a handful of revs** in the whole db; their identity is stamped on
  ~1.5M fact rows and indexed.

## 1. Type signatures (the five spine tables)

```sql
-- one row per repository. tiny.
repo(id INTEGER PRIMARY KEY, slug TEXT NOT NULL UNIQUE, url TEXT NOT NULL DEFAULT '')

-- one row per (repo, git rev). tiny (< a few hundred ever).
-- id is a 1-2 byte varint the whole db references instead of a 40-char hash sid.
rev(id INTEGER PRIMARY KEY, repo_id INTEGER NOT NULL REFERENCES repo(id),
    hash TEXT NOT NULL, UNIQUE(repo_id, hash))

-- one row per (repo, path). content_hash for change detection.
file(id INTEGER PRIMARY KEY, repo_id INTEGER NOT NULL REFERENCES repo(id),
     path_sid INTEGER NOT NULL REFERENCES _strings(id),
     content_hash_sid INTEGER NOT NULL REFERENCES _strings(id),
     UNIQUE(repo_id, path_sid))

-- THE shared coordinate spine. every family that today stores (file,line,col
-- [,end_line,end_col]) references ONE occurrence id instead. this is the
-- cross-family dedup: df_node, scip_occurrence, call_site, type_entity, member,
-- where_bytes all collapse their coordinate columns to occ_id.
occ(id INTEGER PRIMARY KEY, file_id INTEGER NOT NULL REFERENCES file(id),
    line INTEGER NOT NULL, col INTEGER NOT NULL,
    end_line INTEGER NOT NULL DEFAULT -1, end_col INTEGER NOT NULL DEFAULT -1,
    UNIQUE(file_id, line, col, end_line, end_col))

-- unchanged: the text dictionary. names/kinds still intern here.
_strings(id INTEGER PRIMARY KEY, content TEXT NOT NULL)
_sym_dict(id INTEGER PRIMARY KEY, sym_hash INTEGER NOT NULL UNIQUE)  -- from the 0.12.0 arc
```

## 2. Semantics / how a fact family collapses onto it

Today (call def, one row):
```
_call_def(sym_sid, name_sid, repo_sid, kind_sid, file_sid, line, end, rev_sid)
          = 8 columns, of which repo_sid/file_sid/rev_sid re-encode the spine
```
v2:
```
call_def(sym, name, kind, occ_id, rev_id)
         -- occ_id folds (file,line,end); rev_id folds (repo,rev) to a 1-2B int
```

The four `df_node` projections collapse to ONE:
```
-- today: rel_df_node + rel_df_node_rev + rel_df_node_repo_rev + rel_df_node_in_fn
--        = 4 copies of 297k nodes, each with PK autoindex + secondaries (~150 MB)
-- v2:
df_node(id, occ_id, kind, var_sym, fn_sym)   -- one table; occ_id carries file/line/col
df_node_rev(node_id, rev_id)                 -- thin bridge, only if rev-history is needed
-- _rev / _repo_rev / _in_fn become VIEWs or are dropped (df_arg is ALREADY a VIEW)
```

`scip` stops being a parallel coordinate universe: `scip_occurrence(occ_id,
symbol, role, rev_id)` — its `file/line/col/end_line/end_col` (5 cols × 283k
rows + PK autoindex) become one `occ_id`. NOTE (measured): scip and df_node share
only 9% of coordinates, so scip does NOT replace df flow — but both reference the
SAME `occ` table, so identical coordinates are stored once.

## 3. Instance lifetimes

- `repo`, `rev`, `file` — dimension tables. Loaded once per connection into an
  in-memory `hash→id` allocator (mirror `Db::sym_alloc` from the 0.12.0 arc:
  single-writer, lazy-loaded, persisted at flush). Handful to low-thousands of
  rows. Effectively static within a tick.
- `occ` — grows with distinct source coordinates (~280k). Same allocator pattern
  keyed on (file_id, line, col, end_line, end_col). This is `_df_node_dict`
  generalized to every family.
- fact families — per-tick, reference the above by id. Unchanged reactive
  lifetime.

## 4. Storage layout → reads → writes → uniqueness

- Layout: dimension tables are WITHOUT ROWID on their natural UNIQUE key OR plain
  rowid + UNIQUE index; benchmark both (the index-audit arc showed WITHOUT ROWID
  flips fixpoint join sides). Fact families keep their PKs but on `(…, occ_id,
  rev_id)` instead of `(…, file_sid, line, col, repo_sid, rev_sid)`.
- Reads: a query that filters by rev joins `rev` once (id lookup), then filters
  facts on the 1-2B `rev_id` — instead of matching a 5-byte sid on every row.
  Coordinate rendering joins `occ → file → _strings(path)` once per output row.
- Writes: extraction resolves (repo, rev, file, coord) → ids through the
  allocators BEFORE encoding fact rows (same seam as `dense_of_hash`), so a fact
  row is written with ids already dense. One flush per dimension table per tick.
- Uniqueness: `rev` UNIQUE(repo_id, hash); `file` UNIQUE(repo_id, path_sid);
  `occ` UNIQUE(file_id, line, col, end_line, end_col). Bijection-gate each like
  `sym_dict_bijection`: distinct id == distinct natural key per table.

## 5. Projected reclaim (to be measured, not asserted)

Mechanism, not a promise:
- `rev_id`: 1 repo + a few revs → a 1-byte varint replaces a ~5-byte rev sid on
  ~1.5M fact rows across families, plus every `idx_*_rev` shrinks. This is the
  single biggest column-level win because rev has near-zero cardinality yet is
  stamped everywhere.
- `occ_id`: collapses (file,line,col[,end,end]) — today 3-5 int columns per row
  in df_node, scip_occurrence, call_site, type_entity, member — to one id, and
  dedups the ~24k coordinates shared across families.
- df_node projection collapse (4→1 + views): ~150 MB, the largest single reclaim,
  independent of the spine but enabled by it (`occ_id` gives the rev-less and
  in-fn projections a cheap key).

Order of work (each measured A/B like the 0.12.0 cut, each its own arc):
1. `rev` dimension (highest ratio, lowest risk — near-constant cardinality).
2. `df_node` projection collapse to 1 + VIEWs.
3. `occ` shared coordinate spine across all families.
4. `repo`/`file` normalization + retire `_file`/`_files` duplication.

## Build-vs-buy note

This is engine-core schema (the one legitimately bespoke layer per the standing
law), so no library substitutes for the dimension tables themselves. Where a
standard already fits, use it: SCIP is the bought symbol-xref format and stays
the source for `scip_occurrence` (it just references `occ`/`rev` like everyone
else); it does NOT and cannot supply value-flow edges (`df_edge`), so df stays
bespoke. The allocator pattern is already in-tree (`Db::sym_alloc`, 0.12.0) —
reuse it, do not rebuild it per dimension.
