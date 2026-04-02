# Graph Model -- CSS-Queryable Polyglot Index

## Core idea

The sprefa index is a **typed DOM tree with graph edges**. Built-in node types (repo, branch, file, ref, match) have dedicated tables with typed columns and real indexes. User-defined node types (package, artifact, service, whatever rules produce) go in a generic table. Both implement the same `Element` trait so the CSS selector engine treats them uniformly.

Edges connect nodes across the tree (match resolves_to package, package depends_on repo). Built-in edges are foreign keys on typed tables. User-defined edges go in a generic edges table.

## Tree structure

```
root
  repo[name, path, remote_url]
    branch[name]
      file[path, ext, content_hash]
        ref[string, span_start, span_end, is_path]
          match[kind, rule_name, confidence]
    tag[name, commit_sha]
```

Parent-child is structural. A ref is always inside a file. A file is always on a branch. This never changes. CSS child combinator (`>`) and descendant combinator (` `) navigate this tree.

## Storage

### Built-in tables (typed, indexed, fast)

These exist already or are close to what exists. Each row is a node in the tree.

```sql
repos (id, name, path, remote_url)
branches (id, repo_id, name)                    -- parent: repo
files (id, branch_id, path, ext, content_hash)  -- parent: branch
strings (id, value)                              -- dedup, not a node
refs (id, file_id, string_id, span_start, span_end, is_path, parent_key_string_id, node_path)
matches (id, ref_id, rule_name, kind)            -- parent: ref
match_labels (match_id, key, value)              -- attrs on match node
tags (id, repo_id, name, commit_sha)             -- parent: repo
```

### User-defined nodes (generic, normalized)

Rules can spawn arbitrary node types. No schema changes needed.

```sql
custom_nodes (
    id INTEGER PRIMARY KEY,
    type TEXT NOT NULL,          -- "package", "artifact", "service", ...
    parent_id INTEGER,          -- nullable, points to any node (built-in or custom)
    parent_table TEXT,           -- "repos", "custom_nodes", etc. (discriminator)
    identity_hash TEXT NOT NULL, -- dedup: hash of (type + identifying attrs)
    UNIQUE(type, identity_hash)
)

custom_node_attrs (
    node_id INTEGER NOT NULL REFERENCES custom_nodes(id),
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    UNIQUE(node_id, key)
)
CREATE INDEX idx_cna_key_value ON custom_node_attrs(key, value)
```

### Edges (generic, normalized)

All cross-tree links. Both built-in and user-defined.

```sql
edges (
    id INTEGER PRIMARY KEY,
    source_table TEXT NOT NULL,   -- "matches", "custom_nodes", "refs", ...
    source_id INTEGER NOT NULL,
    target_table TEXT NOT NULL,
    target_id INTEGER NOT NULL,
    type TEXT NOT NULL,           -- "resolves_to", "depends_on", "aliases", ...
    confidence REAL DEFAULT 1.0,
    UNIQUE(source_table, source_id, target_table, target_id, type)
)

edge_attrs (
    edge_id INTEGER NOT NULL REFERENCES edges(id),
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    UNIQUE(edge_id, key)
)
CREATE INDEX idx_edges_source ON edges(source_table, source_id)
CREATE INDEX idx_edges_target ON edges(target_table, target_id)
CREATE INDEX idx_edges_type ON edges(type)
CREATE INDEX idx_ea_key_value ON edge_attrs(key, value)
```

## Element trait

The CSS engine's view of any node, regardless of backing storage.

```rust
trait Element {
    fn node_id(&self) -> NodeId;              // globally unique
    fn element_type(&self) -> &str;           // "repo", "file", "match", "package", ...
    fn parent(&self) -> Option<NodeId>;
    fn children(&self) -> ChildIter;
    fn attr(&self, key: &str) -> Option<Cow<str>>;
    fn has_attr(&self, key: &str) -> bool;
    fn edges_out(&self, edge_type: Option<&str>) -> EdgeIter;
    fn edges_in(&self, edge_type: Option<&str>) -> EdgeIter;
}
```

Built-in types implement this by reading typed columns:

```rust
impl Element for FileNode {
    fn element_type(&self) -> &str { "file" }
    fn attr(&self, key: &str) -> Option<Cow<str>> {
        match key {
            "path" => Some(Cow::Borrowed(&self.path)),
            "ext" => self.ext.as_deref().map(Cow::Borrowed),
            "content_hash" => Some(Cow::Borrowed(&self.content_hash)),
            _ => None,
        }
    }
    // ...
}
```

Custom nodes implement it by reading from `custom_node_attrs`:

```rust
impl Element for CustomNode {
    fn element_type(&self) -> &str { &self.type_name }
    fn attr(&self, key: &str) -> Option<Cow<str>> {
        self.attrs.get(key).map(|v| Cow::Borrowed(v.as_str()))
    }
    // ...
}
```

## NodeId

Global node identity across tables. Discriminated by table name + row id.

```rust
enum NodeId {
    Repo(i64),
    Branch(i64),
    File(i64),
    Ref(i64),
    Match(i64),
    Tag(i64),
    Custom(i64),
}
```

Serialized in edges as `(source_table, source_id)`. The discriminator is the table name string so new built-in types don't require schema changes to the edges table.

## CSS selector mapping

### Tree selectors (existing CSS)

| CSS | meaning | SQL |
|---|---|---|
| `repo` | all repos | `SELECT * FROM repos` |
| `repo[name="x"]` | repo with name x | `WHERE name = 'x'` |
| `repo > branch` | branches of repos | `JOIN branches ON repo_id` |
| `file[ext="ts"]` | ts files | `WHERE ext = 'ts'` |
| `repo file` | files anywhere under repo | multi-join through branch |
| `file > ref > match[kind="dep_name"]` | dep_name matches in file | join chain |
| `[path$=".test.ts"]` | suffix match | `LIKE '%.test.ts'` |
| `[name*="api"]` | substring | `LIKE '%api%'` |
| `:has(match[kind="x"])` | existence | `EXISTS (...)` |
| `:not([ext="json"])` | negation | `NOT` |

### Edge selectors (new pseudo-classes)

| CSS | meaning |
|---|---|
| `:links-to(type)` | has outgoing edge to node of type |
| `:links-to(type[attr="x"])` | ... with attr filter |
| `:linked-from(type)` | has incoming edge from node of type |
| `:via(edge_type)` | filter by edge type |
| `:links-to(package:via(depends_on))` | outgoing depends_on edge to package |

### Compound examples

```css
/* files in repos that depend on a package named "@myorg/api" */
repo:links-to(package[name="@myorg/api"]:via(depends_on)) > branch > file

/* matches that resolve to a repo containing deploy values */
match:links-to(repo:has(match[kind="deploy_value"]):via(resolves_to))

/* repos with cross-dependency: A depends on B and B depends on A */
repo:links-to(repo:linked-from(repo:is-same):via(depends_on)):via(depends_on)

/* all version strings pinned to a specific git tag */
match[kind="dep_version"]:links-to(tag[name="v1.2.3"]:via(version_pin))
```

## Rule primitives

Rules have four output clauses. All optional except emit.

### emit (existing) -- produce match nodes

```yaml
emit:
  - capture: "name"
    kind: "dep_name"
  - capture: "version"
    kind: "dep_version"
    parent: "name"
```

### link -- draw edges between nodes

```yaml
link:
  - from: "name"            # source: match node for this capture
    to_type: "package"       # target node type to search
    to_attr: "name"          # target attribute to match against
    match: "exact"           # exact | glob | semver | fuzzy | regex
    edge: "depends_on"       # edge type
    confidence: 0.9
```

`from` refers to a capture name. The source node is the match produced by emitting that capture. The engine searches for existing nodes of `to_type` where `to_attr` matches the captured value using the specified strategy.

### spawn -- create identity nodes

```yaml
spawn:
  - type: "package"
    identity: "{name}"       # dedup key (hashed for identity_hash)
    attrs:
      name: "{name}"
      registry: "npm"
    when: "no_match"         # only if link didn't find existing target
```

Spawn runs after link. If link found an existing package node, spawn is skipped (`when: "no_match"`). If no existing node matched, spawn creates one so subsequent rules can link to it. This is how the entity registry self-populates from extraction.

