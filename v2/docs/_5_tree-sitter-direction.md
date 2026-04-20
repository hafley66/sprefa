# Tree-sitter direction (per-op grammar)

Source: chat 2026-04-18. Author confirmed: tree-sitter, with each
operator defining its own grammar, host grammar runs on top.

## What already exists in v2

The trait surface for per-op grammars is already there. From
`v2/src/_5_op.rs`:

```rust
pub trait Operator: Send + Sync {
    fn name(&self) -> &'static str;
    fn aliases(&self) -> &[&'static str] { &[] }

    fn bracket_grammar(&self) -> Option<GrammarRef> { None }
    fn paren_grammar  (&self) -> GrammarRef;
    fn brace_mode     (&self) -> BraceMode { BraceMode::DefaultFork }

    fn parse(&self, inv: &OpInvocation, pctx: &mut ProgramCtx)
        -> Result<Pipeline, Vec<Box<dyn Diagnostic>>>;
    // ...
}

pub struct GrammarRef(pub Arc<str>);

pub enum BraceMode { DefaultFork, CustomSprf, WalkerPattern }
```

Host grammar is locked at v2.1 in `_8_parse.rs`:

```
program  := stmt*
stmt     := chain ';'?
chain    := op ('>' op)*
op       := IDENT ('[' bracket ']')? ('(' paren ')')? ('{' brace '}')?
```

Slot contents are opaque to the host. `OpInvocation` carries the raw
text + byte_range of each slot to the op's `parse()`, which decides what
to do. Brace contents have three interpretations selected by
`brace_mode`: `DefaultFork` (sub-pipeline), `CustomSprf` (op-specific
sprf-shaped), `WalkerPattern` (the walker DSL in `v2/src/walk/`).

The shape is right. What is missing is the tree-sitter engine behind it.

## Direction (author-confirmed)

No hand parsing. Anywhere. The host gets `tree-sitter-sprefa`, the
walker DSL gets `tree-sitter-sprefa-walker`, each op binds its slots to
a tree-sitter `Language`. The runtime parses the outer program with the
host grammar, then walks each op_call node and re-parses each slot's
byte range with the op's chosen language via
`Parser::set_included_ranges`. Inner trees attach to the OpInvocation.

Composition shape:

```rust
let mut parser = Parser::new();
parser.set_language(&tree_sitter_sprefa::LANGUAGE.into())?;
let host = parser.parse(&src, None)?;

for op_call in walk_op_calls(&host) {
    let body = op_call.child_by_field_name("brace").unwrap();
    let lang = registry.lookup(op_name(op_call)).brace_grammar();
    parser.set_language(&lang.into())?;
    parser.set_included_ranges(&[body.range()])?;
    slot_table.insert(op_call.id(), parser.parse(&src, None)?);
}
```

One `Parser`, swapped between `Language`s. Each sub-tree owns its byte
range of the original source. Positions inside sub-trees are real
source positions; diagnostics emitted from inner trees rebase for free.

Recursive injection works without special-casing: `tree-sitter-sprefa`
injecting `tree-sitter-sprefa` into its own `DefaultFork` braces gives
a real nested tree.

Slots and their grammar bindings:

| Slot      | Trait method        | Today                   | After move                       |
| --------- | ------------------- | ----------------------- | -------------------------------- |
| `[...]`   | `bracket_grammar`   | `Option<GrammarRef>`    | `Option<&'static LanguageFn>`    |
| `(...)`   | `paren_grammar`     | `GrammarRef` (required) | `&'static LanguageFn` (required) |
| `{...}`   | `brace_mode`        | `BraceMode` enum        | `BraceMode` extended with `Custom(LanguageFn)` |

`BraceMode::DefaultFork` keeps the host-recursion semantics (brace = sub
sprf pipeline). `WalkerPattern` keeps walking through the existing
walker DSL until that grammar is itself converted. `Custom(lang)` is the
new escape hatch for ops that want their brace body parsed by an
arbitrary tree-sitter grammar (ast[lang] bodies, marker bodies, json,
md, etc).

## What still needs determining

These are open questions specific to landing tree-sitter in sprf v2.

### A. Host grammar scope (resolved 2026-04-18)

`tree-sitter-sprefa` includes:

- `program`, `stmt`, `chain`, `op_call`
- `IDENT` op-name
- `[...]`, `(...)`, `{...}` slot tokens with byte_range, contents opaque
- comments: `:-` family (line + scoped, exact shape TBD; prolog-adjacent
  to avoid collisions with embedded source languages)
- `>` pipe, `;` fork
- terms as HOST NODES (not leaf tokens):
  - `(term_decl name: (ident))`
  - `(term_ref  name: (ident))`
  - `(addr_ref  body: (addr_expr))`
- group opener `(name)` before an op chain = `term_decl`

Pattern body delimiter is NOT a host concern. The host parses balanced
parens and hands the byte range to the op's `paren_grammar`. Whatever
sub-grammar the op picks (e.g. `tree-sitter-sprefa-pattern` wrapping
`tree-sitter-rust`) figures out where the pattern ends. Inner `(` `)`
inside a pattern stay balanced from the host's view.

### B. GrammarRef migration

`GrammarRef(Arc<str>)` is a name today. After the move it should carry
the tree-sitter `Language` directly so the op trait stops needing a
runtime registry lookup. Either:

- `&'static LanguageFn` (tree-sitter 0.25 style) — zero alloc, requires
  `extern "C"` at link time
