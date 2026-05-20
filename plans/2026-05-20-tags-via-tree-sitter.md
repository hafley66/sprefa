# Tags via tree-sitter — first-class ctags-like surface in sprf

Status: DRAFT 2026-05-20. Investigation only; no worktree yet.
Author: spawned out of antijoin-surface session after the user's "use
tree-sitter for tags" pivot. Precondition: the antijoin commit
`c85d735` + in-memory-sqlite-default `7b58db9` are on `origin/main`.
Body-form semantics of `rule(:r, cols?) { body }` are still
user-confirmation-gated — see plans/2026-05-20-tags-via-tree-sitter.md
§ Open questions.

## Premise

Build-time introspection of the sprf program (`__rule` / `__call`
tables synthesized by the compiler) is too much code for the user's
"unique strings, checked refs" goal. The win is **point tree-sitter at
the source itself** and emit one cursor per matched capture — same
mechanism universal-ctags / GitHub code-nav use. Sprf already wires:

- `tree_sitter_sprefa` C entry point at
  `v4/crates/tree-sitter-sprefa/bindings/rust/lib.rs:16`
- `tree_sitter::Query` runtime (via the `tree-sitter = "0.24"` dep in
  `v4/Cargo.toml:35`)
- `WhereBytes { repo, rev, file, lo, hi, string }` carried on every
  `CursorTerm.at: Ref` (`v4/src/lib.rs:292`) — no FILE/LO/HI projection
  needed, dot-access handles read

Missing pieces:
- a `queries/tags.scm` file co-located with the sprefa grammar
- a `tags(:lang, KIND?: …, NAME?: …, SCOPE?: …)` op
- kwarg-aware `OperatorDef::lower_call` path for source-side filtering

## 1. Type signatures

```rust
pub struct TagsDef;

const TAGS_SPEC: &[ArgSig] = &[ArgSig {
    kind: ArgKind::Atom,
    name: "lang",
    doc: "language atom (:sprefa, :rs, :c, :cpp, :ts, :py, …)",
    required: true,
}];

impl OperatorDef for TagsDef {
    fn name(&self) -> &'static str { "tags" }
    fn paren_args(&self) -> &[ArgSig] { TAGS_SPEC }
    fn cursor_binds(&self) -> &'static [&'static str] { &["KIND", "SCOPE"] }
    fn key_terms(&self) -> &[&str] { &["KIND"] } // identity = (file, lo, hi, KIND)
    // override lower_call (not lower) to keep kwarg structure
    fn lower_call(&self, ctx, flow, args: &[CallArg], block, dsl)
        -> Result<Pipe<Cursor>, LowerError>;
}

pub struct TagsComponent {
    lang: TagLang,
    query: Arc<tree_sitter::Query>,     // compiled once from tags.scm
    kind_filter: Option<Arc<str>>,      // KIND kwarg, exact match
    name_filter: Option<Arc<str>>,      // NAME kwarg, exact match (post-query)
    scope_filter: Option<Arc<str>>,     // SCOPE kwarg
    root: PathBuf,
    store: Option<Arc<SprfStore>>,
}

#[derive(Clone, Copy)]
pub enum TagLang {
    Sprefa,
    Rust,
    C,
    Cpp,
    Ts,
    // …add as tags.scm files land
}
```

## 2. Body (pseudo)

