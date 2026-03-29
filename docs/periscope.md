# Periscope: immediate fuzzy reference graph on rename

## What it does

You rename or move something. Instantly, a ranked list of probable
references appears -- code snippets with file paths, ordered by
confidence, showing the surrounding context. The periscope is the
read side of the rename operation. Tier 1 auto-rewrites the certain
matches. The periscope shows you everything else.

## The graph

The strings index is a fuzzy directed graph.

**Nodes**: every normalized string in the index. An import path, a
file path, an export name, a YAML value, a JSON key, a route string,
a dep name -- all nodes in the same graph.

**Edges**: inferred relationships between nodes.

| Edge type | Direction | Confidence | Source |
|-----------|-----------|------------|--------|
| ImportPath -> file | consumer -> producer | 1.0 (resolved) | target_file_id |
| ImportName in file with ImportPath | consumer -> producer | 0.95 | co-occurrence + target_file_id |
| RsUse -> module | consumer -> producer | 1.0 (resolved at query time) | resolve_to_absolute |
| String contains file path segments | undirected | 0.3-0.8 | segment overlap |
| Same string value in different files | undirected | 0.5-0.9 | exact/norm match |
| Strings co-occurring in same file | undirected | 0.2-0.6 | file_id join |
| Config value matches source path | config -> source | 0.6-0.9 | path segment match + file kind |
| Route string matches module path | route -> handler | 0.4-0.7 | segment overlap |

Direction is known for import/use edges (ref_kind tells you). For
config/string matches, direction is inferred from file type: a YAML
file referencing a .rs path is config -> source. Two source files
sharing a string are symmetric.

## Periscope output

On rename of `helpers` in `src/utils/helpers.rs`:

```
CERTAIN (auto-rewritten by tier 1):
  src/api/handlers/mod.rs:2    use super::super::utils::helpers::sanitize
                               ->  use super::super::common::helpers::sanitize
  src/app.rs:5                 use crate::utils::helpers
                               ->  use crate::common::helpers

PROBABLE (0.7-0.9):
  deploy/k8s/api.yaml:14       source: "src/utils/helpers.rs"
  .github/workflows/ci.yml:28  paths: ["src/utils/**"]
  tests/fixtures/paths.json:3  "handler": "utils/helpers"

POSSIBLE (0.3-0.7):
  docs/architecture.md:45      The helpers module in `src/utils/`
  src/api/routes.rs:12         "/api/utils/helpers"
  scripts/generate.sh:8        HELPER_PATH="src/utils/helpers"

WEAK (0.1-0.3):
  src/common/types.rs:20       helpers    (bare name match, common word)
  README.md:102                helpers    (bare name in prose)
```

Each entry is a snippet: file path, line number, the matched string
in context (a few surrounding lines), and the confidence score. The
snippets are the periscope view -- you scan them and decide which
ones need manual attention.

## Ordering

Two modes:

**By confidence** (default): highest confidence first. You see the
most likely real references at the top, trailing off into noise.
Fast triage -- stop reading when confidence drops below your
threshold.

**By topology**: approximate dependency order. Files that import
from the renamed module come first (consumers), then files that
are peers (same directory, same module level), then config/infra
files, then documentation. Within each tier, alphabetical.

Topo ordering uses:
- ref_kind to determine consumer/producer relationship
- file path depth and shared prefix to determine peer proximity
- file extension to bucket into source/config/docs tiers
- For symmetric edges, fall back to alphabetical

True topological sort is impossible because some edges are undirected.
The ordering is a best-effort linearization of a partially directed
graph. Good enough for scanning results top to bottom.

## Snippet extraction

For each match, extract context from the source file:

1. Look up the ref's span_start and span_end in the source file
2. Expand to the enclosing line(s) -- typically 1-3 lines of context
3. Highlight the matched substring within the line
4. Include the file path and line number

The spans are already in the refs table. Reading the source file
for context is the only IO needed per match. For the periscope
response time target (< 100ms for the query, file reads can stream),
the FTS5 query returns string_ids, then a single JOIN gives spans
and file paths, then context extraction is parallel reads.

## Integration points

### CLI

```
sprefa periscope src/utils/helpers.rs
sprefa periscope --name helpers --kind RsDeclare
sprefa periscope --after-move src/utils/helpers.rs src/common/helpers.rs
```

The `--after-move` variant is what fires automatically after a tier 1
rewrite. It constructs search terms from the old path and shows the
tier 2 results.

### HTTP

```
GET /periscope?old=src/utils/helpers.rs&new=src/common/helpers.rs
GET /periscope?name=helpers&kind=RsDeclare
```

Returns JSON array of matches with confidence, file path, span,
snippet context.

### Editor integration

LSP-adjacent: after a rename operation, push periscope results as
diagnostics or a quickfix list. The editor shows them as warnings
("possible stale reference") with the code snippet inline.

### Standing periscope

A saved periscope query that re-evaluates on every index update.
"Keep watching for anything that still references the old helpers
path." Stored as a URTSL standing rule. Matches accumulate in the
matches table. Surfaces as a persistent notification until all
matches are resolved or dismissed.

## What makes this different from grep

grep finds text in files. The periscope finds typed, located,
normalized strings in a pre-built index, scores them by likelihood
of being an actual reference (not just a coincidental substring),
and presents them with enough context to make a fast yes/no decision.

grep for "helpers" returns every comment, variable name, and prose
mention. The periscope for "helpers" returns strings that were
extracted as refs (imports, exports, config values, dep names) from
specific file types, with confidence weighted by ref kind and
co-occurrence patterns.

The pre-extraction is the key. The extractors already decided "this
string is interesting enough to index." The periscope queries that
curated set, not raw file contents.

## Confidence calibration

Initial confidence scores are heuristic. Over time, calibrate from
user feedback:
- User acts on a periscope result (edits the file) -> boost that
  match pattern's confidence
- User dismisses a result -> reduce confidence for that pattern
- Track precision per ref_kind + match_type combination

This feedback loop is the LLM session artifact: the human refines
the periscope's sensitivity through use, and the refinements persist
as adjusted weights or URTSL standing rules.
