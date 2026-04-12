# Span-Preserving Parsing Research for LSP Diagnostics

## Problem Statement

Current extraction pipeline loses source positions:

```rust
// crates/rules/src/extractor.rs:590-612
fn parse_data(source: &[u8], path: &str) -> Option<serde_json::Value> {
    // ...
    "json" => serde_json::from_slice(source).ok(),  // ← No span info!
    "yaml" | "yml" => serde_yaml::from_slice(source).ok(),  // ← No span info!
    // ...
}
```

When LSP needs to highlight a diagnostic at `package.name` in Cargo.toml, we can only point to the rule definition in `.sprf`, not the actual location in the target file.

## Option Analysis

### Option 1: `toml_edit` (TOML-specific)

**Crate**: `toml_edit = "0.22"`

```rust
use toml_edit::Document;

let doc = source.parse::<Document>()?;
// Navigate and get spans
let package = doc["package"].as_table()?;
let name = &package["name"];
// toml_edit preserves raw string representations
```

**Pros:**
- Battle-tested (used by cargo-edit)
- Preserves formatting, comments, exact byte positions
- Can reconstruct original source

**Cons:**
- TOML only
- Different API from serde

**Status**: ✅ Recommended for TOML

---

### Option 2: `serde_spanned` (Serde-compatible)

**Crate**: `serde_spanned = "0.6"`

```rust
use serde_spanned::Spanned;
use serde::Deserialize;

#[derive(Deserialize)]
struct Package {
    name: Spanned<String>,  // Captures span with value
}

// Access: name.start, name.end, name.value
```

**Pros:**
- Works with any serde-compatible parser
- Minimal API changes

**Cons:**
- Requires serde_json to expose position info (it doesn't natively)
- Only works with structured deserialization, not Value types

**Status**: ❌ Not suitable - serde_json doesn't expose positions

---

### Option 3: `spanned_json_parser` (JSON-specific)

**Crate**: `spanned_json_parser = "0.2"`

```rust
use spanned_json_parser::parse;

let parsed = parse(json_str)?;
// Returns SpannedValue with Position { line, col } for each node
```

**Pros:**
- Purpose-built for spans
- Returns line/column positions

**Cons:**
- Small crate (38 stars), maintenance risk
- Different data model from serde_json::Value
- No YAML support

**Status**: ⚠️ Viable but risky for production

---

### Option 4: Tree-sitter (Multi-format)

**Crates**:
```toml
tree-sitter = "0.25"
tree-sitter-json = "0.24"
tree-sitter-yaml = "0.7"
tree-sitter-toml = "0.7"
```

```rust
use tree_sitter::{Parser, Node};

let mut parser = Parser::new();
parser.set_language(&tree_sitter_json::LANGUAGE.into())?;
let tree = parser.parse(source, None)?;
let root = tree.root_node();
// Walk CST with byte offsets on every node
fn visit(node: Node, source: &[u8]) {
    let start = node.start_position();
    let end = node.end_position();
    // byte_offset = node.start_byte()
}
```

**Pros:**
- Already using via ast-grep
- Full CST (Concrete Syntax Tree) with exact byte positions
- Error recovery (parses partial documents)
- Handles all three formats consistently

**Cons:**
- Heavier dependency chain
- Different paradigm (CST vs AST)
- Need to manage grammar versions

**Status**: ✅ Best overall solution

---

### Option 5: Text Search Fallback (Simplest)

```rust
fn find_value_span(source: &str, path: &[&str], expected: &str) -> Option<(usize, usize)> {
    // Naive: search for "key": "value" or key = "value"
    let pattern = format!(r#"{}\s*[:=]\s*"{}""#, path.last()?, regex::escape(expected));
    regex::find(&pattern, source).map(|m| (m.start(), m.end()))
}
```

**Pros:**
- Zero new dependencies
- Works with any format

**Cons:**
- Fragile with escaped strings
- Multiple occurrences ambiguous
- Doesn't work for nested keys with same name

**Status**: ⚠️ Acceptable as temporary fallback

---

## Recommendation

### Short-term (MVP)
Use **text search fallback** for immediate LSP diagnostic highlighting:
- Quick to implement (~50 lines)
- Good enough for 80% of cases
- Can ship now

### Medium-term (Robust)
Add **Tree-sitter** parsers for JSON/YAML/TOML:
- Consistent CST approach across formats
- Exact byte positions for all nodes
- Aligns with existing ast-grep integration

### Long-term (Specialized)
Consider **toml_edit** for TOML if we need edit capabilities:
- If LSP supports "quick fixes" that modify target files
- toml_edit preserves formatting during edits

## Implementation

### Phase 1: Text Search Fallback ✅ COMPLETE

**Module**: `crates/rules/src/span_fallback.rs`

```rust
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

pub fn find_json_path_span(source: &str, path: &[&str], value: &str) -> Option<SourceSpan>;
pub fn find_yaml_path_span(source: &str, path: &[&str], value: &str) -> Option<SourceSpan>;
pub fn find_toml_path_span(source: &str, path: &[&str], value: &str) -> Option<SourceSpan>;
pub fn find_path_span(source: &str, path: &[&str], value: &str, extension: &str) -> Option<SourceSpan>;
```

**Integration in walk.rs**:
```rust
/// Enhance walk results with approximate span information from source text.
pub fn enhance_with_spans(
    results: &mut [MatchResult],
    source: &str,
    extension: &str,
);
```

**Usage**:
```rust
use sprefa_rules::{walk, enhance_with_spans};

let mut results = walk(&json_value, &steps);
enhance_with_spans(&mut results, original_source, "json");
// Now results[i].captures[j].span_start/end have approximate positions
```

**Status**: ✅ Implemented, tested, and passing all 92+ tests

### Phase 2: Tree-sitter Integration (Next Sprint)
1. Add tree-sitter parsers to `crates/rules/Cargo.toml`
2. Create `crates/rules/src/cst_walk.rs` parallel to `walk.rs`
3. Gradually migrate from `serde_json::Value` to CST-based walking
4. Keep serde path as fallback

### Phase 3: Unified Span System
- All capture paths return `(value, span_start, span_end)`
- LSP diagnostics use exact spans from target files
- Hover shows preview with context

## Related Crates Research

| Crate | Format | Spans | Maintenance | Notes |
|-------|--------|-------|-------------|-------|
| serde_json | JSON | ❌ | Excellent | Current, no positions |
| serde_yaml | YAML | ❌ | Good | Current, no positions |
| toml | TOML | ❌ | Excellent | Current, no positions |
| toml_edit | TOML | ✅ | Excellent | Edit-preserving |
| spanned_json_parser | JSON | ✅ | Weak | 38 stars |
| tree-sitter-json | JSON | ✅ | Good | Via tree-sitter |
| tree-sitter-yaml | YAML | ✅ | Good | Via tree-sitter |
| tree-sitter-toml | TOML | ✅ | Good | Via tree-sitter |
| json-spanned-value | JSON | ✅ | Weak | Unmaintained? |

## Decision

**Go with Phase 1 (text fallback) immediately, then Phase 2 (tree-sitter) for production.**

Tree-sitter is the right long-term choice because:
1. Already in dependency tree via ast-grep
2. Consistent API across JSON/YAML/TOML
3. CST gives exact positions for every token
4. Error recovery for partial documents
