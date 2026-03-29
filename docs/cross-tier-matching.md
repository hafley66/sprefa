# Cross-tier matching: surfacing probable references after a rewrite

## The two tiers

**Tier 1**: daemon watches one live folder. Language-aware import/use
resolution. Auto-rewrites with full confidence. JS/TS imports, Rust
use statements, re-exports.

**Tier 2**: the broader sprefa index across all registered repos.
String-level matching. Surfaces candidates ranked by confidence. Does
not auto-rewrite -- presents results for human review or tooling
integration.

## The intersection

When tier 1 rewrites a file move or declaration rename, it knows:
- the old file path (absolute and relative)
- the new file path
- the old module path (Rust: `crate::utils::helpers`)
- the old symbol name (if decl rename)

Tier 2 should immediately query the full index for strings that
look like they reference any of those values. The query happens
automatically after every tier 1 rewrite batch.

## What tier 2 should match on

### File path fragments

A file moves from `src/utils/helpers.rs` to `src/common/helpers.rs`.
Search for strings containing:
- `src/utils/helpers` (with or without extension)
- `utils/helpers`
- `./utils/helpers`
- `../utils/helpers`

These appear in:
- CI/CD configs: `paths: ["src/utils/**"]`, `COPY src/utils/ .`
- Docker: `ADD src/utils/helpers.rs /app/`
- Test fixtures: `read_fixture("src/utils/helpers.rs")`
- Documentation: markdown links, comments
- Build configs: Makefile targets, justfile recipes
- Package.json: `main`, `exports`, `files` fields

### Module path fragments (Rust)

Old module path `crate::utils::helpers` becomes `crate::common::helpers`.
Search for strings containing:
- `utils::helpers`
- `utils/helpers` (slash variant, common in proc macro paths)

### Symbol name (on rename)

`format_name` renamed to `format_display_name`. Search for strings
matching exactly `format_name`. These appear in:
- Config files referencing function names
- Serialized field names (`#[serde(rename = "format_name")]`)
- API schemas (OpenAPI operation IDs, GraphQL field names)
- Test names (`test_format_name`)
- Log strings, error messages
- Documentation

Symbol name matching has higher false positive rate. Rank lower
than path matches unless the string appears in a file that also
has a path-level reference to the same module.

### Route/URL path correlation

File path `src/api/users/handlers.rs` correlates with route
`/api/users`. Not a substring match -- a structural similarity
where path segments align.

Search for strings where segments overlap:
- `/api/users` (2 of 3 segments match file path)
- `api.users` (dot-separated variant)
- `api/users` (without leading slash)

This is lower confidence. Rank by segment overlap ratio.

## Confidence ranking

Tier 2 matches should be ranked, not binary. Factors:

1. **Match type**: exact path > path substring > module path >
   segment overlap > bare symbol name
2. **File context**: match found in a config file (YAML, TOML, JSON,
   Dockerfile, CI YAML) ranks higher than match in a source file
   (source files are already handled by tier 1)
3. **Co-occurrence**: if the same file contains multiple strings
   referencing the same module/path, confidence is higher
4. **Extension hint**: `.rs`, `.ts`, `.js` in the matched string
   suggests a file path reference
5. **String kind**: if the ref was extracted with `is_path = true`
   or has a node_path suggesting it is a path-like value, rank higher

## Query mechanics

The infrastructure already exists:
- `strings` table with `norm` (lowercased) and FTS5 trigram index
- `refs` table linking strings to files with spans
- `files` table with repo membership

After a tier 1 rewrite batch completes, construct search terms from
the old path/name and query:

```sql
-- path fragment search
SELECT s.value, f.path, repos.name, r.span_start, r.span_end
FROM strings_fts
JOIN strings s ON strings_fts.rowid = s.id
JOIN refs r ON r.string_id = s.id
JOIN files f ON r.file_id = f.id
JOIN repos ON f.repo_id = repos.id
WHERE strings_fts MATCH ?
  AND r.ref_kind NOT IN (10, 11, 30)  -- exclude already-rewritten import/use refs
ORDER BY rank
LIMIT 50
```

The `NOT IN` clause filters out refs that tier 1 already handled.
What remains are config values, string literals, comments, doc
strings -- the tier 2 surface.

## Output

Tier 2 results should be:
- Logged with structured fields (repo, file, span, matched string,
  confidence score, match type)
- Available via HTTP endpoint (`GET /cascade?old=...&new=...`)
- Optionally emitted as LSP diagnostics or editor notifications

The human decides whether to act on them. Tier 2 never auto-rewrites.

## Implementation order

1. **After-rewrite hook in the watch loop**: after apply() succeeds,
   collect old paths/names from EditReason, construct search terms
2. **Query builder**: generate FTS5 MATCH terms from old path
   fragments (trigram index needs >= 3 chars)
3. **Filter and rank**: exclude tier 1 ref kinds, apply confidence
   scoring
4. **Structured output**: log results, expose via HTTP
5. **Route correlation**: segment overlap matching (separate from
   substring search, needs its own scoring)

## What this does NOT cover

- Auto-rewriting config files (too risky without language-specific
  knowledge of the config format's semantics)
- Type-level references (trait implementations, generic bounds)
- Runtime string construction (`format!("src/{}/helpers", module)`)
