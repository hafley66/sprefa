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
