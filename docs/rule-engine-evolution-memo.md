# Rule Engine Evolution Memo

**Date**: 2026-03-25
**Subject**: Analyzing redundancies, chaining, and correlation in the sprefa rule capture system

---

## 1. Current Mechanics Summary

The rule engine operates as a four-stage pipeline:

**Stage 1: Selector Stack (git → file → structural)**
- `GitSelector`: optional, filters by repo name, branch, or tag (pipe-delimited glob patterns)
- `FileSelector`: required, glob(s) on file path
- `select` steps: sequential tree walk through parsed JSON/YAML/TOML data

**Stage 2: Tree Walk (`walk.rs`)**
- Depth-first walk of `serde_json::Value` tree
- Each step is a transformation: `Key`, `KeyMatch`, `Leaf`, `Object`, `Any`, `ArrayItem`, or depth filters
- Steps build up a capture map (`HashMap<String, CapturedValue>`) as the walk descends
- Returns `Vec<MatchResult>` (all matching paths)

**Stage 3: Value Transform (`emit.rs`)**
- Optional `ValuePattern`: regex with named groups applied to a single capture
- Merges named groups back into the capture map (enabling regex-based splitting)
- Example: split `"express@4.18.2"` into `name` and `version` via regex

**Stage 4: Ref Emission (`emit.rs` → `RawRef`)**
- Action's `emit` array maps captures to `RefKind`
- Each `EmitRef` specifies: capture name, kind, optional parent capture (links related refs)
- Template expansion on optional `target_repo` and `target_path` fields
- Output: `Vec<RawRef>` — the atomic refs fed to the cache layer

### Data Flow

```
file {git selector} {file selector}
    ↓
walk (select steps) → Vec<MatchResult> {captures, path}
    ↓
value pattern (regex split) → mutate captures
    ↓
action.emit (capture → kind + parent) → Vec<RawRef>
```

---

## 2. Redundancies

### 2.1 Key Step Duplication

`Key` and `KeyMatch` are nearly identical with a regex/glob difference.

**Today:**
```json
{ "step": "key", "name": "dependencies" }           // exact match
{ "step": "key_match", "pattern": "*", "capture": "name" }  // pattern match
```

**Problem**: Two steps, same semantic layer. A user writes both in the same rule.

```json
{
  "file": "**/package.json",
  "select": [
    { "step": "key_match", "pattern": "dependencies|devDependencies" },
    { "step": "key_match", "pattern": "*", "capture": "name" }
  ]
}
```

The first `KeyMatch` is a *filter* (no capture), the second is a *capture* (with capture name). They share exact execution logic except the pattern syntax.

**Redundancy**: Could unify into one step that takes either a literal string or pattern. Or split more semantically: one step for matching, one for filtering.

### 2.2 Depth Filters Are Clunky

Three separate steps for depth: `DepthMin`, `DepthMax`, `DepthEq`.

```json
{ "step": "depth_min", "n": 2 }
{ "step": "depth_max", "n": 5 }
{ "step": "depth_eq", "n": 3 }
```

**Problem**: Hard to compose. Rarely used in practice. The node_path field (already emitted) encodes depth implicitly via the path structure.

**Redundancy**: Depth is really a post-walk filter on `node_path` or a condition on capture count. Could move to a dedicated filtering step or deprecate in favor of path-based constraints.

### 2.3 Object Step Captures Siblings in Bulk

`Object` step captures multiple sibling keys at once:

```json
{ "step": "object", "captures": { "repository": "repo", "tag": "tag" } }
```

This is a convenience for "when you're at an object with these keys, grab them all." But it's also achievable via a sequence of `Key` steps:

```json
{ "step": "key", "name": "repository", "capture": "repo" },
{ "step": "key", "name": "tag", "capture": "tag" }
```

**Redundancy**: `Object` is syntactic sugar. The same effect is possible (though verbose) with sequential steps. Test `walk_object_step_captures_siblings` shows both approaches work.

