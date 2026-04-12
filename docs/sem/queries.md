# sem: useful queries

Queries to run against `.sem/cache.db`. sem loads everything into memory and filters in Rust, but these work directly against the SQLite file.

## Entity queries

```sql
-- all functions in a file
SELECT name, start_line, end_line
FROM entities
WHERE file_path = 'src/lib.rs' AND entity_type = 'function';

-- containment: what's inside what
SELECT child.name, child.entity_type, parent.name as parent_name
FROM entities child
JOIN entities parent ON child.parent_id = parent.id;

-- largest functions by line count
SELECT name, file_path, (end_line - start_line) as lines
FROM entities WHERE entity_type = 'function'
ORDER BY lines DESC LIMIT 20;

-- structural duplicates (same AST hash)
SELECT structural_hash, GROUP_CONCAT(name), COUNT(*) as n
FROM entities
WHERE structural_hash IS NOT NULL
GROUP BY structural_hash HAVING n > 1;

-- entity type distribution
SELECT entity_type, COUNT(*) as n
FROM entities GROUP BY entity_type ORDER BY n DESC;
```

## Call graph queries

```sql
-- who calls what
SELECT e1.name as caller, e2.name as callee, e1.file_path
FROM edges
JOIN entities e1 ON edges.from_entity = e1.id
JOIN entities e2 ON edges.to_entity = e2.id
WHERE edges.ref_type = 'calls';

-- callers of a specific function
SELECT DISTINCT e.name, e.file_path, e.start_line
FROM edges
JOIN entities e ON edges.from_entity = e.id
WHERE edges.to_entity LIKE '%::function::target_name';

-- fan-out: functions with most outgoing calls
SELECT e.name, e.file_path, COUNT(*) as call_count
FROM edges
JOIN entities e ON edges.from_entity = e.id
WHERE edges.ref_type = 'calls'
GROUP BY edges.from_entity
ORDER BY call_count DESC LIMIT 20;

-- fan-in: most-called functions
SELECT e.name, e.file_path, COUNT(*) as caller_count
FROM edges
JOIN entities e ON edges.to_entity = e.id
WHERE edges.ref_type = 'calls'
GROUP BY edges.to_entity
ORDER BY caller_count DESC LIMIT 20;
```

## Import graph queries

```sql
-- import edges between files
SELECT e1.file_path as from_file, e2.file_path as to_file, e2.name
FROM edges
JOIN entities e1 ON edges.from_entity = e1.id
JOIN entities e2 ON edges.to_entity = e2.id
WHERE edges.ref_type = 'imports'
GROUP BY from_file, to_file;

-- files with most imports
SELECT e.file_path, COUNT(*) as import_count
FROM edges
JOIN entities e ON edges.from_entity = e.id
WHERE edges.ref_type = 'imports'
GROUP BY e.file_path
ORDER BY import_count DESC LIMIT 20;

-- most-imported entities
SELECT e.name, e.file_path, COUNT(*) as imported_by
FROM edges
JOIN entities e ON edges.to_entity = e.id
WHERE edges.ref_type = 'imports'
GROUP BY edges.to_entity
ORDER BY imported_by DESC LIMIT 20;
```

## Cross-cutting queries

```sql
-- edge type distribution
SELECT ref_type, COUNT(*) FROM edges GROUP BY ref_type;

-- orphan entities (nothing references them, they reference nothing)
SELECT e.name, e.entity_type, e.file_path
FROM entities e
LEFT JOIN edges e_out ON e.id = e_out.from_entity
LEFT JOIN edges e_in ON e.id = e_in.to_entity
WHERE e_out.from_entity IS NULL AND e_in.to_entity IS NULL;

-- files by entity density
SELECT file_path, COUNT(*) as entity_count
FROM entities GROUP BY file_path ORDER BY entity_count DESC LIMIT 20;
```
