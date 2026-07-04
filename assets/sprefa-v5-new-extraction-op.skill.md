---
name: sprefa-v5-new-extraction-op
description: Checklist for adding a new body-item extraction op (like match/ast/sg/json) to the v5 dl engine.
---

# Adding a new extraction op to v5

Canonical reference: the `ast_yaml` op (ast-grep relational YAML rules, a
superset of `sg`'s pattern-only form). It is the most recent GENUINELY NEW
`BodyItem` variant. The earlier json/yaml/toml work made the existing `json`/
`jsonp` ops format-polymorphic inside `datapath.rs` by dispatching on file
extension instead of adding a new op (that path is step 5's alternative,
below). `ast_yaml`'s evaluator lives in `src/sg.rs` (`run_ast_yaml`), mirroring
`sg`; the pattern for a new separate op is `sg.rs` (structural) or
`datapath.rs` (data-format).

`engine.rs` no longer exists as one file (the 2026-06-30 engine breakdown):
the engine is `src/engine/mod.rs` (~5600+ lines, still houses `parse_file`,
the per-op evaluation arms, and the `*_RELS` reserved-name arrays) plus
`src/engine/tick.rs` and `src/engine/extract.rs` (the always-run type/call/
dataflow/doc extraction refreshers). Run `cargo build`/`cargo test` from the
**repo root**: v5 was lifted to the repo root 2026-07-01, there is no `v5/`
subdir to `cd` into or `--path` at.

---

## Checklist

### 1. Add a `BodyItem` variant

`src/ast.rs`, the `BodyItem` enum (starts ~line 242). Add a variant, e.g.
(mirroring the real `AstYaml`):

```rust
AstYaml { path: Term, rev: Term, lang: String, yaml: String, line: Term,
          col: Term, end_line: Term, end_col: Term },
```

Also add to the `is_source()` match (~line 382):

```rust
| BodyItem::AstYaml { .. }
```

### 2. Add a parse arm

`src/parse.rs`, the `body_item` dispatch (~line 434). Add a line to the
`if s == "..."` ladder:

```rust
if s == "ast_yaml" { return self.ast_yaml(); }
```

Then add the `fn ast_yaml(&mut self) -> Result<BodyItem>` method, mirroring
`fn sg` (~line 654) for a structural op or `fn json` (~line 774) for a
data-format op.

### 3. Add a typecheck arm

`src/typecheck.rs`, `normalize_body_item`. Add a match arm normalizing every
`Term` field:

```rust
BodyItem::AstYaml { path, rev, line, col, end_line, end_col, .. } => {
    for term in [path, rev, line, col, end_line, end_col] {
        normalize_term(term, dl_path, diags);
    }
}
```

### 4. Add an evaluation arm in `parse_file`

`src/engine/mod.rs`, `fn parse_file` (~line 5511). Add a `BodyItem::AstYaml
{ .. }` arm. The binds-expansion pattern every op uses:

```rust
BodyItem::AstYaml { lang, yaml, line, col, end_line, end_col, .. } => {
    let line_var = opt_var(line)?;
    let col_var = opt_var(col)?;
    let end_line_var = opt_var(end_line)?;
    let end_col_var = opt_var(end_col)?;
    let hits = crate::sg::run_ast_yaml(&content, lang, yaml)?;
    let mut next_binds: Vec<Bind> = Vec::new();
    for existing_bind in &binds {
        for (hit_line, hit_col, hit_end_line, hit_end_col, _lo, _hi, captures) in &hits {
            let mut extended = existing_bind.clone();
            if let Some(v) = &line_var { extended.insert(v.clone(), Value::Int(*hit_line)); }
            if let Some(v) = &col_var { extended.insert(v.clone(), Value::Int(*hit_col)); }
            if let Some(v) = &end_line_var { extended.insert(v.clone(), Value::Int(*hit_end_line)); }
            if let Some(v) = &end_col_var { extended.insert(v.clone(), Value::Int(*hit_end_col)); }
            bind_captures(&mut extended, captures, &mut where_bytes);
            next_binds.push(extended);
        }
    }
    binds = next_binds;
}
```

Key points:
- Iterate `&binds` (existing bindings), produce `next_binds` by crossing with
  each hit: the binds-expansion pattern every op uses.
- Call `bind_captures`/`push_span`/`bind_span_id` (whichever fits your op's
  output shape) so every captured string value writes byte coordinates into
  the ref spine (`_where_bytes`). This is what makes a match a rewritable
  coordinate, not just a string.
- Byte offsets (`lo`/`hi`) index into `content`, not line numbers.

### 5. Write the evaluator

If the op parses a genuinely new grammar, put the logic in its own module
(structural/CST-shaped: alongside `sg.rs`; register a new file as
`pub mod myop;` in `src/lib.rs`), or extend `sg.rs` if it is `sg`/`ast_yaml`-
shaped (a pattern/rule matcher over an existing tree-sitter grammar already
wired for `ast`/`sg`).

If the op is a new DATA FORMAT lookup (dotted-path / brace-pattern style),
`datapath.rs` already dispatches `json`/`jsonp` across json/yaml/toml by file
extension (`fmt_of`); add a new format there instead of a new `BodyItem`.

### 6. Tree-sitter grammar (if a new format/language)

Add the grammar dep to `Cargo.toml`. Proven pairs on tree-sitter 0.25:

```toml
tree-sitter-yaml = "0.7"
tree-sitter-toml-ng = "0.7"
```

Use CST walking, not serde: values must carry byte spans and serde cannot
recover them. For quoted scalars, return the inner span with quotes stripped;
see `datapath.rs`'s `yaml_scalar_text` / `toml_unquote` for the offset
adjustment pattern.

### 7. Update docs and add a test

- Regenerate the op table: `dl examples/gen-reference.dl --root .` (writes
  `docs/reference/syntax.md` + the `README.md` op table; never hand-edit
  those spliced blocks).
- Add an e2e test to `tests/it/data_ops.rs` (registered in `tests/it/main.rs`,
  the single integration harness) following the existing `sandbox`/`run`/
  `prog` pattern (see the `sprefa-v5-working-conventions` skill).

---

## Borrow gotcha (E0597 with tree-sitter cursor)

Using a `TreeCursor` as a tail expression fails with E0597 because the cursor
borrows from the node it was created from:

```rust
// FAILS: cursor lifetime doesn't outlive the expression
let matched_child = {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).find(|_| true)
};
```

Fix: bind to a named local and return the local, so the cursor is dropped
before the result is used:

```rust
let mut cursor = node.walk();
let named_children: Vec<_> = node.named_children(&mut cursor).collect();
// use named_children after the loop; cursor drops at end of block
```

---

## Cache invalidation gotcha

`source_rule_digests` (`src/engine/mod.rs:2569`, called from
`src/engine/tick.rs`) invalidates cached extractions when a **rule's text**
changes, but changing op SEMANTICS in the **binary** does NOT invalidate a
warm db. Rows linger until the rule or file changes.

After engine changes: `cargo install --path . --bin dl` (repo root) and touch
the relevant files (or delete the db) to force a re-extraction.

---

## Anchor drift log

Verified 2026-07-04 against the post-engine-breakdown tree:
- `BodyItem` enum: `ast.rs` line 242; `AstYaml` variant line 263
- `is_source()` match: `ast.rs` line 382 (`AstYaml` included at 385)
- parse dispatch ladder: `parse.rs` line 434 (`fn body_item`); `ast_yaml` arm
  at line 445, `fn ast_yaml` at line 696; `fn sg` at 654, `fn json` at 774
- typecheck arm: `typecheck.rs` line 213 (`BodyItem::AstYaml` in
  `normalize_body_item`)
- `parse_file` evaluation arm: `src/engine/mod.rs`, `fn parse_file` starts
  line 5511, the `AstYaml` match arm is ~line 5639
- `lib.rs` `pub mod` list: lines 1-30ish (confirmed pattern; `sg`, `datapath`,
  `rels` all present)

Re-verify with `grep -n` before trusting a line number in this file: it goes
stale the moment another op lands above these in the same match block.