```rust
fn lower_call(&self, ctx, _flow, args, _block, _dsl) {
    let lang = parse_lang_atom(first_positional_atom(args)?)?;
    let query = load_tags_query(lang);                  // include_str! per lang
    let kind = find_kwarg(args, "KIND")?;
    let name = find_kwarg(args, "NAME")?;
    let scope = find_kwarg(args, "SCOPE")?;
    let comp = TagsComponent {
        lang, query, kind_filter: kind, name_filter: name, scope_filter: scope,
        root: ctx.root.clone(), store: ctx.sprf_store.clone(),
    };
    Ok(Pipe::new().step(Arc::new(comp)))
}

// Per-cursor (mirrors AstNmComponent::render_batch shape)
fn render(&self, ctx, c: &Cursor) -> Node<Cursor> {
    let source = SourceReader::new(self.root.clone(), self.store.clone(), None)
                    .read_cursor_uninterned(c)?;
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&lang_to_ts(self.lang))?;
    let tree = parser.parse(&source.bytes, None)?;
    let mut q = tree_sitter::QueryCursor::new();
    let mut children = Vec::new();
    for m in q.matches(&self.query, tree.root_node(), source.bytes.as_slice()) {
        let (name_cap, kind_cap, scope_cap) = extract_captures(&self.query, &m);
        if let Some(k) = &self.kind_filter { if k.as_ref() != kind_cap.as_str() { continue } }
        if let Some(n) = &self.name_filter { if n.as_ref() != name_cap.text { continue } }
        if let Some(s) = &self.scope_filter { if s.as_ref() != scope_cap.unwrap_or("") { continue } }
        let mut child = c.clone();
        // Stamp focal = NAME text. The `at` is the NAME node's byte
        // range — that's WHERE the symbol IS, not where the definition
        // node BEGINS. (See § Open questions: which span wins?)
        let coord = Coord {
            repo: source.coord.repo, rev: source.coord.rev, fs: source.file,
            lo: name_cap.range.start as u32, hi: name_cap.range.end as u32,
        };
        stamp_source_value(&mut child, &self.store.as_ref().unwrap(), coord, name_cap.text);
        // Side terms — KIND always, SCOPE when present.
        child.set_arc("KIND", Arc::from(kind_cap.as_str()));
        if let Some(s) = scope_cap { child.set_arc("SCOPE", Arc::from(s)); }
        children.push(child);
    }
    Node::Multi(children.into_iter().map(Arc::new).collect())
}
```

## 3. Instance lifetimes

| Type | Lifetime |
|---|---|
| `tree_sitter::Query` | Arc'd, compiled once per `TagsDef::lower_call` |
| `TagLang` | enum, Copy |
| `kind_filter` / `name_filter` / `scope_filter` | `Option<Arc<str>>` |
| `tree_sitter::Parser` | per render() call — tree-sitter Parsers are NOT Send-safe across the rayon par_render fold. (Verify; the AstNm path uses ast-grep which wraps tree-sitter, may have its own handling.) |
| Compiled-once-per-lang query | considered: lazy_static `OnceCell<Query>` keyed by lang. Saves recompile across pipes. |

## 4. Storage layout, reads, writes

**Cursor shape per emit** (no projected location terms):

```
focal.name          = "" (FOCAL sentinel)
focal.value         = "<symbol text>"        // e.g. "openapi_ops"
focal.value_id      = StringId::of(text)
focal.cursor_value  = CursorValue::WhereBytes(where_bytes_id)
focal.at            = Ref::of(coord)         // ← carries file/lo/hi/repo/rev

terms["KIND"]       = "definition.rule" | "reference.rule" | …
terms["SCOPE"]?     = e.g. "openapi_ops" when match is inside a rule body
```

**Liftable**:

```rust
Liftable::Stream {
    schema: vec![
        (Col::from("$FOCAL"), IdKind::StringId),   // NAME
        (Col::from("KIND"),   IdKind::StringId),
        (Col::from("SCOPE"),  IdKind::StringId),   // nullable
    ],
    run: super::lower::liftable::stream_noop,
}
```

`at` is NOT a Liftable schema column — it's the `where_bytes_id`
column the fuser already wires onto every rule-fact row via the
existing source-tracking machinery (`v4/src/compile/fuser.rs` SELECT
clause sprf_blake3_id wraps it implicitly).

## 5. tags.scm for sprefa

Co-locate at `v4/crates/tree-sitter-sprefa/queries/tags.scm`.
`include_str!("../../queries/tags.scm")` in the rust binding crate, or
load at TagsDef construction via `Query::new(LANGUAGE, raw)`.

