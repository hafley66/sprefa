# Plan: .sprf Parser + JSON Destructuring Engine

```
.sprf text ──parse──▶ SelectorChain ──lower──▶ Rule ──scan──▶ SQLite
                          │                      │
                    <<{$K:$V}>> ──parse──▶ Pattern ──match──▶ Vec<MatchResult>
                                            ▲                       │
                                     serde_json::Value ─────────────┘
```

## Context

sprefa's rule config is JSON (`sprefa-rules.json`). Building a `.sprf` DSL with CSS-style `>` selector chains and a JSON destructuring mini-lang for structured data matching. The destructuring syntax supports glob, capture, or regex at any position in the tree.

## .sprf Syntax

```sprf
# Destructuring: capture dependency keys
*/package.json > << { dependencies: { $KEY: $_ } } >> > dep_name($KEY)

# Nested: name + array iteration with ancestor carry-forward
*/data.json > << { name: $NAME, values: [...{version: $X}] } >> > pkg_version($NAME, $X)

# ast-grep for source code
**/*.ts > ast <<import { $$$NAMES } from '$SOURCE'>> > import_name($NAMES)

# Regex
helm/templates/*.yaml > re <<image:\s+(?P<REPO>[^:]+):(?P<TAG>.+)>> > image_repo($REPO)
```

Capture groups require screaming, all captures do.

