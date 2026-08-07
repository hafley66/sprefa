---
name: sql-relational-design
description: The relational design laws for every table this repo emits or hand-writes — surrogate integer keys, dictionary encoding for natural keys, no composite TEXT PKs, no stringly-typed values. Read BEFORE designing any schema, rel declaration lowering, DDL emission, or storage layout, in v5, v6, or any lab.
---

# SQL relational design laws (user-set 2026-08-07, after the second interning incident)

## The law
Every stored relation keys on INTEGER surrogate ids. A natural key (path,
name, any composite of TEXT columns) is stored ONCE, in a dictionary/entity
table with `UNIQUE` on the natural key; every other table carries the id.

```sql
-- WRONG (what got emitted twice, v5 and v6):
CREATE TABLE flow_reach (
  from_path TEXT, from_name TEXT, to_path TEXT, to_name TEXT,
  PRIMARY KEY (from_path, from_name, to_path, to_name)) WITHOUT ROWID;

-- RIGHT:
CREATE TABLE sym  (id INTEGER PRIMARY KEY, path TEXT NOT NULL,
                   name TEXT NOT NULL, UNIQUE (path, name));
CREATE TABLE flow_reach (from_sym INTEGER NOT NULL, to_sym INTEGER NOT NULL,
                         PRIMARY KEY (from_sym, to_sym)) WITHOUT ROWID;
```

## Why (measured in this repo, 2026-08-07)
- Identical 4-column WITHOUT ROWID table: INTEGER keys insert 1.7-2.0x faster
  than TEXT at every volume 4k-1M rows (labs/exec_shootout/intern_bench/
  REPORT-INTERN.md section 3).
- Every index is a full copy of its key. A fat key is copied into EVERY
  btree that mentions it (table PK, delta tables, frontier tables, their
  indexes: 5+ copies per row in the emitted engine).
- WITHOUT ROWID stores the full key in interior btree nodes too: fat keys
  shrink fanout, deepen the tree, and every probe compares the shared prefix
  of path-like strings byte by byte.
- Interning is cheap where it belongs, at the boundary: 7.5M edges/sec, 0.06-
  4.2% of total; materializing ids back to TEXT for output is 1:29 against
  the insert it feeds (same report, sections 2 and 4).

## The rest of day-1
- Atomic columns only (1NF): no JSON-in-a-TEXT-column pretending to be a key,
  no comma-joined lists in key positions.
- Booleans are INTEGER 0/1, never 'true'/'FalsE'/text.
- Ids never leak meaning: no parsing an id, no ranges-with-semantics.
- Human-readable output is a JOIN or view at the read boundary, never fat
  columns in the hot tables.
- The dictionary is append-only within a run; UNIQUE constraint is its dedup.

## Enforcement
- A composite TEXT PRIMARY KEY in emitted DDL or hand DDL is a DEFECT, not a
  style choice. Flag it in review; do not land it.
- When lowering a rel whose declared columns are TEXT, the lowering owes a
  dictionary + id plan, or an explicit waiver naming why the table is cold.
- Sibling skill: sqlite-costs (the measured constants behind these laws).