However, `Object` is semantically richer: it fails the entire match if *any* capture is missing. Sequential steps would succeed even if a key is absent (the missing capture just doesn't appear in the map).

**Verdict**: Not pure redundancy, but a missing constraint mechanism. See Section 5 for unification proposal.

### 2.4 Value Pattern Is a Special Case of Extraction

The `value` pattern (regex with named groups) does extraction at the boundary of walk + emit.

```json
{
  "value": {
    "source": "raw",
    "pattern": "(?P<name>[^@]+)@(?P<version>.+)",
    "full_match": true
  }
}
```

This is a *second-pass* capture, applied *after* the walk but *before* emit. It's orthogonal to the walk steps — the walk captures `raw`, then the value pattern splits it.

**Redundancy**: This is a different mechanism than `KeyMatch` capture, but it does the same job in a different phase. A rule author might be tempted to:
1. Use a regex in a walk step (doesn't exist), or
2. Use `value` pattern (does exist, but only at the emit phase)

The walk engine is coarse-grained (navigate tree structure); value refinement happens later. This is actually sound architecture but exposes a gap: you can't *select* based on a regex until you're extracting.

### 2.5 Parent Linking via parent_key String

The `parent` field in `EmitRef` links a version back to a dep name:

```json
{
  "capture": "version",
  "kind": "dep_version",
  "parent": "name"
}
```

This creates `parent_key_string_id` in the database — a foreign key to another string.

**Redundancy**: This is the only mechanism for linking related refs. It's powerful but flat: a ref has at most one parent (by string name). Complex relationships (e.g., "this version belongs to this dep in this manifest file") require multiple refs + the same parent string, which the index then correlates.

The `node_path` field (stored in every ref) already encodes the structural relationship. Why use a separate parent_key mechanism?

**Answer**: parent_key is explicit semantic linking; node_path is positional. They serve different purposes in queries: node_path reconstructs the tree (for anti-unification), parent_key marks domain relationships (dep version → dep name). But there's redundancy in *how* the relationship is expressed.

---

## 3. Chained Rules

**Today**: Rules are independent. Each rule walks the entire file and emits refs.

**Limitation**: No cross-rule composition. Example:

1. Rule A: extract all `package.json` deps
   ```json
   {
     "name": "package-deps",
     "file": "**/package.json",
     "select": [
       { "step": "key_match", "pattern": "dependencies|devDependencies" },
       { "step": "key_match", "pattern": "*", "capture": "name" },
       { "step": "leaf", "capture": "version" }
     ],
     "action": { "emit": [{ "capture": "name", "kind": "dep_name" }] }
   }
   ```

2. Rule B (hypothetical): extract which file declares each dep
   ```json
   {
     "name": "dep-sources",
     "requires": ["package-deps"],  // <-- new field: depends on output of Rule A
     "file": "**/package.json",
     "input": "package-deps",  // <-- new: use results from Rule A
     "select": [
       { "step": "key_match", "pattern": "dependencies|devDependencies" },
       { "step": "key_match", "pattern": "*", "capture": "name" }
     ],
     "action": {
       "emit": [
         {
           "capture": "name",
           "kind": "dep_source_file",
           "correlation": "package-deps"  // <-- link to Rule A output
         }
       ]
     }
   }
   ```

**Use Cases for Chaining**:

1. **Multi-stage refinement**: Extract version strings, then parse semver, then tag as pre-release.
   ```
   Rule 1: capture "version" from package.json → emit all versions
   Rule 2: input: rule-1 results, apply semver regex, emit "prerelease" kind only
   ```

2. **Cross-file correlation**: Find all configs, then for each, load referenced env files.
   ```
   Rule 1: extract ENV_FILE names from configs
   Rule 2: input: rule-1 names, match against actual .env files, emit file exists status
   ```

3. **Dependency tracking with transitive insight**: Extract direct deps, then cross-reference lock files.
   ```
   Rule 1: extract package.json deps
   Rule 2: input: rule-1 dep names, query lock file for transitive deps
   ```

**Technical Proposal: `requires` + `input` fields**

```rust
pub struct Rule {
    pub name: String,

    // existing
    pub git: Option<GitSelector>,
    pub file: FileSelector,
    pub select: Option<Vec<StructStep>>,
    pub action: Action,

    // new
    /// Prerequisite rule(s) that must run first
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires: Option<Vec<String>>,  // rule names

    /// How to use results from a prerequisite rule
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<RuleInput>,
}

pub struct RuleInput {
    /// Which rule's output to consume
    pub from_rule: String,

    /// What field from that rule's emitted refs to use as input
    /// e.g. "value" (the dep name) or "parent_key" (semantic link)
    pub field: String,

    /// Optional filter: only process refs where kind == this value
    pub filter_kind: Option<ActionKind>,

    /// How to pass the input value into this rule's walk
    /// "search" = literal string match in walk
    /// "filter" = only matches with this value proceed
    pub mode: InputMode,
}

pub enum InputMode {
    Search,   // find this value in the current file
    Filter,   // only emit if the value matches
}
```

**Execution Model**:
1. Topological sort rules by `requires` dependency.
2. Run rules in order, accumulating results in an in-memory map: `HashMap<String, Vec<RawRef>>`.
3. When processing a rule with `input`, pass the prerequisite rule's output as a constraint to the walk engine.
4. Walk engine can short-circuit or filter based on the input.

**Example (chained extraction)**:

```json
{
  "name": "pnpm-lock-versions",
  "file": "**/pnpm-lock.yaml",
  "select": [
    { "step": "key", "name": "packages" },
    { "step": "key_match", "pattern": "*", "capture": "pkg_key" }
  ],
  "value": {
    "source": "pkg_key",
    "pattern": "(?P<name>[^@]+)@(?P<version>.+)",
    "full_match": true
  },
  "action": { "emit": [{ "capture": "version", "kind": "dep_version" }] }
}
```

Then a second rule consumes it:

```json
{
  "name": "pnpm-prerelease-check",
  "requires": ["pnpm-lock-versions"],
  "file": "**/pnpm-lock.yaml",
  "input": {
    "from_rule": "pnpm-lock-versions",
    "field": "value",
    "mode": "filter"
  },
  "select": [
    { "step": "key", "name": "packages" },
    { "step": "key_match", "pattern": "*", "capture": "pkg_key" }
  ],
  "value": {
    "source": "pkg_key",
    "pattern": "(?P<name>[^@]+)@(?P<version>.*(?:alpha|beta|rc).+)",
    "full_match": true
  },
  "action": { "emit": [{ "capture": "version", "kind": "dep_version" }] }
}
```

This second rule only processes versions that matched the first rule and also match the prerelease pattern. Reduces noise in a two-pass extraction.

---

## 4. Correlation: Beyond parent_key

**Today**: `parent_key` links a ref to another via shared string name. This is transitive only at query time (the schema has `parent_key_string_id`).

**Limitations**:
1. **One-to-one only**: A ref has one parent string. Complex relationships need multiple refs.
2. **No explicit type**: The query engine doesn't know *why* a version is linked to a dep name. It's convention.
3. **No cross-file correlation**: parent_key is local to a file's parse tree. Can't express "this dep in manifest A links to this package in lock file B."
4. **No rule-to-rule causality**: Refs don't record which rule emitted them. Can't trace back or debug.

### Proposal: Enhanced Correlation Model

**Field 1: Rule name on RawRef**

Add to `RawRef`:
```rust
pub rule_name: String,  // which rule emitted this ref
```

Benefits:
- Query-time debugging: "which rule extracted this?"
- Post-processing: "all refs from rule X that match condition Y"
- Audit trail for anti-unification: know the source rule when inferring new ones

**Field 2: Correlation groups**

Add to `RawRef`:
```rust
pub correlation_group: Option<String>,  // e.g. "npm-deps-pkg-json-0"
```

Usage:
- Within a single rule invocation on a single file, refs from the same walk match share a group ID
- Query: "all refs in correlation group X" = "all parts of the same extracted entity"
- Example: dep name + version + dev flag all have the same group ID

**Field 3: Typed parent links**

Change `parent_key` to a structured type:

```rust
pub parent_links: Vec<ParentLink>,

pub struct ParentLink {
    pub parent_string_id: i64,
    pub kind: ParentKindHint,  // Dependency, Export, Semantic, StructuralPath
}

pub enum ParentKindHint {
    Dependency,       // "this version belongs to this dep"
    ExportMapping,    // "this implementation satisfies this export"
    Semantic,         // catch-all, user-defined intent
    StructuralPath,   // rebuild tree traversal path
}
```

Benefits:
- Explicit semantics: not just "parent key string" but *what kind* of relationship
- Enables smarter queries: "find all versions whose parent is a dep_name"
- Reduces ambiguity in multi-parent scenarios

### Proposal: Cross-Rule Correlation

Add optional field to `EmitRef`:

```rust
pub struct EmitRef {
    pub capture: String,
    pub kind: ActionKind,
    pub parent: Option<String>,

    // new
    /// Name of another rule whose output should be correlated with this
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlate_rule: Option<String>,

    /// How to correlate: by string value match, by regex, by time order
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlate_mode: Option<CorrelateMode>,
}

pub enum CorrelateMode {
    /// Exact match on the emit ref's value with another rule's value
    ValueMatch,

    /// Join on a common parent key (e.g. both have parent_key = "express")
    ParentKeyMatch,

    /// Temporal: this ref appears after that rule's output on same branch
    After,

    /// Spatial: same file, same node_path prefix
    SameBranch,
}
```

Example: Link package.json dep to package-lock.json resolved entry:

```json
{
  "name": "npm-deps",
  "file": "**/package.json",
  "select": [...],
  "action": {
    "emit": [{
      "capture": "name",
      "kind": "dep_name",
      "correlate_rule": "npm-lock-resolved",
      "correlate_mode": "value_match"
    }]
  }
}
```

At emit time, the extractor would look up `npm-lock-resolved` refs with the same value and create linkage in the output.

---

## 5. Simplification Proposals

### 5.1: Unify Key and KeyMatch

**Today**: Two steps, one pattern. Merge into one.

```json
// Before
{ "step": "key_match", "pattern": "dependencies", "capture": "dep_type" }

// After: single "key" step accepts either literal or pattern
{ "step": "key", "pattern": "dependencies", "capture": "dep_type" }

// Or, be explicit about match style:
{ "step": "key", "mode": "literal", "value": "dependencies" }
{ "step": "key", "mode": "pattern", "value": "dependencies", "capture": "dep_type" }
```

**Upside**: One mental model. Fewer code paths.
**Downside**: Harder to distinguish intent. What's the default? Introduce mode field to clarify.

**Recommendation**: Introduce a `match_mode` enum field (default: `literal`). This clarifies intent without splitting steps.

```rust
pub enum KeyMatchMode {
    Literal,   // exact string match
    Pattern,   // glob pattern
}

pub struct KeyStep {
    pub value: String,  // either literal or pattern
    pub mode: Option<KeyMatchMode>,  // default: Literal
    pub capture: Option<String>,
}
```

### 5.2: Retire Depth Filters

Move depth constraints to a post-walk filter or deprecate.

**Why**: They're rarely used. The `node_path` field is more expressive and composable.

**Instead**, add a `filter` step:

```rust
pub enum StructStep {
    // ... existing ...

    /// Filter match results by a condition on the accumulated state
    Filter {
        #[serde(rename = "type")]
        filter_type: FilterType,
        value: Option<String>,
    },
}

pub enum FilterType {
    DepthMin(u32),
    DepthMax(u32),
    NodePathMatches(String),  // glob on node_path
    CaptureExists(String),    // only matches where this capture was populated
}
```

Then depth constraints become:

```json
{ "step": "filter", "type": "depth_min", "value": 2 }
{ "step": "filter", "type": "node_path_matches", "value": "dependencies/*" }
```

**Benefit**: Explicit filtering, reusable abstraction, can add new filters without new steps.

### 5.3: Strengthen Object Step with Required Capture

Today, `Object` fails silently if a key is missing. Make this explicit:

```rust
pub struct ObjectStep {
    pub captures: BTreeMap<String, String>,

    /// If true, require all captures to exist; if false, missing keys are silently skipped
    #[serde(default)]
    pub required: bool,
}
```

Usage:

```json
{
  "step": "object",
  "captures": { "repository": "repo", "tag": "tag" },
  "required": true  // fail the match if either is missing
}
```

**Benefit**: Explicit constraint. Rules are clearer about what's optional vs required.

### 5.4: Unify Value Pattern and AST Selector

Today:
- `select` + `value` handle data files (JSON, YAML, TOML)
- `select_ast` handles source code (via ast-grep)

Both emit captures → action → refs. But they're separate fields, and `select_ast` is not yet implemented.

**Proposal**: Merge into one selector concept with a `parse_mode` field:

```rust
pub enum ParseMode {
    Structural,  // JSON, YAML, TOML tree walk (today's default)
    Ast,         // ast-grep pattern matching (future)
    Regex,       // full-file regex (new option)
}

pub struct Rule {
    pub name: String,
    pub git: Option<GitSelector>,
    pub file: FileSelector,

    // Unified: either structured walk OR AST pattern, not both
    pub parse_mode: ParseMode,  // default: Structural

    pub selector: Option<Selector>,  // can be steps, ast pattern, or regex

    pub action: Action,
}

pub enum Selector {
    Structural(Vec<StructStep>),
    Ast(AstSelector),
    Regex(RegexSelector),
}
```

**Benefit**: One conceptual model. Rules don't have to think about two different selector syntaxes.
**Cost**: Larger enum, but clearer mental map.

### 5.5: Name Captures Explicitly in the Action

Today, a capture must exist in the walk map or emit silently skips it. No way to express "this ref is required."

**Proposal**: Action schema clarifies intent:

```rust
pub struct EmitRef {
    pub capture: String,
    pub kind: ActionKind,
    pub parent: Option<String>,

    /// If false, missing capture is skipped (today's behavior)
    /// If true, missing capture causes the entire action to fail
    #[serde(default)]
    pub required: bool,
}
```

Usage:

```json
{
  "action": {
    "emit": [
      { "capture": "name", "kind": "dep_name", "required": true },
      { "capture": "version", "kind": "dep_version", "required": false }
    ]
  }
}
```

**Benefit**: Explicit contracts. Author knows if a ref is optional or mandatory.

---

## 6. Wild Ideas

### 6.1: Declarative Rule Composition via DSL

Instead of JSON, offer a compact DSL for rules:

```
rule npm-deps {
  file: **/package.json

  select:
    @dependencies.* -> name
    value -> version

  emit:
    name as dep_name
    version as dep_version (parent: name)
}
```

Advantages:
- Shorter syntax for common patterns
- Visual hierarchy (indentation) mirrors tree structure
- Easier to read complex rules

Could compile this DSL to the JSON schema. Let users write .rules files instead of JSON.

**Effort**: Medium. Needs a parser, but the JSON schema is the compile target.

### 6.2: Computed Captures via Transform Pipes

Add inline transformations to captures, like jq or URTSL pipes:

```json
{
  "select": [
    { "step": "key", "name": "version", "capture": "raw_version" }
  ],
  "value": {
    "source": "raw_version",
    "transforms": [
      "lowercase",
      "strip(/^v/)",
      "split(/-/)[0]"  // first segment before dash
    ]
  },
  "action": {
    "emit": [{
      "capture": "raw_version | lowercase | strip(/^v/)",
      "kind": "dep_version"
    }]
  }
}
```

The pipe syntax in capture name means: apply these transforms to the captured value before emit.

**Benefit**: Reduces number of rules. One rule can express multi-step normalization without chaining.
**Cost**: Parsing, validation, performance (more ops per capture).

### 6.3: Pattern Library & Rule Inheritance

Rules are functions; compose them via inheritance:

```json
{
  "bases": ["npm-deps-common"],
  "name": "npm-workspace-deps",
  "file": "**/package.json",

  "select-override": [
    { "step": "key", "name": "workspaces" },
    { "step": "array_item" },
    { "step": "key", "name": "dependencies" },
    { "step": "key_match", "pattern": "*", "capture": "name" }
  ],

  "action-override": {
    "emit": [{
      "capture": "name",
      "kind": "workspace_dep",
      "parent": null
    }]
  }
}
```

The `npm-deps-common` base rule defines the core walk. The derived rule overrides specific steps and actions.

**Benefit**: DRY for similar rules. Reduces duplication in large rule sets.
**Cost**: Complexity. Need to resolve overrides, handle conflicts.

---

## Summary Table

| Redundancy | Severity | Proposal |
|------------|----------|----------|
| Key + KeyMatch duplication | Medium | Unify with `match_mode` field |
| Depth filters underused | Low | Replace with Filter step or deprecate |
| Object step optional captures | Low | Add `required` field for clarity |
| Value pattern orthogonal | Low | Keep but clarify semantics in docs |
| parent_key single link | Medium | Add correlation_group, rule_name, ParentLink types |
| Rules isolated, no chaining | High | Add `requires` + `input` fields, topological sort |
| select + select_ast duplication | High | Unify under `Selector` enum with ParseMode |
| Implicit capture requirements | Low | Add `required` field to EmitRef |

### Quick Wins (Low effort, high clarity)

1. Add `match_mode` to Key step to unify key/key_match intent
2. Add `required` field to Object and EmitRef for explicit constraints
3. Add `rule_name` field to RawRef for debugging and traceability
4. Add `correlation_group` to track sets of related refs from one walk match

### Medium Effort (Unlock new capabilities)

1. Implement rule chaining via `requires` + `input` fields
2. Merge `select_ast` into unified Selector enum with ParseMode
3. Add Filter step to retire depth filters and enable new conditions

### Future / Experimental

1. DSL syntax for rule authoring (jq-like, compact)
2. Computed captures with transform pipes
3. Rule inheritance and pattern library system

