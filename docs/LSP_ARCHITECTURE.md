# LSP Architecture: Visual Plan

## The Problem: Parser Fails on Incomplete Syntax

```
USER TYPING                    PARSER SEES
───────────                    ───────────

rule(test) {                   rule(test) {
  fs(**/car                       fs(**/car
       ▲                               ║
       │                               ║
    cursor                        UNTERMINATED!
    here                          NO SEMICOLON!
                                  NO CLOSING PAREN!
                                       ║
                                       ▼
                                PARSE ERROR
                                ═══════════
                                No AST returned
                                No completions
                                User sad
```

## Solution: Three-Layer Hybrid Parser

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         INPUT TEXT                                      │
│  "rule(test) { fs(**/car"                                              │
│                              ▲                                          │
│                              │                                          │
│                           CURSOR                                        │
└──────────────────────────────┬──────────────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ LAYER 1: FULL PARSE                                                     │
│                                                                         │
│   parse_program(text)                                                   │
│          │                                                              │
│          ├──► SUCCESS ──► Use AST for completions                       │
│          │                   (happy path)                               │
│          │                                                              │
│          └──► FAIL ─────► Continue to Layer 2                           │
│                           (most common for partial syntax)              │
└──────────────────────────────┬──────────────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ LAYER 2: ERROR-RECOVERY PARSE                                           │
│                                                                         │
│   parse_recovery(text[..cursor])                                        │
│                                                                         │
│   Parse only up to cursor position                                      │
│   Mark error nodes but continue                                         │
│                                                                         │
│          │                                                              │
│          ├──► Partial AST ──► Check if errors after cursor              │
│          │                      │                                       │
│          │                      ├──► YES ──► Use partial AST            │
│          │                      │                                          │
│          │                      └──► NO ───► Continue to Layer 3        │
│          │                                                              │
│          └──► Complete fail ─► Continue to Layer 3                      │
└──────────────────────────────┬──────────────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────────────┐
│ LAYER 3: HEURISTIC TOKEN SCAN                                           │
│                                                                         │
│   Scan backwards from cursor:                                           │
│                                                                         │
│   Position: rule(test) { fs(**/car|                                     │
│                                    ▲                                    │
│                                    │                                    │
│   Step 1: "**/car"  ← partial pattern                                   │
│   Step 2: "("       ← arg list open                                     │
│   Step 3: "fs"      ← tag identifier                                    │
│   Step 4: "{"       ← block open (context)                              │
│                                                                         │
│   Result: INSIDE fs() WITH PARTIAL="**/car"                             │
│                                                                         │
│   ► Trigger file completions with pattern "**/car"                      │
└─────────────────────────────────────────────────────────────────────────┘
```

## Why This Architecture Wins

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   FULL PARSE    │    │ ERROR RECOVERY  │    │    HEURISTICS   │
├─────────────────┤    ├─────────────────┤    ├─────────────────┤
│                 │    │                 │    │                 │
│ Fast for        │    │ Handles most    │    │ Always works    │
│ valid syntax    │    │ partial cases   │    │ no matter what  │
│                 │    │                 │    │                 │
│ 100% accurate   │    │ 90% accurate    │    │ 70% accurate    │
│                 │    │                 │    │                 │
│ Fails on        │    │ Fails on        │    │ Never fails     │
│ broken syntax   │    │ complex broken  │    │                 │
│                 │    │                 │    │                 │
│ Example:        │    │ Example:        │    │ Example:        │
│ fs("**/x")      │    │ fs("**/x"       │    │ fs(**/x|        │
│ ► Works         │    │ ► Works         │    │ ► Works         │
│                 │    │                 │    │                 │
└─────────────────┘    └─────────────────┘    └─────────────────┘
        │                      │                      │
        └──────────────────────┼──────────────────────┘
                               │
                               ▼
                    ┌─────────────────────┐
                    │   COMBINED RESULT   │
                    │                     │
                    │ • Fast when fast    │
                    │ • Robust when messy │
                    │ • User always gets  │
                    │   completions       │
                    └─────────────────────┘
```

## Token Scan Heuristics Detail

```
PATTERN MATCHING TABLE
═══════════════════════════════════════════════════════════════════════

Pattern Detected              Context                        Action
───────────────────────────────────────────────────────────────────────
tag( [partial]                Inside tag args               Complete based
                                                              on tag type

repo( [name] ) { ...          Inside repo block             Use that repo's
rev( [partial]                                                git refs

tag( " [partial]              Inside quoted string          Strip quotes,
                                                              complete

rule( [name] ) {              Inside rule body              Complete captures,
[partial]                                                     tags, refs

$ [partial]                   Capture variable start        Complete captures
                                                              in scope

> [partial]                   After chain operator          Complete selectors
═══════════════════════════════════════════════════════════════════════
```

## Part 2: Hover + SQLite Architecture

```
HOVER REQUEST
═════════════

    User hovers over $NAME
            │
            ▼
┌───────────────────────┐
│ 1. AST LOOKUP         │
│                       │
│ Find token at cursor  │
│ position              │
│                       │
│ Result: $NAME         │
└───────────┬───────────┘
            │
            ▼
┌───────────────────────┐
│ 2. FIND PARENT RULE   │
│                       │
│ Walk up AST from      │
│ $NAME node            │
│                       │
│ Result: package_name  │
└───────────┬───────────┘
            │
            ▼
┌───────────────────────┐     ┌──────────────────────────────────────────┐
│ 3. SQL BUILDER        │────►│                                          │
│                       │     │  SELECT s.value                          │
│ Map:                  │     │  FROM strings s                          │
│   rule: package_name  │     │  JOIN package_name__data d               │
│   var:  NAME          │     │    ON s.id = d.name_ref                  │
│                       │     │  LIMIT 50                                │
│ Table: package_name__data                                   │
│ Column: name_ref                                                │
│                       │     │                                          │
└───────────┬───────────┘     └──────────────────────────────────────────┘
            │                              │
            │                              │
            ▼                              │
┌───────────────────────┐                  │
│ 4. SQLITE QUERY       │──────────────────┘
│    ~/.sprefa/index.db │
│                       │
│ Execute SQL           │
│ Fetch results         │
│                       │
│ Result: [values]      │
└───────────┬───────────┘
            │
            ▼
┌───────────────────────┐
│ 5. FORMAT MARKDOWN    │
│                       │
│ **Capture: NAME**     │
│ Rule: package_name    │
│                       │
│ Values (47 total):    │
│ • sprefa_sprf         │
│ • sprefa_config       │
│ • ...                 │
└───────────────────────┘
```

## Database Schema to SQL Mapping

```
RULE DEFINITION                    DATABASE TABLES
─────────────────────────────────────────────────────────────────

rule(package_name) {           ┌─────────────────────────┐
  fs(**/Cargo.toml) >          │  package_name__data    │
  json({                       ├─────────────────────────┤
    package: {                 │  id          INTEGER  │
      name: $NAME              │  package_ref INTEGER  │
    }                          │  name_ref    INTEGER ─┼──► strings.id
  })                           │  repo_ref    INTEGER  │
};                             │  rev_ref     INTEGER  │
                               └─────────────────────────┘
                                      │
                                      │ FOREIGN KEY
                                      ▼
                               ┌─────────────────────────┐
                               │  strings               │
                               ├─────────────────────────┤
                               │  id    INTEGER         │
                               │  value TEXT            │
                               │  hash  BLOB            │
                               └─────────────────────────┘

COLUMN NAMING CONVENTION
═════════════════════════════════════════════════════════════════
Capture    Column Name
─────────────────────────────────────────────────────────────────
$NAME      name_ref
$REPO      repo_ref  
$TAG       tag_ref
$DEP       dep_ref
$VERSION   version_ref
═════════════════════════════════════════════════════════════════
```

## Combined LSP State

```
┌─────────────────────────────────────────────────────────────────┐
│                    LSP SERVER STATE                              │
│                                                                  │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐  │
│  │   DOC STATE     │  │   WORKSPACE     │  │    DATABASE     │  │
│  │                 │  │                 │  │                 │  │
│  │ • Parsed AST    │  │ • sprefa.toml   │  │ • SQLite conn   │  │
│  │ • Partial AST   │  │ • Repos list    │  │ • Query cache   │  │
│  │ • Token stream  │  │ • Sources       │  │                 │  │
│  │                 │  │                 │  │                 │  │
│  │ For:            │  │ For:            │  │ For:            │  │
│  │ completions,    │  │ completions     │  │ hover values    │  │
│  │ hover context   │  │ (repo, paths)   │  │                 │  │
│  └─────────────────┘  └─────────────────┘  └─────────────────┘  │
│           │                    │                    │            │
│           └────────────────────┼────────────────────┘            │
│                                │                                 │
│                                ▼                                 │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │              REQUEST ROUTER                              │    │
│  │                                                          │    │
│  │  textDocument/completion ──► doc + workspace ──► items   │    │
│  │                                                          │    │
│  │  textDocument/hover      ──► doc + database ───► markdown│    │
│  │                                                          │    │
│  └─────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
```

## File Structure

```
crates/sprf-lsp/src/
├── main.rs          # LSP server, request routing
├── lib.rs           # Public exports
├── context.rs       # Token scanner, heuristic detection  [NEW]
├── workspace.rs     # Config loading, repo discovery      [NEW]
├── completion.rs    # Completion providers (rg/git)       [NEW]
├── db.rs            # SQLite connection, query builder    [NEW]
└── hover.rs         # Hover provider, markdown fmt        [NEW]
```

## Implementation Order

```
PHASE 1: FOUNDATION
────────────────────────────────────────────────────────────────
1. workspace.rs     Load sprefa.toml, discover repos
2. context.rs       Token scanner, pattern detection
3. Modify main.rs   Wire up workspace + context

PHASE 2: COMPLETIONS
────────────────────────────────────────────────────────────────
4. completion.rs    File/repo/ref completion providers
5. Modify main.rs   Add completion handler with 3-layer parser

PHASE 3: HOVER
────────────────────────────────────────────────────────────────
6. db.rs            SQLite connection
7. hover.rs         Hover provider
8. Modify main.rs   Add hover handler
9. Cargo.toml       Add rusqlite dependency
```

## Why This Design

| Aspect | Our Approach | Alternative | Why Ours Wins |
|--------|--------------|-------------|---------------|
| **Parser** | 3-layer hybrid | Single parser | Handles broken syntax |
| **File discovery** | Shell to rg | Build cache | Simpler, always fresh |
| **DB for hover** | SQLite query | In-memory | Persistent, fast |
| **Namespace** | Auto-detect | Manual config | Zero config for user |
| **Git refs** | Current repo only | All repos | Focused, less noise |
```


## Appendix: Future Ideas & Unsolved Problems

### JSON Path Completion from Live Files

**The Idea:**
Use the JSON matcher to enumerate all JSON paths from actual files in the workspace, then use those paths to suggest completions for JSON/YAML/TOML patterns.

**Example:**
```
User types:                    Suggested completions:
json({ name: $                json({ name: $NAME })     ← from package.json
                              json({ name: $N })         ← from Cargo.toml
                              json({ name: $title })     ← from some.yaml
```

**The Challenge:**
JSON Path distinguishes between key-position matching and value-position matching:
- `$.name` matches the key `"name"`
- `$.name` also matches the value at that key
- But capturing is value-oriented: `json({ name: $CAP })` captures the value

**Unsolved Questions:**
1. How do we capture keys vs values distinctly?
2. Should `$KEY` capture the key name and `$VAL` capture the value?
3. What about nested paths like `dependencies.$PKG.$VERSION`?
4. How do we handle arrays: `scripts[$INDEX]`?

**Implementation Sketch:**
```rust
// 1. Enumerate all paths from a sample of JSON files
fn enumerate_json_paths(file: &Path) -> Vec<JsonPath> {
    // Walk the JSON tree, build paths like:
    // - $.name
    // - $.dependencies.lodash
    // - $.scripts.build
}

// 2. Suggest completions based on common patterns
fn suggest_json_patterns(paths: &[JsonPath]) -> Vec<CompletionItem> {
    // Group by similarity, suggest patterns like:
    // "json({ name: $NAME })"
    // "json({ dependencies: { $PKG: $VERSION } })"
}
```

**Status:** Interesting but unsolved. The capture semantics for key vs value positions need design work.

---

### Simpler Approach: Declarative JSON Path → Pattern

**Pragmatic Design:**
Skip the key/value capture complexity. Just:
1. Enumerate JSON paths from real files
2. Convert paths to declarative patterns
3. **Auto-place capture at value position**

**Example:**
```
Discovered path:          Suggested pattern:
$.name                    json({ name: $NAME })
$.version                 json({ version: $VERSION })
$.dependencies.lodash     json({ dependencies: { lodash: $LODASH } })
$.scripts.build           json({ scripts: { build: $BUILD } })
```

**Key insight:** The capture is *always* at the leaf value. User can rename `$NAME` to whatever.

**Implementation:**
```rust
fn path_to_pattern(path: &JsonPath) -> String {
    // $.dependencies.lodash → json({ dependencies: { lodash: $LODASH } })
    let parts = path.segments();
    let leaf = parts.last().unwrap().to_uppercase();
    build_nested_json(parts, &format!("${}", leaf))
}
```

**Trade-off:** Can't match on dynamic keys (e.g., "find all dependency names"), but covers 80% of use cases.

**Next Steps:**
- [ ] Enumerate paths from `package.json`, `Cargo.toml`
- [ ] Build frequency map (common paths = higher priority)
- [ ] Suggest top 10 patterns as completions

---

### TOML/YAML/JSON Unified Completion

**The Idea:**
Extend the JSON matcher concept to TOML and YAML with unified syntax:
```
# All three match the same conceptual pattern
toml([package] name = $NAME)
yaml(package: name: $NAME)
json({ name: $NAME })
```

**Challenge:** Different nesting syntax (tables vs objects) makes unification tricky.

**Status:** Would be fun to implement after JSON path completion is solved.