```scm
; rule(:name, ...)            -> definition.rule
; rule(:name) { ... }
(op_invocation
  name: (identifier) @_op (#eq? @_op "rule")
  (paren_slot
    (atom_literal (identifier) @name))) @definition.rule

; foo(...)  foo?(...)  foo!(...)   -> reference.rule
; (any op_invocation whose name is NOT a builtin keyword)
(op_invocation
  name: (identifier) @name) @reference.rule

; raw :atom literals anywhere
(atom_literal
  (identifier) @name) @reference.atom

; declared columns inside rule head: rule(:r, COL?, COL2?)
(op_invocation
  name: (identifier) @_op (#eq? @_op "rule")
  (paren_slot
    (paren_slot (identifier) @name (#match? @name "\\?$")))) @definition.col
```

(Exact node names confirmed via `v4/crates/tree-sitter-sprefa/src/node-types.json`. Stub above; the real predicates need a pass against the grammar.)

## 6. Surface examples

```sprf
; one-step: every rule decl across the workspace
fs(glob`**/*.sprf`)
  > tags(:sprefa, KIND: `definition.rule`)

; stale-ref derived rule (body semantics user-confirmation-gated):
rule(:rule_decl, NAME?) {
    fs(glob`**/*.sprf`) > tags(:sprefa, KIND: `definition.rule`)
};

rule(:rule_ref, NAME?) {
    fs(glob`**/*.sprf`) > tags(:sprefa, KIND: `reference.rule`)
};

rule(:unreferenced_rule, NAME?) {
    rule_decl?(NAME?) > not(rule_ref?(NAME))
};

unreferenced_rule?(NAME?)
  > lsp_warn(:unreferenced_rule)`rule \`${NAME}\` declared but never used`;
```

LSP hover/goto at the declaration site falls out of the existing
probe-sink wire (`v4/src/compile/probe_wrap.rs`) — every emitted
cursor carries `at`, which is what the LSP span resolver uses.

## 7. Implementation order (smallest first)

1. **`tags.scm` for sprefa** — author ~30 lines, vendor under
   `v4/crates/tree-sitter-sprefa/queries/tags.scm`. Add a test in
   that crate that compiles the query against `LANGUAGE` and matches
   a hand-written .sprf string. Zero op work.
2. **`TagsDef` op** (sprefa-only first):
   - `v4/src/compile/lower/ops.rs`: new `TagsDef` after `AstDef` (~150 LoC).
   - Register in `v4/src/compile/lower/mod.rs` near the AstDef line.
   - Kwarg-aware: override `lower_call` not `lower`. Use the
     `find_kwarg(args, "KIND")` helper (new; ~20 LoC).
   - `Liftable::Stream` classify.
3. **`TagsComponent`** in `v4/src/v2_ops.rs` next to `AstNmComponent`
   (~200 LoC). Mirrors the AstNm source-read path. Emits one cursor
   per capture-match with `at` stamped from the NAME node range.
4. **Liftable wiring** — `tags` lands in `classify_op` (fuser.rs:155)
   alongside `ast`, `fs`. Same Stream-eligibility.
5. **Tests**:
   - `tags_classify_target.rs` — assert Stream schema, kwarg parsing.
   - `tags_smoke.rs` — run on a small .sprf string, assert capture
     count + cursor `at` + KIND term.
   - `stale_refs_example_smoke.rs` — CLI run of the full example,
     assert lsp_warn output.
6. **Multi-language extension** — add tags.scm files for rust/c/cpp
   (re-using ast-grep-language's tree-sitter parsers) behind a feature
   flag. Out of scope for the MVP commit.

## 8. Open questions

**Q1 — body semantics**. The example bodies above assume the rule's
declared captures (`NAME?` in the head) are populated by the body's
terminal step. The user has not confirmed this in writing. Memory
`feedback_no_imperative_seed_pipes.md` flags this as user-correction-
gated — ask before shipping a test that depends on this shape.

**Q2 — kwarg consumption**. `OperatorDef::lower_call` already receives
`&[CallArg]` with keyword info preserved. But the default `lower_call`
strips it to `&[Value]` for `lower()`. We override `lower_call`. Need
to verify the registry's `lower_call_at` dispatch (`v4/src/compile/walk.rs:550`)
calls `lower_call` not `lower` when an op overrides it. Likely
just-works given the trait default-method pattern, but verify with a
read of `Registry::lower_call_at`.

**Q3 — which span is `at`?**. tags.scm produces TWO spans per match:
the outer `@definition.rule` span (covers `rule(:foo, ...)` entirely)
and the inner `@name` capture (just `foo`). Which becomes the cursor's
`at`? Proposal: `@name`'s span — that's where the SYMBOL is, what the
LSP wants for goto-definition. Outer span only matters for "what's
this rule's full source extent" which is a separate query.

**Q4 — `tags.scm` loading**. include_str! at compile time pins the
queries into the binary (refresh requires rebuild). Loading from disk
at runtime adds I/O + cwd questions. include_str! is simpler; if
ergonomics suffer the user can override via an env var pointing at
an external tags.scm. Default: include_str.

**Q5 — `Parser::new()` cost per cursor**. tree-sitter parsers are
cheap to construct but not free. The AstNm path creates one parser per
`par_render` worker via `lang.ast_grep(&src)`. Mirror that — one
parser per render closure invocation, not pooled.

**Q6 — scope of `SCOPE` term**. What goes in SCOPE? The rule name a
reference appears inside, the enclosing definition's NAME, or empty?
Proposal: enclosing definition NAME if any (e.g. `reference.rule`
inside `rule(:foo, ...) { bar?(X?) }` → SCOPE=`foo`). Done via a
second tree walk that records enclosing definition node per match.
Marginal cost; meaningful for stale-ref queries scoped to a rule.

**Q7 — recursive vs non-recursive bodies**. The `rule_decl` /
`rule_ref` rules above are non-recursive (single Stream source).
`unreferenced_rule` reads `rule_decl?` and `not(rule_ref?)` — pure
RuleQuery + AntiJoin, falls into the FullSql kind. No recursion path
needed. Confirm `Stream → RuleQuery` order is fine (it is — see fuser
kind selection at `v4/src/compile/fuser.rs:330`).

**Q8 — cross-language tags table**. If `tags(:rs)` and `tags(:sprefa)`
both emit rows into the same downstream rule, the `lang` info needs to
be a column or the rule needs to be scoped per-lang. Likely add LANG
term to the cursor in addition to KIND/SCOPE. Out of scope for MVP
(sprefa-only) but flag for the multi-lang extension.

## 9. Cost estimate

| step | LoC | uncertainty |
|---|---|---|
| tags.scm (sprefa) | 30-50 | low |
| `find_kwarg` helper | 20 | low |
| `TagsDef` (lower_call override) | 100 | medium |
| `TagsComponent` (render) | 200 | medium — par_render + tree-sitter parser per closure |
| `classify_op` wire | 5 | low |
| tests (3) | 150 | low |
| docs / example | 60 | low |
| **total** | **~565** | |

Compared to the introspection-table alternative (which would have
needed compiler-state synthesis at lower-time, ~600+ LoC for
`__rule` / `__call` alone, plus a host-injection seam): roughly even.
But the tags approach generalizes to ANY language with a tags.scm —
the introspection table only ever sees sprf source.

## 10. Out of scope (deliberately)

- Multi-language tags.scm files (rust/c/cpp) — separate vendor pass.
- Cross-language stale-ref (rust definitions referenced by sprf calls)
  — different problem; needs symbol resolution across language
  boundaries.
- `tags(:lang)` over a buffer that hasn't been written to disk (LSP
  unsaved buffers) — needs a buffer-text source op. Future.
- Streamed-Rust integration with stratifier `neg:` SUBSCRIBE edges
  (antijoin follow-up) — orthogonal arc.
