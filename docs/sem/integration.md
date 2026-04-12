# sem + sprefa integration surface

## What sem gives sprefa

| sem has | sprefa lacks | enables |
|---------|-------------|---------|
| `entities.start_line / end_line` | enclosing entity for a capture | "which function contains this config ref" |
| `entities.parent_id` | containment chain | "method inside class inside module" |
| `edges.ref_type = 'calls'` | call graph edges | fan-in / fan-out queries |
| `entities.structural_hash` | content-addressed dedup | detect cloned functions across repos |
| `entities.content` | raw source per entity | entity-scoped extraction without re-reading file |
| 15+ language support | language-aware entity boundaries | entity spans for any tree-sitter language |

## What sprefa gives sem

| sprefa has | sem lacks | enables |
|-----------|----------|---------|
| pattern-matched captures ($VAR) | only entity name/type | "what value does image.repository have" |
| cross-repo scanning | single-repo only | "which deploy config references which package.json" |
| demand scanning (repo/rev discovery) | none | "follow the tag, scan that repo at that rev" |
| per-rule SQL tables | flat entity/edge dump | structured queryable extractions |
| check blocks (invariant SQL) | none | CI-friendly violation detection |
| incremental per-file (content hash) | full nuke on any change | fast re-scan |

## Integration models

### A: sem-core as library dependency

```
sprefa scan per-file:
  1. RuleExtractor::extract(bytes, path, ctx)  → rule captures
  2. sem_plugin.extract_entities(content, path) → entities + edges
  both flush to sprefa's SQLite
```

Pro: one tree-sitter parse shared, one process, one DB.
Con: sem-core becomes a build dependency.

### B: sem as sidecar (two processes, two DBs)

```
sprefa scan → sprefa.db (rule tables)
sem index   → .sem/cache.db (entities + edges)

sprefa attaches sem DB:  ATTACH '.sem/cache.db' AS sem;
queries join across:     sem.entities JOIN deploy_image_data ...
```

Pro: zero coupling, sem evolves independently.
Con: two parses per file, two DBs, ATTACH complexity.

### C: sem as MCP tool during check blocks

```
sprefa check runs SQL → needs entity info →
  calls sem_entities MCP tool → gets JSON →
  user writes check SQL against both
```

Pro: already works today via MCP.
Con: no SQL join, check blocks can't reference sem data natively.

## Entity ID as join key

sem entity IDs follow `{file_path}::{entity_type}::{name}`.

sprefa's refs table has `file_id` + `span_start` + `span_end`.

Join predicate for "capture inside entity":
```sql
SELECT e.name, r.value
FROM sem.entities e
JOIN refs r ON r.file_id = (SELECT id FROM files WHERE path = e.file_path)
WHERE r.span_start >= e.start_byte AND r.span_end <= e.end_byte
```

Note: sem stores `start_line`/`end_line`, sprefa stores `span_start`/`span_end` (bytes). Line-to-byte conversion needed, or sem adds byte columns.

## Hypothetical .sprf rules over sem data

If sem entities were available as a sprefa base fact table:

```sprf
-- "functions that contain env var refs"
check(env_in_functions) {
  SELECT e.name as fn_name, ev.name as env_var
  FROM entities e
  JOIN kitchen_sink__env_var_ref_data ev
    ON ev.file_id = e.file_id
    AND ev.span_start BETWEEN e.start_byte AND e.end_byte
  WHERE e.entity_type = 'function'
};

-- "functions with high fan-out calling into deploy configs"
check(hot_deployers) {
  SELECT e.name, COUNT(*) as refs
  FROM entities e
  JOIN edges ON edges.from_entity = e.id
  JOIN entities callee ON edges.to_entity = callee.id
  WHERE e.entity_type = 'function'
  GROUP BY e.id HAVING refs > 10
};
```