Globs have bespoke capture syntax: `"main|{support/10.*:-$SUPPORT}" but branches and repos have context as they are found in the matched' path. So its less useful to capture branches, but we should allow glob capturing anything.

Selector chain: `repo_pattern > branch_patttern > fs_pattern > content_matcher > output_kind`. Separated by `>`. No keywords.

**Pattern resolution rule**: every bare string is assumed glob until proven otherwise. Quotes only exist to escape spaces (`"my project/*.json"`). `$VAR` capture syntax works everywhere -- in globs, in destructuring keys, in output slots. Regex capture groups (`(?P<NAME>...)`) coexist with `$VAR` captures. The two syntaxes live in different engines (regex vs glob/destructuring) so there's no ambiguity.

## Destructuring Mini-Lang

Operates on `serde_json::Value` (JSON/YAML/TOML all parse to this).

### Syntax elements

| Syntax | Meaning |
|--------|---------|
| `{ key: pat }` | Object: match key, descend into pat. Partial -- unlisted keys ignored. |
| `{ $KEY: $VAL }` | Object iteration: capture each key-value pair |
| `{ dep_*: $VAL }` | Glob on key name |
| `[...pat]` | Array: iterate elements, match each against pat |
| `$NAME` | Capture: bind leaf value to name |
| `$_` | Wildcard: match but don't bind |
| `lodash` | Glob (exact match here, `lodash*` would wildcard) |
| `4.*` | Glob with wildcard |
| `/^4\.\d+/` | Regex (slashes delimit, escaped slashes for literal) |
| `"my key"` | Quoted glob -- quotes only escape spaces and `>` for the parser, content is still glob-matched |

No literal mode. Every string is glob or regex. Quotes are parser grouping, not semantic -- `"foo bar"` is the glob `foo bar`. `$VAR` (SCREAMING only) = capture, `$_` = wildcard, `re:pattern` = regex (prefix required since `/` is a path character). Bare strings including `$lowercase` are just glob chars.

**Delimiter counting**: `<<` opens, `>>` closes, must count pairs. Content can contain single `>` freely.

**Whitespace**: purely cosmetic. The grammar is whitespace-insensitive. Rules terminate with `;`.

**`>>` in content**: if you need literal `>>` inside `<<...>>`, escape it via quotes. The parser greedily scans for unquoted `>>`.

**Captures**: ast-grep metavariable convention everywhere. `$VAR` = single match (like `*`), `$$$VAR` = multi match (like `**`). Works in globs (`src/$DIR/*.ts`), destructuring keys/values (`{ $KEY: $VAL }`), and ast-grep patterns. One vocabulary, three contexts. `$$VAR` reserved.

**Inherited context**: `$REPO` and `$BRANCH` flow down from matched path like CSS `currentColor` -- no explicit capture needed.

Ancestor captures carry forward through array iteration: `{ name: $N, items: [...{v: $X}] }` yields one result per array element, each containing both `$N` and `$X`.

### Pattern AST

```rust
enum Pattern {
    Object(Vec<(Pattern, Pattern)>),  // key and value both use Pattern
    Array(Box<Pattern>),
    Capture(String),          // $NAME -- single match (like *)
    MultiCapture(String),     // $$$NAME -- multi match (like **)
    Wildcard,                 // $_ -- match, don't bind
    Glob(String),             // bare string, may contain *, ?, []
    Regex(String),            // re:pattern
}
```

Key and value positions use the same `Pattern` type. `$KEY` = `Capture("KEY")`, `dep_*` = `Glob("dep_*")`, `$$$PATH` = `MultiCapture("PATH")`. `$REPO`/`$BRANCH` are inherited context, not explicit captures.

### Tree matcher (~150 lines)

```rust
fn match_pattern(value: &Value, pattern: &Pattern, captures: &HashMap<String, CapturedValue>) -> Vec<MatchResult>
```

Recursive walk of Value against Pattern. Produces `Vec<MatchResult>` (same type as walk.rs). walk.rs stays untouched for JSON-loaded rules.

## Crate structure

```
crates/sprf/
  Cargo.toml          (winnow, anyhow, serde_json, globset)
  src/
    lib.rs
    _0_ast.rs           (Statement, Slot, Pattern, KeyPattern)
    _1_parse.rs         (winnow: .sprf selector chains)
    _2_pattern.rs       (winnow: destructuring patterns inside <<>>)
    _3_match.rs         (tree matcher: Pattern × Value -> Vec<MatchResult>)
    _4_lower.rs         (SelectorChain -> Rule)
```

### `_0_ast.rs` -- types

```rust
enum Statement {
    Extract(SelectorChain),
}

struct SelectorChain {
    slots: Vec<Slot>,
}

enum Slot {
    Glob(String),
    Pattern { engine: Engine, body: String },
    Destructure(Pattern),                     // parsed <<{ ... }>>
    Output { kind: String, captures: Vec<String> },
}

enum Engine { Ast, Re }
```

### `_1_parse.rs` -- selector chain parser (~200 lines, winnow)

- `parse_program` -> `Vec<Statement>`
- `parse_chain` -> `SelectorChain` (slots separated by `>`)
- `parse_slot` -> `Slot` (dispatch on first token)
- `parse_glob` -> `String` (bare or `"quoted"`)
- `parse_delimited` -> `(Option<Engine>, String)` (engine tag + `<<...>>`)
- `parse_output` -> `(String, Vec<String>)` (`kind($CAP, ...)`)
- `#` comments, blank lines separate rules

### `_2_pattern.rs` -- destructuring parser (~120 lines, winnow)

- `parse_pattern` -> `Pattern`
- `parse_object` -> `Vec<(KeyPattern, Pattern)>` (after `{`)
- `parse_array` -> `Pattern` (after `[...`)
- `parse_key` -> `KeyPattern` (`$CAP`, `"literal"`, bare ident, glob)
- `parse_value` -> `Pattern` (recursive: object, array, capture, literal)

### `_3_match.rs` -- tree matcher (~150 lines)

- `match_pattern(value, pattern, inherited_captures) -> Vec<MatchResult>`
- Object: for each (key_pat, val_pat), find matching keys, recurse into values
- Array: for each element, recurse with inherited captures, collect all results
- Capture: bind stringified value, return single result
- Literal: check equality, return single result or empty
- KeyPattern::Capture: bind key name, recurse into value
- KeyPattern::Glob: filter keys by glob, iterate matches

### `_4_lower.rs` -- lowering (~100 lines)

`fn lower_chain(chain: &SelectorChain) -> Result<Rule>`

- `Slot::Glob` -> `SelectStep::File { pattern }`
- `Slot::Pattern { engine: Ast }` -> `AstSelector { pattern }`
- `Slot::Pattern { engine: Re }` -> `ValuePattern { source, pattern }`
- `Slot::Destructure(pat)` -> store on Rule (new optional field, or run matcher in extractor)
- `Slot::Output` -> `MatchDef { capture, kind }`

**Wiring into extractor**: add `pub destructure: Option<sprf::Pattern>` to `Rule`. In `extractor.rs`, when `destructure` is Some, call `sprf::match_pattern()` instead of `walk()`.

### Wire into CLI

- `crates/cli/src/main.rs`: `load_ruleset()` dispatches on `.sprf` extension
- `crates/cli/Cargo.toml`: add `sprefa-sprf` dep

### Tests

- Pattern parse: `{ dependencies: { $KEY: $_ } }` -> correct Pattern AST
- Pattern parse: `{ name: $N, values: [...{version: $X}] }` -> nested Pattern
- Tree match: flat object capture -> correct results
- Tree match: nested array iteration with ancestor carry-forward -> multiple results
- Tree match: glob key pattern -> filters correctly
- Selector chain parse: `*/package.json > << { deps: { $K: $_ } } >> > dep($K)`
- Lower: chain -> Rule with correct File step + destructure + MatchDef
- Round-trip: .sprf rules produce same matches as equivalent JSON rules
- Error: unclosed `<<`, unclosed `{`, missing `>`

### Files

| Action | File |
|--------|------|
| Create | `crates/sprf/Cargo.toml` |
| Create | `crates/sprf/src/lib.rs` |
| Create | `crates/sprf/src/_0_ast.rs` |
| Create | `crates/sprf/src/_1_parse.rs` |
| Create | `crates/sprf/src/_2_pattern.rs` |
| Create | `crates/sprf/src/_3_match.rs` |
| Create | `crates/sprf/src/_4_lower.rs` |
| Modify | `Cargo.toml` (workspace members += "crates/sprf") |
| Modify | `crates/rules/src/types.rs` (add `destructure` field to Rule) |
| Modify | `crates/rules/src/extractor.rs` (call match_pattern when destructure is Some) |
| Modify | `crates/cli/Cargo.toml` (add sprf dep) |
| Modify | `crates/cli/src/main.rs` (load_ruleset dispatch) |

## Verification

- `cargo check --workspace`
- `cargo test --workspace --exclude sprefa_watch`
- Write a `.sprf` file with 2-3 extraction rules, run `sprefa scan`, verify matches in DB match equivalent JSON rules

## Future

- Link rules (shared capture variable unification)
- Query subcommand
- Tree-sitter grammar for .sprf highlighting
- `:has()`, `:not()` pseudo-selectors
- `repo()`, `branch()` context selectors
- Migrate sprefa-rules.json to .sprf as default
