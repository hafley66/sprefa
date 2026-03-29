# sprefa database schema

SQLite database at `~/.sprefa/index.db` (configurable via `[db].path` in `sprefa.toml`).
WAL mode, foreign keys enabled. Query with `sprefa sql "<SELECT ...>"`.

## Tables

### repos

One row per registered repository.

| Column | Type | Notes |
|--------|------|-------|
| id | INTEGER PK | |
| name | TEXT UNIQUE | from `sprefa add --name` or directory name |
| root_path | TEXT | absolute path on disk |
| org | TEXT | nullable, for future grouping |
| git_hash | TEXT | HEAD at last scan |
| scanned_at | TEXT | ISO 8601 timestamp |

### files

One row per unique (repo, path, content_hash) triple.

| Column | Type | Notes |
|--------|------|-------|
| id | INTEGER PK | |
| repo_id | INTEGER FK -> repos | |
| path | TEXT | repo-relative, forward slashes |
| content_hash | TEXT | xxh3_128 hex |
| stem | TEXT | filename without extension |
| ext | TEXT | file extension |
| scanner_hash | TEXT | hash of extractor config at scan time |

UNIQUE(repo_id, path, content_hash)

### strings

Deduplicated string values. Every extracted value appears exactly once.

| Column | Type | Notes |
|--------|------|-------|
| id | INTEGER PK | |
| value | TEXT UNIQUE | raw extracted string |
| norm | TEXT | lowercase(value), indexed |
| norm2 | TEXT | with suffix stripping (configurable) |

### strings_fts

FTS5 virtual table with trigram tokenizer over `strings.norm`. Synced via triggers on insert/update/delete. Supports substring search:

```sql
SELECT s.value FROM strings s
JOIN strings_fts fts ON fts.rowid = s.id
WHERE strings_fts MATCH 'widget'
```

### refs

A physical occurrence: string X at byte span [start, end) in file Y.

| Column | Type | Notes |
|--------|------|-------|
| id | INTEGER PK | |
| string_id | INTEGER FK -> strings | |
| file_id | INTEGER FK -> files | |
| span_start | INTEGER | byte offset, inclusive |
| span_end | INTEGER | byte offset, exclusive |
| is_path | INTEGER | 1 if the string is a file path |
| confidence | REAL | nullable, 0.0-1.0 |
| target_file_id | INTEGER FK -> files | resolved import target, nullable |
| ref_kind | INTEGER | legacy, unused (always 0) |
| parent_key_string_id | INTEGER FK -> strings | for key-value pairs (dep version -> dep name) |
| node_path | TEXT | structural position in AST/config tree |

UNIQUE(file_id, string_id, span_start)

### matches

Semantic interpretation of a ref. One ref can have multiple matches from different rules.

| Column | Type | Notes |
|--------|------|-------|
| id | INTEGER PK | |
| ref_id | INTEGER FK -> refs | |
| rule_name | TEXT | which extractor/rule produced this: "js", "rs", "cargo-deps", etc. |
| kind | TEXT | semantic kind: "import_path", "rs_declare", "dep_name", etc. |

UNIQUE(ref_id, rule_name, kind)

### match_labels

Arbitrary key-value metadata on matches.

| Column | Type | Notes |
|--------|------|-------|
| match_id | INTEGER FK -> matches | |
| key | TEXT | |
| value | TEXT | |

UNIQUE(match_id, key)

### branch_files

Junction table linking files to branches. A file can appear in multiple branches.

| Column | Type | Notes |
|--------|------|-------|
| repo_id | INTEGER FK -> repos | |
| branch | TEXT | "main", "main+wt", etc. |
| file_id | INTEGER FK -> files | |

UNIQUE(repo_id, branch, file_id)

The `+wt` suffix denotes working-tree state (uncommitted changes). The watcher updates `+wt` branch_files on file create/delete.

### repo_branches

Branch metadata per repo.

| Column | Type | Notes |
|--------|------|-------|
| repo_id | INTEGER FK -> repos | |
| branch | TEXT | |
| git_hash | TEXT | |
| is_working_tree | INTEGER | 1 for +wt branches |

### git_tags, repo_packages

Supporting tables for tag tracking and package ecosystem metadata. Not commonly queried directly.

## Kind values

Kinds from language extractors:

| kind | rule_name | source |
|------|-----------|--------|
| import_path | js | `import ... from './path'`, `require('./path')` |
| import_name | js | `import { Name }` |
| export_name | js | `export function Name`, `export { Name }` |
| import_alias | js | `import { Name as Alias }` |
| export_local_binding | js | local name in `export { local as exported }` |
| rs_use | rs | `use crate::module::Item` (full path as value) |
| rs_declare | rs | `pub fn name`, `pub struct Name` (bare name as value) |
| rs_mod | rs | `mod name` declaration |
| dep_name | cargo-deps | package name from Cargo.toml dependencies |
| dep_version | cargo-deps | version string, parent_key links to dep_name |
| workspace_member | cargo-workspace-members | path from workspace.members |

User-defined rules in `sprefa-rules.yaml` produce arbitrary kind strings (e.g. "helm_value", "operation_id", "path_alias", "service_name").

## Common queries

### All refs with their kinds

```sql
SELECT s.value, m.kind, m.rule_name, f.path, repos.name as repo
FROM strings s
JOIN refs r ON r.string_id = s.id
JOIN matches m ON m.ref_id = r.id
JOIN files f ON r.file_id = f.id
JOIN repos ON f.repo_id = repos.id
ORDER BY repos.name, f.path
```

### Cross-repo string overlap

Strings appearing in 2+ repos:

```sql
SELECT s.value, COUNT(DISTINCT repos.id) as repo_count, GROUP_CONCAT(DISTINCT repos.name) as repos
FROM strings s
JOIN refs r ON r.string_id = s.id
JOIN files f ON r.file_id = f.id
JOIN repos ON f.repo_id = repos.id
GROUP BY s.id
HAVING repo_count > 1
ORDER BY repo_count DESC
```

### FTS5 substring search

```sql
SELECT s.value, m.kind, f.path
FROM strings_fts fts
JOIN strings s ON fts.rowid = s.id
JOIN refs r ON r.string_id = s.id
JOIN matches m ON m.ref_id = r.id
JOIN files f ON r.file_id = f.id
WHERE strings_fts MATCH 'widget'
LIMIT 50
```

### Refs by kind

```sql
SELECT s.value, f.path, r.span_start, r.span_end
FROM matches m
JOIN refs r ON m.ref_id = r.id
JOIN strings s ON r.string_id = s.id
JOIN files f ON r.file_id = f.id
WHERE m.kind = 'rs_declare'
ORDER BY f.path, r.span_start
```

### Parent-key chains (dep version -> dep name)

```sql
SELECT
    sv.value as version,
    sn.value as name,
    f.path
FROM refs rv
JOIN strings sv ON rv.string_id = sv.id
JOIN strings sn ON rv.parent_key_string_id = sn.id
JOIN matches m ON m.ref_id = rv.id
JOIN files f ON rv.file_id = f.id
WHERE m.kind = 'dep_version'
```

### Branch file comparison

Files in working tree but not in committed branch:

```sql
SELECT f.path FROM branch_files wt
JOIN files f ON wt.file_id = f.id
WHERE wt.branch = 'main+wt'
  AND wt.file_id NOT IN (
      SELECT file_id FROM branch_files WHERE branch = 'main' AND repo_id = wt.repo_id
  )
```

### Broken imports (target file missing)

```sql
SELECT s.value as import_path, f.path as source_file, repos.name as repo
FROM refs r
JOIN strings s ON r.string_id = s.id
JOIN matches m ON m.ref_id = r.id
JOIN files f ON r.file_id = f.id
JOIN repos ON f.repo_id = repos.id
WHERE m.kind = 'import_path'
  AND r.target_file_id IS NULL
  AND r.is_path = 1
```

### Stats overview

```sql
SELECT
    (SELECT COUNT(*) FROM repos) as repos,
    (SELECT COUNT(*) FROM files) as files,
    (SELECT COUNT(*) FROM refs) as refs,
    (SELECT COUNT(*) FROM strings) as strings,
    (SELECT COUNT(*) FROM matches) as matches,
    (SELECT COUNT(DISTINCT kind) FROM matches) as kinds,
    (SELECT COUNT(DISTINCT rule_name) FROM matches) as rules
```

## Join patterns

The core join chain for most queries:

```
strings <-- refs --> files --> repos
              |
           matches --> match_labels
```

- `strings s JOIN refs r ON r.string_id = s.id` -- ref to its string value
- `refs r JOIN files f ON r.file_id = f.id` -- ref to its source file
- `refs r JOIN matches m ON m.ref_id = r.id` -- ref to its semantic interpretation
- `refs r JOIN strings pk ON r.parent_key_string_id = pk.id` -- key-value parent
- `refs r JOIN files tf ON r.target_file_id = tf.id` -- resolved import target
- `files f JOIN branch_files bf ON bf.file_id = f.id` -- branch scoping
