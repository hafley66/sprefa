---
name: sprefa-v5-new-extraction-op
description: Checklist for adding a new body-item extraction op (like match/ast/sg/json) to the v5 dl engine.
---

# Adding a new extraction op to v5

Canonical reference: the `json` op and the yaml/toml port (commit a35f879). The `json` op lives in `datapath.rs`; the pattern for a new separate op is `sg.rs` / `datapath.rs`.

---

## Checklist

### 1. Add a `BodyItem` variant

`src/ast.rs` ~line 130 — add to the `BodyItem` enum:

```rust
Yaml { path: Term, rev: Term, ypath: String, out: Term },
```

Also add to the `is_source()` match at ~line 154:

```rust
| BodyItem::Yaml { .. }
```

### 2. Add a parse arm

`src/parse.rs` ~line 205 — add to the `body_item` dispatch block:

```rust
if s == "yaml" { return self.yaml(); }
```

Then add the `fn yaml(&mut self) -> Result<BodyItem>` method mirroring `fn json`.

### 3. Add a typecheck arm

`src/typecheck.rs` ~line 201 — add to `normalize_body_item`:

```rust
BodyItem::Yaml { path, rev, out, .. } => {
    for t in [path, rev, out] { normalize_term(t, dl_path, diags); }
}
```

### 4. Add evaluation arm in `parse_file`

`src/engine.rs` ~line 2736 — add a `BodyItem::Yaml { .. }` arm inside `parse_file`. The binds-expansion pattern:

```rust
BodyItem::Yaml { ypath, out, .. } => {
    let ov = var_of(out)?;
    let vals = crate::datapath::run_yaml(path, &content, ypath);
    let mut next: Vec<Bind> = Vec::new();
    for b in &binds {
        for (v, lo, hi) in &vals {
            let mut ext = b.clone();
            ext.insert(ov.clone(), Value::Text(v.clone()));
            push_span(v, *lo, *hi, &mut where_bytes);
            next.push(ext);
        }
    }
    binds = next;
}
```

Key points:
- Iterate `&binds` (existing bindings), produce `next` by crossing with each hit — this is the binds-expansion pattern every op uses.
- Call `push_span(text, lo, hi, &mut where_bytes)` for every captured string value to write byte coordinates into the ref spine.
- `lo`/`hi` are byte offsets into `content`, not line numbers.

### 5. Write the evaluator module

If the op parses a new format, put the logic in its own module (`src/myop.rs`) and register it in `src/lib.rs` as `pub mod myop;`.

If the format is already handled by `datapath.rs` (json/yaml/toml all dispatch there on file extension), add a new dispatch branch there instead.

### 6. Tree-sitter grammar (if new format)

Add grammar dep to `Cargo.toml`. Proven pairs on tree-sitter 0.25:

```toml
tree-sitter-yaml = "0.7"
tree-sitter-toml-ng = "0.7"
```

Use CST walking over serde: values must carry byte spans. Serde cannot recover them.

For quoted scalars, return the inner span with quotes stripped. See `datapath.rs` `yaml_scalar_text` and `toml_unquote` for the offset adjustment pattern.

### 7. Update README and add a test

- Add a row to the op table in `README.md`.
- Add an e2e test to `tests/data_ops.rs` following the existing `sandbox` / `run` / `prog` pattern.

---

## Borrow gotcha (E0597 with tree-sitter cursor)

Using a `TreeCursor` as a tail expression fails with E0597 because the cursor borrows from the node it was created from:

```rust
// FAILS: cursor lifetime doesn't outlive the expression
let child = { let mut c = node.walk(); node.named_children(&mut c).find(...) };
```

Fix: bind to a named local and return the local, so the cursor is dropped before the result is used:

```rust
let mut c = node.walk();
let children: Vec<_> = node.named_children(&mut c).collect();
// use children after loop; c is dropped at end of block
```

---

## Cache invalidation gotcha

The rule-text digest (`source_rule_digests` in engine.rs) invalidates cached extractions when a **rule** changes, but changing op semantics in the **binary** does NOT invalidate a warm db. Rows linger until the rule or file changes.

After engine changes: run `cargo install --path v5 --bin dl` and touch the relevant files (or delete the db) to force a re-extraction.

---

## Anchor drift log

Verified 2026-06-11:
- `BodyItem` enum: `ast.rs` line 122 (original spec said ~130, actual 122)
- `is_source()` match: `ast.rs` line 154 (matches)
- parse dispatch: `parse.rs` line 202-205 (original spec said ~314 for `json` arm; actual json fn at line 302, dispatch at 205)
- typecheck arm: `typecheck.rs` line 201 (matches)
- `parse_file` json arm: `engine.rs` line 2736 (matches)
- `lib.rs` `pub mod` list: lines 1-20 (confirmed pattern)