### crawl -- expand the graph

```yaml
crawl:
  - follow_edge: "version_pin" # which edge type to follow
    target_type: "tag"          # expected target node type
    scan_as: "tag"              # treat target's name attr as a git ref to scan
```

Crawl is the expansion trigger. After link creates a `version_pin` edge to a tag node, crawl says "scan the repo at that tag." The scanner enqueues it. When scanned, new file/ref/match nodes appear under `repo > tag > file > ...`, and the cycle can repeat.

## Evaluation pipeline

```
Pass 0: discover + extract
  - Walk repos, branches, tags → built-in tree nodes
  - Run extractors (js, rs, rule engine) → ref + match nodes
  - All emit clauses execute here

Pass 1: link + spawn
  - For each rule with link/spawn clauses:
    - Search node index for targets
    - Create edges where matches found
    - Spawn nodes where no match and when=no_match
  - Repeats until no new edges/nodes produced (fixed point within pass)

Pass 2: crawl
  - For each rule with crawl clauses:
    - Follow edges, resolve git refs
    - Enqueue scan targets
  - Execute enqueued scans → back to pass 0

Pass 3+: repeat passes 0-2 until no new crawl targets
```

Within pass 1, link rules execute in dependency order. If rule A spawns a package node and rule B links to package nodes, B must run after A (or re-run after A produces new nodes). The fixed-point loop handles this: keep running link/spawn rules until the node/edge counts stabilize.

## Logic model

The system reduces to a small set of relations and inference rules. Everything above (tables, traits, YAML config, CSS selectors, the pipeline) is implementation of these.

### Base facts (populated by extractors and discovery)

```prolog
% the world as discovered from disk
repo(Name, Path).
branch(RepoName, BranchName).
tag(RepoName, TagName, CommitSha).
file(RepoName, BranchName, FilePath, Ext, ContentHash).
ref(FileId, String, SpanStart, SpanEnd).
match(RefId, RuleName, Kind).
match_attr(MatchId, Key, Value).
```

### Derived facts (populated by link/spawn rules)

```prolog
% a named thing that exists independently of where it was mentioned
entity(Type, Identity).
entity_attr(Type, Identity, Key, Value).

% a directional relationship between two nodes
edge(SourceType, SourceId, TargetType, TargetId, EdgeKind, Confidence).
edge_attr(EdgeId, Key, Value).
```

### Rule clauses as inference rules

**emit** -- when selector matches file content, assert match facts:

```prolog
% "if file matches selector and capture binds, assert a match"
match(RefId, RuleName, Kind) :-
    file(Repo, Branch, Path, _, _),
    file_matches_selector(Path, FileSelector),
    git_matches_selector(Repo, Branch, GitSelector),
    content_matches(Path, SelectChain, Captures),
    member(emit(CaptureName, Kind), EmitList),
    capture_to_ref(Captures, CaptureName, RefId).
```

**link** -- when a match exists and a target node is findable, assert an edge:

```prolog
% "if match M has value V, and entity of type T has attr A = V, draw edge"
edge(match, MatchId, T, EntityId, EdgeKind, Conf) :-
    match(RefId, _, _),
    match_id(RefId, MatchId),
    ref_value(RefId, Value),
    link_clause(MatchKind, T, A, MatchStrategy, EdgeKind, Conf),
    entity(T, EntityId),
    entity_attr(T, EntityId, A, TargetValue),
    strategy_matches(MatchStrategy, Value, TargetValue).
```

**spawn** -- when link finds no target, create the entity:

```prolog
% "if link search found nothing, assert the entity into existence"
entity(Type, Identity) :-
    match(RefId, _, _),
    ref_value(RefId, Value),
    spawn_clause(Type, IdentityTemplate, AttrsTemplate, no_match),
    \+ entity_matches_link(Type, Value, _),    % negation: no existing target
    instantiate(IdentityTemplate, Value, Identity).
```

**crawl** -- when an edge points to a scannable ref, assert new base facts:

```prolog
% "if edge points to a tag, scan that repo at that tag"
file(Repo, TagName, Path, Ext, Hash) :-
    edge(_, _, tag, TagId, version_pin, _),
    tag(Repo, TagName, _),
    tag_id(Repo, TagName, TagId),
    scan(Repo, TagName, Path, Ext, Hash).     % side-effect: reads disk
```

### Fixed-point evaluation

The full system is a stratified Datalog program. Each pass corresponds to a stratum:

```prolog
% stratum 0: base facts (no recursion, just disk reads)
%   repo/branch/tag/file/ref/match -- from extractors

% stratum 1: link + spawn (semi-naive evaluation)
%   edge/entity -- derived from matches + link/spawn clauses
%   iterates until no new edge/entity facts produced

% stratum 2: crawl (triggers new stratum 0 facts)
%   scan targets derived from edges
%   each scan restarts from stratum 0 with new files

% global fixed point: no new crawl targets across iterations
```

Stratification matters because spawn uses negation (`\+ entity_matches_link`) which requires all link results to be computed first within the stratum. Crawl is in a higher stratum because it produces base facts (files) that link/spawn consume.

### CSS selectors as queries over the fact base

A CSS selector compiles to a conjunction of goals:

```prolog
% CSS: repo[name="org/api"] > branch[name="main"] > file[ext="ts"]
query(FilePath) :-
    repo("org/api", _),
    branch("org/api", "main"),
    file("org/api", "main", FilePath, "ts", _).

% CSS: match[kind="dep_name"]:links-to(package[name="lodash"])
query(MatchId) :-
    match(_, _, "dep_name"),
    match_id(_, MatchId),
    edge(match, MatchId, package, PkgId, _, _),
    entity_attr(package, PkgId, "name", "lodash").

% CSS: repo:has(match[kind="deploy_value"]):has(match[kind="import_path"])
query(RepoName) :-
    repo(RepoName, _),
    branch(RepoName, Branch1),
    file(RepoName, Branch1, Path1, _, _),
    ref(File1Id, _, _, _), file_id(RepoName, Branch1, Path1, File1Id),
    match(Ref1Id, _, "deploy_value"), ref_id(File1Id, Ref1Id),
    branch(RepoName, Branch2),
    file(RepoName, Branch2, Path2, _, _),
    ref(File2Id, _, _, _), file_id(RepoName, Branch2, Path2, File2Id),
    match(Ref2Id, _, "import_path"), ref_id(File2Id, Ref2Id).
```

### Composition: rules are Horn clauses, queries are goals

The whole system is:

```
facts     = what's on disk (extractors produce these)
rules     = Horn clauses with stratified negation (YAML rules compile to these)
queries   = conjunctive goals (CSS selectors compile to these)
pipeline  = semi-naive bottom-up evaluation with stratification
```

Each YAML rule compiles to one or more Horn clauses. The `emit` clause is a simple rule (selector body implies match head). The `link` clause is a join rule (match + entity search implies edge). The `spawn` clause is a negated rule (match + no entity implies new entity). The `crawl` clause is a cross-stratum trigger.

A CSS selector compiles to a query goal. The engine evaluates it top-down against the materialized fact base. Stylo's optimizations (bloom filters, rule hashing, selector specificity ordering) are execution strategies for this evaluation, not changes to the logic.

## Open questions

- How does the CSS engine handle edge traversal efficiently? Stylo optimizes tree traversal with bloom filters on ancestors. Edge traversal is a different access pattern (index lookup, not tree walk). May need a separate edge index structure.
- Should edges be directional or bidirectional? Current model is directional (source/target) with `:links-to` / `:linked-from` for both directions. Bidirectional edges would simplify some queries but complicate the mental model.
- How to handle version resolution strategies (semver, calver, git-describe) without hardcoding them? The `match: "semver"` field in link rules delegates to a strategy, but the strategies need to be extensible.
- What's the right granularity for crawl depth limits? Unbounded crawl of transitive deps could explode. Need a max-depth or budget mechanism.
- How do custom_node_attrs perform at scale vs JSONB? Normalized k/v with composite index should be fine for attribute counts per node < ~20. If nodes grow many attrs, may want a JSONB fallback column.