- `fn() -> Language` — equivalent, more conventional

Either way, ops written today using `GrammarRef("rust")` need updating
to `&tree_sitter_rust::LANGUAGE` or equivalent. Touch surface is small
(every op file, one return value).

### C. Slot injection mechanism

After host parse, the runtime needs a uniform pass that walks every
op_call and reparses its slots with the op's chosen languages. Two
shapes:

- **Eager, parse-time**: build all inner trees once, attach to
  OpInvocation. Memory cost = sum of inner trees. Simple.
- **Lazy, on-demand**: inner tree produced when the op's `parse()` asks
  for it. Saves memory on never-looked-at slots. More plumbing.

Phase 1 budget pressure (16GB / 500 repos) suggests lazy.

### D. BraceMode disposition (resolved 2026-04-18)

- `DefaultFork`: recursive `tree-sitter-sprefa` injection. `>` is the
  special pipe-flow delim and stays the brace-default.
- `WalkerPattern`: becomes `tree-sitter-sprefa-walker`.
- `CustomSprf`: survives. Ops can opt into a sprf-flavored brace body
  with their own twist.

### E. Walker DSL grammar

Resolved: walker DSL becomes `tree-sitter-sprefa-walker`. No
hand-parsing survives the move.

### F. ast-grep op grammar binding

The `ast_grep` op already drives tree-sitter via ast-grep-core. Its
slot grammars map to tree-sitter-rust / tree-sitter-typescript / etc
based on the `[lang]` selector. Open: does the op trait carry one
language at construction time, or pick the language at parse-time per
invocation?

Per-invocation is more flexible (one op kind handles every language)
but requires the trait to expose a `language_for_invocation(inv) ->
Language` slot rather than a constant.

### G. Migration order (resolved 2026-04-18)

Author does not care about migration order — targeting the v3 landing
directly, no transitional version. v3 ships with the new shape and
the hand-written parser is gone.

### H. Resolution table key stability (punted)

Tree-sitter incremental reparse preserves node ids for unchanged
regions. Keying Resolution by node id works for the common case;
edits that touch a term ref invalidate its Resolution (which you
want anyway since the ref text changed). Revisit only if hover info
flickers in practice.

### I. Cross-rule resolution timing (resolved 2026-04-18)

Cross-rule has no special namespace under v3. A Ref is a Ref; it
resolves to any in-scope Decl regardless of depth or origin. The
sigil `${rule.$V}` collapses into plain `${X}`. See
`_0_term-language.md`.

### K. Interpolation hole mechanism

Resolved (2026-04-18): host owns `${X}` / `&{...}` nodes; sub-grammar
parses with `set_included_ranges(&[body_range minus interp_ranges])`
so the sub-grammar sees the body as if interps are not there. Same
pattern tree-sitter-php uses for `<?= $x ?>`. Interp-position
validity per sub-grammar is enforced at body-extract stage, not by
the sub-grammar itself.

See `_6_lowering-pipeline.md` for the full pipeline.

### J. Term language shape and host grammar

The term-language doc specifies brace-required `${X}` / `$$${X}` /
`%{X}`. `_8_parse.rs::parse_capture` currently accepts both `$X` and
`${X}`. Locking the grammar to brace-required is a fixture migration
event. Either:

- Land tree-sitter and the brace-required rule together; one churn
  event.
- Land tree-sitter accepting both forms, then tighten the grammar
  later.

The first is cleaner; the second is safer.

## Grammar inventory after the move

Each is its own `tree-sitter-X` crate, independent at build/link time:

| Crate                              | Covers                                                     |
| ---------------------------------- | ---------------------------------------------------------- |
| `tree-sitter-sprefa`               | host (v2.1 outer); recursively injected into `DefaultFork` |
| `tree-sitter-sprefa-walker`        | walker DSL for `BraceMode::WalkerPattern`                  |
| `tree-sitter-sprefa-pattern` [tentative] | host term-ref interpolation inside sub-language bodies |
| `tree-sitter-rust` / `-typescript` / `-python` / ... | ast[lang] op bodies (already published) |
| `tree-sitter-markdown` / `-json` / `-toml` / ... | md / json / toml op bodies (already published) |

What grammars cannot share across the boundary:

- `extras` (whitespace, comments) — duplicated per grammar
- external scanners (hand-written C for context-sensitive bits) — per grammar
- `word`, `precedences`, `conflicts` — per grammar

Helpful conventions only:

- Shared scanner C files can be linked by multiple grammars for common
  lexer logic.
- grammar.js is full JS, so sprf grammars can share JS helper modules
  for naming/precedence consistency.

Cost is bounded — typically dozens of lines of grammar.js per grammar,
not hundreds.

## Pieces that already fit

These do not need new design — they map onto tree-sitter without
friction:

- **Per-op diagnostics** — ops walk their inner tree, emit Diag the
  same way they do today (`Op::pipe()` rules unchanged).
- **Hover dispatch** — node_kind keyed lookup, replaces today's
  byte-range probe in `analysis.rs::hover_at`.
- **Incremental reparse** — `Tree::edit` + `Parser::parse(text,
  Some(&old))`, drives the existing `DocSession::on_source_change`
  reparse path.
- **Path tagging** — runner-side `Pipeline::run` (today's leaf-first
  `PathSeg::Op` / `PathSeg::ForkArm`) is unchanged. Tree-sitter only
  affects parse, not run.
- **Content contract** — PATH A/B/C dispatch in op pipe() is
  orthogonal to host parser.
