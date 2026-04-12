# sem SQLite schema

Source: `/Users/chrishafley/projects/ext/sem`
Cache location: `.sem/cache.db` (per-repo)
Invalidation: full nuke + rebuild if any file mtime changes or file count shifts

## Tables

```sql
CREATE TABLE files (
    path        TEXT     PRIMARY KEY,
    mtime_secs  INTEGER  NOT NULL,
    mtime_nanos INTEGER  NOT NULL
);

CREATE TABLE entities (
    id               TEXT     PRIMARY KEY,   -- "{file}::{type}::{name}"
    name             TEXT     NOT NULL,
    entity_type      TEXT     NOT NULL,       -- function, class, method, module
    file_path        TEXT     NOT NULL,
    start_line       INTEGER  NOT NULL,
    end_line         INTEGER  NOT NULL,
    content          TEXT     NOT NULL,       -- raw source text of entity
    content_hash     TEXT     NOT NULL,       -- SHA of content
    structural_hash  TEXT,                    -- AST hash ignoring comments/whitespace
    parent_id        TEXT,                    -- containment chain → entities.id
    metadata_json    TEXT
);

CREATE TABLE edges (
    from_entity  TEXT  NOT NULL,              -- entities.id
    to_entity    TEXT  NOT NULL,              -- entities.id
    ref_type     TEXT  NOT NULL               -- calls | typeref | imports
);
```

No indexes beyond PKs. No views. No UDFs.

## Production queries (all 8)

```sql
-- cache validity
SELECT COUNT(*) FROM files;

-- per-file freshness
SELECT mtime_secs, mtime_nanos FROM files WHERE path = ?1;

-- full cache load (entities)
SELECT id, name, entity_type, file_path, start_line, end_line,
       content, content_hash, structural_hash, parent_id, metadata_json
FROM entities;

-- full cache load (edges)
SELECT from_entity, to_entity, ref_type FROM edges;

-- writes
INSERT INTO files (path, mtime_secs, mtime_nanos) VALUES (?1, ?2, ?3);
INSERT INTO entities (...all 11 cols...) VALUES (?1..?11);
INSERT INTO edges (from_entity, to_entity, ref_type) VALUES (?1, ?2, ?3);

-- cache nuke
DELETE FROM files; DELETE FROM entities; DELETE FROM edges;
```

Everything loads into memory. All filtering/traversal happens in Rust, not SQL.

## Entity ID format

`{file_path}::{entity_type}::{name}`

With parent scoping for nested entities (methods inside classes, etc).

## Extraction pipeline

1. `file_paths.par_iter()` → per-file tree-sitter parse
2. Plugin dispatched by extension → `extract_entities(content, path) → Vec<SemanticEntity>`
3. Plugin walks tree-sitter AST matching `LanguageConfig::entity_node_types`
4. Pass 2: symbol table (name → entity IDs), then resolve identifiers → edges
5. Flush all to SQLite in one transaction

Languages: TS/TSX, JS, Python, Go, Rust, Java, C, C++, Ruby, C#, PHP, Fortran, Swift, Elixir, others.
