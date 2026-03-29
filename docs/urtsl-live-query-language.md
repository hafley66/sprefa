# URTSL: live query language over the normalized string index

## Core premise

Every extracted string -- import path, file path, export name, dep
version, YAML value, JSON key, route decorator, Rust use path,
serde rename, env var -- enters the same normalized strings table.
At the storage layer there is no ontological difference between
`crate::utils::helpers` and `/api/utils/helpers` and
`src/utils/helpers.rs` and `"utils-helpers"`. They are all strings
with a source location, a ref kind tag, and a normalization.

Matching is approximate by default. Exact matching is a special
case of approximate matching with confidence = 1.0.

The index is a live tree: the daemon keeps it current with the
filesystem. Git operations rebuild it. The strings table is a
point-in-time snapshot of every interesting token in every file
across every registered repo.

## What URTSL is

A pattern language for querying this string space. Designed to be:

1. **Writable by humans** for simple cases (`find "helpers"` across
   all repos)
2. **Generatable by LLMs** for complex cases (a session produces a
   set of standing patterns that fire on future index changes)
3. **Composable** -- patterns combine with set operations
4. **Confidence-aware** -- every match carries a score
5. **Time-aware** -- patterns can reference git history (when did
   this string first appear, across which branches)

Not a regex language. Not SQL. A focused vocabulary for expressing
"things that look like this thing" with tunable precision.

## The vocabulary

### Atoms

```
"helpers"                     -- exact normalized substring
stem("helpers")               -- file stem match (strips ext, path)
path("src/utils/helpers")     -- path fragment (slash-aware segmenting)
mod("crate::utils::helpers")  -- module path (::‐aware segmenting)
seg("utils", "helpers")       -- ordered segment match (any separator)
```

Atoms match against the `norm` column (lowercased, trimmed). Each
atom produces a set of (string_id, confidence) pairs.

### Confidence modifiers

```
exact("helpers")              -- confidence 1.0, no fuzzy
fuzzy("helpers", 0.8)         -- edit distance threshold
near("helpers", "utils")      -- both segments within N positions
```

### Filters (narrow the result set)

```
in_repo("my-app")            -- restrict to repo
in_file("*.yaml")            -- glob on file path
of_kind(ImportPath)           -- restrict to ref kind
of_kind(!ImportPath, !RsUse)  -- exclude ref kinds (tier 1 refs)
on_branch("main")            -- restrict to branch
depth_min(2)                  -- file path depth >= 2
```

### Set operations

```
A & B                         -- intersection (both match)
A | B                         -- union
A - B                         -- difference (A but not B)
A >> B                        -- cascade: matches of A, then for each,
                                 find co-occurring B in the same file
```

### Time operations

```
since("2025-01-01")          -- string appeared after date
before("v2.0")               -- string appeared before git tag
added_with("other_string")   -- appeared in the same commit
```

### Confidence aggregation

When atoms combine, confidence multiplies (intersection) or takes
max (union). Filters don't affect confidence. Time operations can
boost confidence (a string that appeared in the same commit as
another is more likely to be related).

## Example queries

### "What else references the file I just moved?"

```
path("src/utils/helpers") - of_kind(ImportPath, RsUse)
```

Finds all strings containing the old path fragment, excluding the
import/use refs that tier 1 already rewrote. What remains: config
files, test fixtures, CI paths, documentation.

### "What routes correlate with this module?"

```
seg("api", "users") & in_file("*.rs") & of_kind(RsUse, RsDeclare)
  >> seg("api", "users") & in_file("*.yaml", "*.toml")
```

Find Rust code referencing api/users, then cascade to find config
files that also reference those segments. The cascade boosts
confidence when both code and config mention the same path.

### "What renamed across repos?"

```
exact("OldName") & since("2025-03-01") & of_kind(ExportName)
  >> exact("NewName") & of_kind(ExportName)
```

Find files where OldName was an export after March 1, then check
if the same file now exports NewName. Detects renames across the
index even without watching the live filesystem.

### "Standing pattern: alert on stale references"

```
path("src/utils/helpers") & of_kind(!ImportPath, !RsUse)
  | mod("crate::utils::helpers") & of_kind(!RsUse)
```

This pattern is stored as a rule. Whenever the index changes and
a match fires, emit an alert: "this string still references the
old path/module that was renamed."

## LLM session -> standing patterns

The intended workflow:

1. Human describes what they want to track: "I renamed the auth
   module, find everything that still references the old name"
2. LLM generates a URTSL query, runs it, shows results
3. Human refines: "exclude test files", "also check the deploy repo"
4. LLM updates the query, re-runs
5. When satisfied, human says "save this as a standing rule"
6. The query is stored in the `rules` table with a URTSL expression
7. On every index update, standing rules re-evaluate and emit
   matches to the `matches` table

The LLM's job is to translate intent into patterns and tighten them
based on false positive feedback. The patterns are the artifact, not
the LLM session itself.

## Implementation layers

### Layer 0: already exists
- strings table with norm, FTS5 trigram index
- refs table with ref_kind, file_id, spans
- files table with repo membership, path, branch

### Layer 1: query parser
- Parse URTSL expressions into an AST
- Atoms -> FTS5 MATCH or LIKE queries on strings.norm
- Filters -> WHERE clauses on refs/files/repos
- Set operations -> INTERSECT / UNION / EXCEPT
- Confidence scoring as a post-filter

### Layer 2: time integration
- Join against git_tags, repo_branches for time predicates
- git log integration for added_with (same-commit correlation)

### Layer 3: standing rules
- rules table already has name, selector, ref_kind, rule_hash
- selector column stores the URTSL expression
- matches table links rule_id -> ref_id
- After each index update, re-evaluate dirty rules

### Layer 4: cascade execution
- The >> operator runs the RHS query scoped to files where LHS matched
- Confidence aggregation across cascade steps

## Why not just regex

Regex operates on raw text. URTSL operates on the pre-extracted,
normalized, typed, located string index. The difference:

- Regex: scan every file, character by character, no type info
- URTSL: query pre-indexed strings with ref kind, file location,
  repo membership, branch, git time, confidence scoring

A URTSL atom like `path("utils/helpers")` is not `grep -r utils/helpers`.
It queries pre-segmented path strings in the normalized index, scores
by segment overlap, filters by ref kind, and returns results with
source locations and confidence. The FTS5 trigram index makes
substring matching fast without scanning files.

The LLM generates URTSL patterns, not regex. The patterns are
readable, composable, and carry semantic intent (path vs module vs
symbol) that regex cannot express.

## Relationship to the rules engine

The existing rules engine (sprefa-rules.json/yaml) defines extraction
patterns: what to pull out of files during scanning. URTSL defines
query patterns: what to find in the already-extracted index.

Extraction rules produce refs. URTSL queries consume refs.

They share the rules/matches tables. An extraction rule says "when
you see this AST pattern, create a ref." A URTSL standing rule says
"when the index contains strings matching this pattern, create a
match." Both are stored, versioned, and incrementally evaluated.
