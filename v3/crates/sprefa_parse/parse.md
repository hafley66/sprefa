# sprefa v3 — language spec

Single source of truth for the v3 surface: grammar, lowering, runtime
semantics, persistence, and tooling contracts. This file absorbed the
former `v3/docs/v3-unified-language-locks.md` on 2026-04-21. Companion
docs (`v3-semantic-model.md`, `v3-plugin-author-surface.md`,
`v3-min-author-ops.md`, `v3-vs-v2-reading-preview.md`) remain as
essays; they reference this file and do not lock anything.

Sessions captured: 2026-04-19, 2026-04-20, 2026-04-21. Update this file
alongside the code. Drift is worse than redundancy.

---

## Table of Contents

### Part I — Foundations
1. [Scope and layering](#1-scope-and-layering)
2. [Core invariants (six)](#2-core-invariants-six)
3. [Concept model](#3-concept-model)
4. [Three tiers: stream / name / value](#4-three-tiers-stream--name--value)

### Part II — Surface grammar
5. [Casing as syntax](#5-casing-as-syntax)
6. [Sigils — three, each with one lowering](#6-sigils--three-each-with-one-lowering)
7. [The `$` op family](#7-the--op-family)
8. [Cursor narrowing at carveout](#8-cursor-narrowing-at-carveout)
9. [Dotted access and xrefs](#9-dotted-access-and-xrefs)
10. [`> $X` capture-write](#10--x-capture-write)
11. [Fork and void](#11-fork-and-void)
12. [Scan-pointers as ops](#12-scan-pointers-as-ops)

### Part III — Sub-grammars
13. [Sub-grammar lowering (two flavors)](#13-sub-grammar-lowering-two-flavors)
14. [Core pattern DSLs — glob + regex host-owned](#14-core-pattern-dsls--glob--regex-host-owned)
15. [Term annotations — open exploration lane](#15-term-annotations--open-exploration-lane)

### Part IV — Semantics
16. [Phase ordering: parse, lower, run](#16-phase-ordering-parse-lower-run)
17. [Rule = named Pipeline with params](#17-rule--named-pipeline-with-params)
18. [Arg-mode dispatch](#18-arg-mode-dispatch)
19. [Binding resolution — three phases, five sources](#19-binding-resolution--three-phases-five-sources)
20. [Control flow and fork intersection](#20-control-flow-and-fork-intersection)
21. [Lazy / subscribe policy](#21-lazy--subscribe-policy)
22. [Runtime model — mergeByKey](#22-runtime-model--mergebykey)
23. [Dagging — StageDeps](#23-dagging--stagedeps)

### Part V — Persistence + effects
24. [Relations tier](#24-relations-tier)
25. [Mutation effects — four optional slots](#25-mutation-effects--four-optional-slots)

### Part VI — Tooling
26. [Diagnostics surface](#26-diagnostics-surface)
27. [Op authoring surface — min-viable](#27-op-authoring-surface--min-viable)
28. [First-pass implementation scope](#28-first-pass-implementation-scope)

### Part VII — Meta
29. [What was discarded](#29-what-was-discarded)
30. [Open items](#30-open-items)
31. [Invariant count summary](#31-invariant-count-summary)
32. [Future: type transclusion](#32-future-type-transclusion)
33. [Reading order](#33-reading-order)

---

# Part I — Foundations

## 1. Scope and layering

1.1 This spec governs everything the parser, lowerer, and runtime must
decide. Grammar syntax is authoritative in `v3/crates/tree-sitter-sprefa/grammar.js`;
this file is authoritative for semantics and lowering.

1.2 `sprefa_parse` is a leaf crate holding the AST types and parse
functions. Downstream crates (`pipeline`, plus the future `sprefa`
runtime) consume it. The boundary is enforced by crate separation, not
convention.

```text
sprefa_parse  ──────────────▶  pipeline  ──────▶  sprefa (runtime, LSP, HTTP)
  site::ParseSite                   Op trait         registry, store, drivers
  ast::OpInvocation                 Pipeline enum
  parse::host_parse()               Cursor
```

1.3 LSP is not tied to `tower-lsp`. The main `sprefa` crate hosts an
HTTP server; the tower-lsp adapter and the CLI are both proxies.
LSP-as-op lives in core, per the v2 pattern.

1.4 When this spec disagrees with code, code wins if recent (< 1 week);
otherwise this file wins and code is out of date. Teaching doc
(`v2/docs/_b_v3-unified-language.md`) is historical context only.

---

## 2. Core invariants (six)

1. **Ops own everything** — diagnostics, patterns, hover, fix, effect type, schema, registry access.
2. **Cursor is the unit of flow** — `BoxStream<Arc<[Cursor]>>`.
3. **Content contract PATH A → B → C** — slot reuse → `cursor.content[byte_range]` → `reader.bytes()`.
4. **Reads are pipe, writes are deferred effects.**
5. **Reparse cheap, cancellation real.**
6. **Every cursor carries `path: SprfPath`** — never Option, never synthesized at read time.

---

## 3. Concept model

Six concepts, four Pipeline cases, five EntityRef cases, six Cursor fields.

### 3.1 Concepts

| concept | what it is |
|---|---|
| Op | Rust-implemented cursor-stream transform |
| Rule | named Pipeline; zero or N params; registered per rule-op invocation |
| Pipeline | composition of ops; the runtime unit |
| Cursor | the flow unit; carries path + content + byte_range + slots + captures + parent + last_bound |
| Capture | named projection from a cursor stream; addressed by `TermPath = (scope_path, name)` |
| Scalar | value tier; string / number / atom / bool / null |

### 3.2 Pipeline

```rust
enum Pipeline {
    Op(LoweredOp),
    Chain(Vec<Pipeline>),     // A > B > C
    Group(Box<Pipeline>),     // (A > B)
    Fork(Vec<Pipeline>),      // { > A; > B; }
}
```

### 3.3 EntityRef

```rust
enum EntityRef {
    Scalar(Value),
    Op(Arc<dyn Operator>),
    Pipeline(Arc<Pipeline>),      // anonymous, content-hash-named
    Rule(Arc<Rule>),              // named; has params: Vec<TermPath>
    Capture(SlotKey),
}
```

### 3.4 Cursor (Shape C — final)

```rust
struct Cursor {
    path: SprfPath,                    // always present
    content: Arc<[u8]>,                // flow-universal
    byte_range: Range<usize>,          // flow-universal
    slots: SlotMap,                    // typed payload
    captures: Captures,                // named bindings
    parent: Option<Arc<Cursor>>,       // narrowing chain
    last_bound: Option<Arc<str>>,      // name of the most recently written capture
}
```

`fs` / `repo` / `rev` live as slot entries, owned by their respective
ops. Content and byte_range stay as struct fields because every
byte-reading op reads both. `last_bound` is the scan-pointer mechanic
(see §12); set by capture-writing ops, read by annotate-by-reference ops.

### 3.5 SprfPath and ParseSite

Two coordinate systems coexist: `ParseSite` names a location in the
`.sprf` source (compile-time stable); `SprfPath` names the per-cursor
runtime trail through pipeline stages.

```rust
// compile-time coordinate
struct ParseSite {
    file:       Arc<Path>,
    path:       Arc<[ParseSeg]>,
    byte_range: Range<usize>,
}

enum ParseSeg {
    Child { index: u16 },
}

// runtime coordinate
struct SprfPath { segments: Vec<PathSeg> }

enum PathSeg {
    Op { name: &'static str, step: usize },
    ForkArm { index: usize },
    Named(Atom),                                // rule name
    Anon { file: Atom, hash: [u8; 8] },         // synthesized for anonymous Pipeline
    Carveout { source_range: Range<usize> },    // §8
}
```

`ParseSeg::Child { index }` walks named-child indices from the tree
root. The previous v2 triad (`BraceChild`, `ParenChild`, `PatternLeaf`)
is retired. Tree-sitter owns the walk.

Anonymous pipeline name: `file_atom(source_file) + "." + blake3(normalized_ast).short()`.

---

## 4. Three tiers: stream / name / value

Information in a running sprf program lives in one of three tiers.

4.1 **Stream tier.** Cursors flow through operators. Runtime evaluation model.

4.2 **Name tier.** Identifiers bind to values in a single lexically-scoped
environment. Resolved kind: op, rule, capture, or scalar (see §3.3).

4.3 **Value tier.** Scalar literals: string, atom, number, bool, null.

```rust
enum Value {
    String(Arc<str>),
    Atom(Arc<str>),
    Number(f64),
    Bool(bool),
    Null,
}
```

4.4 Values flow as fields inside cursors. Names reference streams of
cursors. Ops transform streams. One uniform rule per tier.

---

# Part II — Surface grammar

## 5. Casing as syntax

5.1 First character of an identifier determines its category:

| first char       | category                           |
|------------------|------------------------------------|
| UPPERCASE letter | term (capture decl or ref)         |
| lowercase letter | op or rule name                    |
| punctuation      | sigil op (`$`, `&`, carveouts)     |
| digit            | number literal                     |

5.2 Prolog convention. Locked 2026-04-19; first-pass implementation
does not enforce it — convention only.

5.3 Enforcement (future): at classify time, bare ident starting
uppercase is rejected unless preceded by `$`. Bare ident starting
lowercase in term position is rejected. Two symmetric diagnostics,
reserved for a later pass.

---

## 6. Sigils — three, each with one lowering

### 6.1 `$TERM`

Universal term intervention. Mode dispatched by position; semantics by op.

| source position | lowered form |
|---|---|
| `$TERM` as op arg | `ArgValue::TermRef { term_path, slot_key }` |
| `> $TERM` chain station | `Pipeline::Op(CaptureWriteOp, [TermRef])` |
| `$TERM` standalone in chain | `Pipeline::Op(TermOp, [TermRef])` — filter-semijoin default |
| `$TERM` in walker pattern hole | walker-internal capture slot decl |
| `${expr}` carveout | host expr lowers to `Pipeline`, wrapped by `CarveoutOp` with narrowed cursor |
| `${op($T1, $T2)}` | application lowers normally; TermRefs propagate |

Runtime resolution:

```rust
fn resolve_term(cursor: &Cursor, r: &TermRef) -> TermMode {
    match cursor.captures.get(r.slot_key) {
        Some(v) => TermMode::Bound(v.clone()),
        None    => TermMode::Unbound,
    }
}
```

### 6.2 `&`

Cursor rebase.

| source | lowered |
|---|---|
| `&.fs` / `&.repo` / `&.rev` / `&.byte_range` | `Pipeline::Op(CursorRefOp, [AddrExpr::CursorField(kind)])` |
| `&.$X` | `Pipeline::Op(CursorRefOp, [AddrExpr::Capture(ref)])` |
| `&{addr}` | addr parses under address grammar; lowers to `AddrExpr::Computed(pipeline)` |

### 6.3 Carveouts `${...}` and `&{...}`

Balanced-brace pre-pass scan at lex time. Inside `${...}` parses as
host expr grammar. Inside `&{...}` parses as address grammar. Carveout
ranges get subtracted from sub-grammar's included ranges via
`set_included_ranges`.

### 6.4 `&&` — retired

No double-ampersand sigil. Registry access is op-mediated:
`rule($R)` with `$R` unbound iterates the rule registry; `fs($P)` with
`$P` unbound iterates fs-op invocations. Each op owns its registry.

### 6.5 `$$` — retired

No double-dollar sigil. The v2 `ScanPointerRef { sigil }` class and the
transitional "Ans slot" recycle are both gone. Scan-pointer becomes an
ordinary op reading `cursor.last_bound` (see §12). Annotate-by-reference
ops read `last_bound`; the source author never writes `$$`.

---

## 7. The `$` op family

### 7.1 Unified shape

`$` is a single op with two argument shapes: bare uppercase ident, or
balanced-brace expression body.

```rust
//   $NAME        ≡   ${NAME}
//   ${expr}      →   CarveoutOp around host_expr(expr)
//   ${op($X)}    →   normal application; TermRef for $X propagates
//   $NAME in     →   TermRef { name: "NAME" }
//   op arg
```

`$NAME` is shorthand for `${NAME}`. Same AST node in both cases:

```rust
enum CarveoutNode {
    TermRef { name: Arc<str>, parse_site: Arc<ParseSite> },
    Expr    { raw_range: Range<usize>, parse_site: Arc<ParseSite> },
}
```

The parser re-enters `Expr.raw_range` as host_expr when lowering a carveout.

### 7.2 Lex rule

On byte `$`:

| follow-up    | action                                                         |
|--------------|----------------------------------------------------------------|
| `{{`         | shell-brace escape (§7.4)                                      |
| `{`          | enter carveout, balanced-brace scan (§7.3)                     |
| `[A-Z]`      | term-ref shorthand; consume `[A-Z0-9_]+`                       |
| other        | parse error `bare-dollar-without-target`                       |

`$$` is never a valid token (see §6.5).

### 7.3 Balanced-brace pre-pass

Runs at lex time. Produces a `Vec<Carveout>` indexed by byte position
used by every sub-grammar's included-ranges computation.

```rust
struct Carveout {
    outer_range: Range<usize>,   // includes `${` and `}`
    inner_range: Range<usize>,   // strictly between braces
    kind:        CarveoutKind,
}

enum CarveoutKind {
    HostExpr,      // ${...}
    Address,       // &{...}
    ShellLiteral,  // ${{...}}
}
```

Scanner rules:

- Inside `"..."` and `'...'`: skip contents (respecting escape sequences).
- Inside `r"..."` / `r#"..."#`: skip with matching hash count.
- Inside `#` comment to end-of-line: skip.
- Nested `${` and `&{`: push a new frame; track both kinds independently.
- Shell escape `${{` ... `}}`: atomic; braces don't affect the counter.

Sub-grammar consumers (ast-grep walker body, regex, json/yaml/toml,
shell) must call:

```rust
pub fn strip_carveouts(
    range:     Range<usize>,
    carveouts: &[Carveout],
) -> Vec<Range<usize>>
```

to produce the included-ranges multi-range their parser sees, with
carveout bytes removed.

### 7.4 Shell-brace escape `${{...}}`

Opens a `CarveoutKind::ShellLiteral` whose inner bytes pass through
verbatim to `sh` op bodies. Allows writing `${VAR}` literally in a
shell command. Lexer scans to matching `}}`; single-brace counts are
ignored inside.

Only meaningful inside a `sh(...)` body. Elsewhere treated as a parse
error or as a literal `${...}` pair by the host grammar (choice pinned
at implementation).

### 7.5 Parser re-entry on carveouts

When the host parser reaches a `Carveout` token, it recursively parses
`inner_range` using the host-expr grammar. Re-entry is eager: errors
inside surface at parse time, not at lower time.

Lowered form:

```rust
Pipeline::Op(CarveoutOp {
    inner:        Arc<Pipeline>,
    source_range: Range<usize>,   // outer_range from §7.3
})
```

`source_range` is the lexical `${...}` span in the outer .sprf source.
It is the byte-range the cursor narrows to at runtime, not the extent
of the inner expression.

---

## 8. Cursor narrowing at carveout

`CarveoutOp::pipe` runs per incoming cursor. For each cursor:

```rust
fn pipe(&self, ctx: OpCtx, stream: BoxStream<Arc<[Cursor]>>) -> BoxStream<Arc<[Cursor]>> {
    stream
        .map(|batch| batch.iter().map(|c| narrow(c, self.source_range.clone())).collect())
        .flat_map(|narrowed| self.inner.pipe(ctx.clone(), narrowed))
        .map(|batch| batch.iter().map(|c| rebase(c, outer_byte_range)).collect())
        .boxed()
}
```

Field-by-field inheritance at narrow (entry):

| cursor field | narrow policy |
|---|---|
| path         | appended with `PathSeg::Carveout { source_range }` |
| content      | inherited (same Arc) |
| byte_range   | replaced with `source_range` |
| slots        | inherited |
| captures     | inherited |
| parent       | set to outer cursor |
| last_bound   | inherited |

At rebase (exit), `byte_range` is restored to the outer cursor's. All
other fields carry whatever the inner pipeline wrote. Pattern sugar
stays isomorphic to inline form — the only thing carveout *owns* is
range narrowing.

The narrow/rebase helpers reuse `cursor_ref` machinery; do not build a
parallel path.

---

## 9. Dotted access and xrefs

9.1 `rule.$V` is an ordinary host_expr. Left of `.` resolves to an
`EntityRef::Rule`; right of `.` is a capture projection.

```rust
struct Xref {
    rule:       Arc<str>,
    capture:    Arc<str>,
    parse_site: Arc<ParseSite>,
}

ArgValue::TermRef {
    term_path: TermPath { scope: rule_scope, name: capture },
    slot_key:  runtime_key,
}
```

9.2 At runtime: the op containing the xref subscribes to `rule`'s
output stream and performs a semijoin on `capture`. Parked cursors
wait for `rule` to emit a matching row; drop silently on upstream close.

9.3 `${rule.$V}` is a carveout whose body is the host_expr `rule.$V`.
No special-case lexer form. `${rule.$V > $TARGET}` parses as a chain
of xref + capture-write inside a carveout.

9.4 Casing rule: `rule.name` (lowercase right of dot) is a path
continuation; `rule.$V` (capture sigil) is a capture projection.
Resolver disambiguates.

---

## 10. `> $X` capture-write

10.1 At chain-step position, a bare `$IDENT` (not followed by `(` or
`{`) lowers to a capture-write:

```rust
struct CaptureWriteOp {
    target: Arc<str>,
}

impl Op for CaptureWriteOp {
    fn pipe(&self, _ctx: &RtCtx, c: Cursor) -> BoxFuture<Vec<Cursor>> {
        Box::pin(async move {
            let mut out = c;
            out.captures.push(Capture {
                name:       self.target.clone(),
                byte_range: out.byte_range.clone(),
            });
            out.last_bound = Some(self.target.clone());
            vec![out]
        })
    }
}
```

10.2 Semantics:

- Write `captures[slot] = SpanBacked { span: cursor.byte_range }`.
- Set `last_bound = Some(slot)`.
- Emit the cursor; `content` and `byte_range` are untouched.

10.3 Annotate-only. The narrowing variant (`&>` sigil) was considered
and dropped in favor of fork-to-void (§11) for the rare case where a
side computation should transform its own content without polluting
the main cursor.

10.4 Storage type: `captures[slot]` is a `Capture` with `SpanBacked`
kind holding a `Range<usize>` into `cursor.content`. Zero-copy — the
content Arc is shared. Downstream `&.$X` rebase narrows to the stored
span without copying.

10.5 Binding-graph contribution: lower phase records
`BindingSource::ChainStageEmit(stage_id)` for the slot. Downstream
`$X` refs resolve to this source; resolver rejects term refs with an
empty source vector (§19).

---

## 11. Fork and void

11.1 Fork syntax: `{ > A ; > B ; }`. Each arm is a pipeline starting
with `>` or bare chain. Arms are separated by `;`.

11.2 Lowered form:

```rust
Pipeline::Fork(vec![
    ForkBranch { pipeline: arm0, parse_site: site0 },
    ForkBranch { pipeline: arm1, parse_site: site1 },
])
```

11.3 Runtime: fork duplicates each incoming cursor to every arm via
`Arc::clone`. Arms run concurrently. Merge is stream interleave — no
join, no combine, no key matching. Each emitted cursor carries
`PathSeg::ForkArm(i)` appended at fork entry.

11.4 Multiplicity: if upstream emits K cursors and there are N arms
with per-arm multiplicities `m_i`, the fork emits `K * sum(m_i)`
cursors. Arms ending in `void` contribute 0.

11.5 `void` is a regular op that drains and emits nothing:

```rust
struct VoidOp;

impl Op for VoidOp {
    fn name(&self) -> &'static str { "void" }
    fn pipe<'a>(&'a self, _ctx: &'a RtCtx, _c: Cursor) -> BoxFuture<'a, Vec<Cursor>> {
        Box::pin(async move { Vec::new() })
    }
}
```

11.6 Fork-to-void pattern for side-effect taps that transform:

```sprf
A > $VAR > {
  > norm > scan(:kind) > void ;   # side-chain; sink via void
  > main_rest ;                    # main flow continues
}
```

Arm 0 inherits `$VAR` and `last_bound`, runs `norm` (may rewrite its
own content/byte_range; arm-local because each arm holds its own
cursor clone), records via `scan`, drops via `void`. Arm 1: untouched
main flow. Merge output = arm 1 only.

11.7 Pure pass-through annotate-ops do not need fork-to-void. Inline
is sufficient:

```sprf
> foo > $VAR > scan(:kind)         # reads cursor.last_bound → VAR
```

---

## 12. Scan-pointers as ops

12.1 No syntactic scan-pointer class. No `$$` sigil (see §6.5). The
v2 `$$sigil` token class is retired.

12.2 A scan-pointer / annotate-by-reference op is an ordinary op
taking a bound term and writing a relations row. It reads
`cursor.last_bound` when the author omits an explicit term argument.

```rust
// One concrete shape (exact signature is op-author's call):
struct ScanPointerOp {
    kind: Arc<str>,   // e.g. "repo", "rev", "fs", "repo_norm"
}

// Reads:  cursor.last_bound → captures[that]
// Writes: relations row with kind + payload
// Cursor: passes through unchanged.
```

12.3 Built-in variants are free to specialize: `is_repo($R)`,
`is_rev($T)`, `is_fs($F)`, `is_repo_norm($R)`, `is_rev_norm($T)`. Each
owns its kind and relations writer logic. `$R` etc are bound terms;
mode dispatch per §18 rejects unbound call sites at lower.

12.4 Scan-pointer rows land in the `relations` table (§24).

12.5 Inline use:

```sprf
> foo > $VAR > is_repo($VAR) > ...
```

No fork, no sigil. Cursor flows; relations table gets a row. `last_bound`
lets the author write `> scan(:repo)` without re-naming `$VAR` when
the previous op already did the naming.

---

# Part III — Sub-grammars

## 13. Sub-grammar lowering (two flavors)

| sub-grammar | lowers to | why |
|---|---|---|
| json, yaml, toml, md | sprf op chain (fork over field-extract ops) | structural, composable |
| ast-grep walker | walker-native rule tree + capture slot decls | opaque, engine-owned |
| regex (as op body) | single `re_match(pattern)` op | opaque, leaf; see §14 for arg-position regex |
| shell | `sh` op with body as literal + carveout substitution | opaque, effect |

`${...}` and `&{...}` carveouts inside any sub-grammar body are
extracted via the balanced-brace pre-pass (§7.3); sub-grammar parses
with those ranges excluded. Host ranges re-enter as narrowed-cursor
sub-pipelines.

`sh` double-brace escape `${{var}}` passes literal `${var}` to shell.

---

## 14. Pattern DSLs — ops own grammar, queries, and parse

Patterns are **ops**, not string literals. `str(...)`, `glob(...)`,
`re(...)`, `json(...)`, `ast(...)`, `sh{...}` are op invocations whose
paren (or brace) body is their pattern. No quoted-string pattern
surface. No host-owned pattern sort. Each op ships its own grammar
and owns every tool-facing concern for its DSL.

Ops own everything is the invariant (§2.1). Pattern ops take that
literally.

### 14.1 Per-op deliverables

Each pattern op lives in its own folder under
`v3/crates/pipeline/src/ops/<name>/`:

```
ops/glob/
├── mod.rs                       # Op + PatternOp impls
├── grammar.js                   # tree-sitter grammar for paren body
├── scanner.c                    # OPTIONAL; only if op needs external tokens
└── queries/
    ├── highlights.scm           # editor colorization
    └── injections.scm           # OPTIONAL; for nested DSLs
```

Non-pattern ops (`capture_write`, `void`) remain single-file
`ops/<name>.rs`.

### 14.2 Surface

```sprf
str(literal bytes)               # constant; diag pattern/str-forbids-hole on any $TERM
glob(**/$DIR/file.txt)           # $DIR is a term_ref CST node inside glob sub-tree
re(TODO\($WHO\))                 # $WHO is term_ref; native (?<X>...) also allowed
ast[rust](fn $NAME ($$$ARGS))    # bracket tags language; body is ast-grep pattern
json({ pkg: $PKG, version: $V }) # json walker owns its brace grammar
```

String literals survive in the grammar for scalar positions (atoms,
messages). They never carry pattern bodies.

### 14.3 Shared tokens — one source of truth

Every pattern sub-grammar must emit `term_ref` and `carveout_expr`
identically. Host ships a shared rules fragment:

```text
v3/crates/tree-sitter-sprefa/
└── shared/
    └── tokens.js                # exports term_ref + carveout_expr
```

Each op's `grammar.js` spreads it:

```js
const shared = require('../../../tree-sitter-sprefa/shared/tokens');
module.exports = grammar({
  name: 'sprefa_glob',
  rules: {
    source_file: $ => repeat($._atom),
    _atom: $ => choice($.segment, $.double_star, $.term_ref, $.carveout_expr),
    segment:     $ => /[^/*${}]+/,
    double_star: $ => '**',
    ...shared($),
  },
});
```

Build-script asserts every op's `grammar.js` contains `...shared($)`.
Drift-grep, not an external scanner. (Rationale: tree-sitter external
scanners are ~300-line C state machines; two lines of shared JS beat
that for a two-token shared surface.)

### 14.4 Build pipeline

Lives at `pipeline/build.rs`. One script walks the ops folder.

| step | what | output |
|---|---|---|
| 1 | walk `src/ops/*/grammar.js` | list of pattern-op names |
| 2 | for each, shell out to `tree-sitter generate` | per-op `parser.c` under `OUT_DIR/ops/<name>/` |
| 3 | compile each `parser.c` + optional `scanner.c` via `cc` | one `.a` per op |
| 4 | emit `op_languages.rs` | `pub fn language_of(name) -> Option<Language>` + `highlights_of(name)` |
| 5 | regenerate host `queries/injections.scm` from the op list | committed alongside `parser.c` |
| 6 | grep each op's `grammar.js` for the `...shared($)` spread | drift assert, fails build on miss |

Generated host injection query (shape):

```scheme
((op_invocation
  name: (identifier) @_n
  paren: (paren_slot) @injection.content)
 (#match? @_n "^(str|glob|re|json|ast|sh)$")
 (#set! injection.language "sprefa_\\1"))
```

### 14.5 Rust trait surface

Two slots added to `Op` (both default to "not a pattern op"), plus a
companion `PatternOp` sub-trait for pattern-specific hooks.

```rust
pub trait Op: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn pipe<'a>(&'a self, ctx: &'a RtCtx, c: Cursor) -> BoxFuture<'a, Vec<Cursor>>;

    /// Sub-grammar for this op's paren-slot body. None = non-pattern op.
    fn language(&self) -> Option<tree_sitter::Language> { None }

    /// Highlight queries for the sub-grammar.
    fn highlights(&self) -> Option<&'static str> { None }
}

pub trait PatternOp: Op {
    /// Compile parsed sub-tree into executable matcher.
    fn compile(&self, tree: &Tree, bytes: &[u8])
        -> Result<CompiledPattern, Diagnostics>;

    /// Capture names declared in the parsed sub-tree (v2 binds_captures port).
    fn binds_captures(&self, tree: &Tree) -> Vec<Arc<str>>;

    /// Hover body for a pattern-local node kind (e.g. re's char_class).
    /// `term_ref` hover is framework-owned; this is for match-kind nodes.
    fn hover_match(&self, node: Node, cursors: &[Cursor]) -> Option<String> { None }
}
```

`#[sprf_pattern_op(name = "glob")]` proc-macro fills in `language()`
and `highlights()` from sibling files:

```rust
#[sprf_pattern_op(name = "glob")]
pub struct GlobOp;

// proc-macro expands to:
// impl Op for GlobOp {
//     fn name(&self) -> &'static str { "glob" }
//     fn language(&self) -> Option<Language> { Some(op_languages::GLOB.into()) }
//     fn highlights(&self) -> Option<&'static str> {
//         Some(include_str!("queries/highlights.scm"))
//     }
// }
```

### 14.6 Framework glue — zero per-op LSP code

Hover dispatcher lives once in the LSP adapter:

```text
hover(byte_pos):
  1. host.descendant_for_byte(pos)
  2. walk up until inside a paren_slot of an op_invocation
  3. op = registry.lookup(op_name)
  4. lang = op.language()?                       # skip non-pattern ops
  5. injected = parse_injected(slot_bytes, lang)
  6. node = injected.descendant_for_byte(pos)
  7. match node.kind() {
       "term_ref"      => BindingGraph hover (framework-owned),
       "carveout_expr" => recurse into host dispatch,
       _               => op.hover_match(node, cursors),
     }
```

Lower-time parsing mirrors this:

```text
lower:
  for each op_invocation:
    injected = parse_injected(paren_bytes, op.language())
    captures = op.binds_captures(injected)
    pattern  = op.compile(injected, bytes)?
    push Pipeline::Op(LoweredOp { op, pattern, captures, parse_site })
```

Neither hover nor lower contains pattern-DSL-specific code. Adding a
new pattern op = new folder, plus registry entry. Framework core does
not change.

### 14.7 Hole mechanic, grounded

`$NAME` inside a pattern body is a **term_ref CST node** in the op's
injected tree. The op's `compile()` walks its tree, finds term_ref
nodes, and desugars them to the engine's native metavariable:

| op | how `$NAME` desugars at compile |
|---|---|
| `glob` | a `Segment::Hole { name }` in the compiled glob AST; match fills `captures[NAME]` with the matched path segment |
| `re` | rewritten to `(?P<NAME>.*?)` (non-greedy default) before compilation; on match, `captures[NAME] = regex.name("NAME")` |
| `ast` | passed through as ast-grep native metavar `$NAME` (already that engine's language) |
| `json` | walker-step capture field, per v2 `_5_json.rs:144-157` |
| `str` | rejected; emit `pattern/str-forbids-hole` diag |

Native engine-side capture syntax stays available where applicable:
`re((?<X>\w+))` and `re($X)` are equivalent, both surface in
`binds_captures(tree) -> ["X"]`.

### 14.8 Diagnostics at parse phase

With ops owning their sub-grammars, pattern-syntax errors fire at
**parse phase**, not lower. The op's grammar produces ERROR / MISSING
nodes; `sprefa_parse::host_parse` walks them via `collect_errors` the
same way it does for host errors.

New codes:

| code | phase | where |
|---|---|---|
| `pattern/str-forbids-hole` | parse | §14.2, `str(...)` containing term_ref |
| `pattern/<op>-syntax` | parse | tree-sitter ERROR inside op's injected tree |
| `pattern/<op>-missing` | parse | tree-sitter MISSING inside op's injected tree |

### 14.9 Dogfood path

Phase 1 — manual (near-term landing):
- `pipeline/build.rs` + `op_languages.rs` generator
- `tree-sitter-sprefa/shared/tokens.js` + drift-grep
- `ops/str/`, `ops/glob/`, `ops/re/` folders with hand-written grammar
- `#[sprf_pattern_op]` proc-macro
- host `injections.scm` regeneration
- injected-tree support in `sprefa_parse::host_parse`

Phase 2 — dogfood:
- a sprf rule walks `pipeline/src/ops/*/`, finds `#[sprf_pattern_op]` sites
- reads sibling `grammar.js`, drift-checks shared tokens, regenerates
- LSP code-action "register new pattern op" drops a skeleton folder +
  reruns build via the same `MutationEffect` machinery as §32 type
  transclusion
- `render_into_marker` writes the regenerated `injections.scm`

### 14.10 Removes the v1 weirdness

- `re:pattern` prefix strings → gone; regex lives in `re(...)`.
- `glob("derp/$$$PATHS/x")` triple-sigil → gone; `$PATH` inside
  `glob(derp/$PATH/x)` is the hole (ordinary term_ref node).
- Capture groups forced to SCREAMING → still uppercase by convention,
  falls out of §5 uniformly.
- String-literal-with-hole parsing → gone; no string-level `$NAME`
  lexing. Every hole is a CST node in an op-owned sub-tree.

---

## 15. Term annotations — open exploration lane

Reserved at the grammar tier; semantics unpicked. This is where
arbitrary linking / tagging / annotating may eventually live.

### 15.1 Motivation

A term reference carries more than a name. It may also carry:

- **mode** — must-be-bound / must-be-unbound / either
- **kind** — the scan-pointer-like class (repo, rev, fs, norm, ...)
- **link directive** — bind this term to another term by rule
- **scope modifier** — escape to ancestor, limit to fork arm, etc.
- **persistence directive** — record / skip / annotate-only
- **arbitrary user tag** — op-local convention

Forcing all of these into one sigil (`$`) loses discriminability.
Forcing each into a new sigil grows the language. A term-annotation
grammar is the compromise: one extra character-class after `$NAME`,
parsed into a structured annotation AST.

### 15.2 Candidate shapes (not locked)

```
$NAME:atom                   # atom annotation (uses existing `:` grammar)
$NAME@kind                   # @-prefixed kind sigil
$NAME!mode                   # !-prefixed mode sigil
$NAME(annotation_expr)       # paren-carved sub-expression
$NAME{annotation_body}       # brace-carved annotation block
$NAME[kind, mode, link]      # bracket annotation list
$NAME :: kind :: mode        # prolog-ish cascading annotations
```

| shape | grammar cost | readability | extensibility | collisions |
|---|---|---|---|---|
| `$NAME:atom` | zero (reuses atom) | low for chained | single-slot | none |
| `$NAME@kind` | new sigil | medium | single-slot | bash-ish |
| `$NAME!mode` | new sigil | low | single-slot | yaml-like |
| `$NAME(...)` | parse ambiguity with op call | medium | high | risky |
| `$NAME{...}` | parse ambiguity with fork | medium | high | risky |
| `$NAME[...]` | conflicts with slot bracket | low | high | collision with `ast[lang]` unless terms can't appear in op-head position |
| `$NAME :: ...` | lex-level new token | high | high | mercury/haskell lineage |

### 15.3 Scope of what annotation could carry

1. **Mode** — `:bound`, `:free`, `:either`. Derived by default; explicit overrides derivation.
2. **Kind** — scan-pointer kinds (`:repo`, `:rev`, `:fs`, `:repo_norm`). Could make `is_repo($R)` redundant.
3. **Link** — `$X linked_to $Y`. Writes a relation row at binding time.
4. **Persistence** — `:persist`, `:ephemeral`, `:annotate`.
5. **Arbitrary user** — namespaced user tags (`:user/important`).

### 15.4 Design questions (all open)

- Do annotations compose? (`$X:bound:repo:persist` or `$X[bound, repo, persist]`)
- Do annotations run at bind time or reference time?
- Are annotations write-once at declaration or mutable through the chain?
- Do annotations participate in mode derivation or override it?
- Is there a default annotation set per rule or per op?
- How do LSP hover and completion surface annotations?
- Can user-defined tag-ops and link-ops be absorbed into annotations, reducing the op surface?

### 15.5 Relation to the rest of the system

If annotations can carry kind + persistence directives, the tag-op
family shrinks or vanishes. If annotations can carry link directives,
the link-op family shrinks or vanishes. If annotations carry only
mode, they're a narrower feature and tag/link ops stay.

Pick a shape before writing grammar.js extensions.

---

# Part IV — Semantics

## 16. Phase ordering: parse, lower, run

Three phases, three diagnostic classes.

| phase | input                       | output             | diag class |
|-------|-----------------------------|--------------------|------------|
| parse | source bytes                | OpInvocation tree  | parse diag |
| lower | OpInvocation + op registry  | Pipeline + graphs  | lower diag |
| run   | Pipeline + runtime ctx      | RunEvent stream    | run diag   |

16.1 **Parse** builds syntax via tree-sitter. Each op's `parse` hook
drives sub-grammar parsing. Parse errors carry byte-range + structured
kind (see §26).

16.2 **Lower**:

- resolves every name to `EntityRef`
- builds `BindingGraph: HashMap<TermPath, Vec<BindingSource>>` (§19)
- builds `StageDeps: { reads, writes, path }` per rule stage (§23)
- checks ArgSpec vs call-site modes (§18)
- detects rule cycles via Tarjan on the call graph

16.3 **Run** executes lowered `Pipeline` against the runtime. Unbound
term-refs park cursors; upstream close drops them (never throw).

---

## 17. Rule = named Pipeline with params

`rule` is the single declaration form. Params are unbound terms in the
signature. Zero params is the degenerate case.

```sprf
rule(classes) > ast[rust] { class $NAME }                # zero params; runs on subscribe
rule(used_by, $CLASS) > ast[rust] { new $CLASS() }       # one param; lazy until call

rule(audit) {
  > classes
  > used_by($NAME)     # binds $CLASS = $NAME at call site
}
```

### 17.1 Rule type

```rust
struct Rule {
    name:   Atom,
    path:   SprfPath,
    params: Vec<TermPath>,
    body:   Arc<Pipeline>,
    schema: RowSchema,
}
```

- `params.is_empty()` → auto-subscribed by runner, persists to `rule_<path>`.
- `params.len() > 0` → parametric; subscribed via call-site references. Table columns = `arg_<param>` + capture columns.
- `op` keyword removed — rules subsume reusable op definitions. User-defined ops in Rust remain the Rust op surface.

### 17.2 No shadowing (for now)

Rule names may not reuse any in-scope built-in op or other rule name.
Resolver emits duplicate-declaration diagnostic. Re-open if needed.

### 17.3 Recursion

Self-referencing rules require `@recursive(max_depth=N)` attribute.
Without it, resolver emits cycle diagnostic. Cycle detection via
Tarjan on the rule-call graph at lower time.

---

## 18. Arg-mode dispatch

Per-op, per-arg ArgSpec declared by op author. Resolver checks at call
site. Runtime dispatches.

```rust
struct ArgSpec {
    name: Atom,
    accepts: AcceptsMode,
}

enum AcceptsMode {
    BoundOnly,            // error if unbound at call site
    UnboundOnly,          // error if bound
    Either,               // op dispatches per mode
}

enum TermMode {
    Bound(Value),
    Unbound,              // with SlotKey for write-back
}

trait ArgModeDispatch {
    fn dispatch(&self, ctx: OpCtx, modes: &[TermMode], cursor: &Cursor) -> OpAction;
}

enum OpAction {
    EmitBound(Cursor),
    IterateRegistry(BoxStream<Arc<Cursor>>),
    Filter,
    Diagnose(Box<dyn Diagnostic>),
}
```

### 18.1 Rule mode derived from body

Resolver walks rule body, collects per-param constraints from op
ArgSpecs. Propagation produces per-param derived mode at rule
declaration.

Example: rule `r` whose body calls `tag($PARAM, :kind)` — tag requires
arg 0 bound; therefore `r`'s `$PARAM` is derived as `BoundOnly` at
call site.

### 18.2 Rule mode: explicit annotation

Reserved for §15 term-annotations lane. Not locked.

---

## 19. Binding resolution — three phases, five sources

| phase | what's checked | failure mode |
|---|---|---|
| parse | syntactic well-formedness, `$TERM` / `${...}` / `&.` valid | parse diag |
| lower | binding-source DAG complete, ArgSpec vs call-site modes, rule-mode derivation, cycle detection | lower diag |
| run | cursor backpressure on missing terms | drop on upstream close (silent trace), never throw |

### 19.1 Binding sources (five kinds)

```rust
enum BindingSource {
    Param(RuleId, usize),
    WalkerBody(OpCallId),
    ChainStageEmit(StageId),
    ForkArmAncestor(ArmId),
    ParametricCallProducer(CallSiteId),
}

struct BindingGraph {
    sources: HashMap<TermPath, Vec<BindingSource>>,
}
```

Resolver builds `BindingGraph`. Every `$TERM` reference must have a
non-empty source vector.

### 19.2 Runtime wait semantics

| state at op entry | outcome |
|---|---|
| all required terms bound | op runs, emits downstream |
| term missing, upstream emitting | cursor parks (backpressure) |
| term missing, upstream closed | cursor drops |

Never throw for unbound at runtime. Static check in phase lower
prevents indefinite waits.

---

## 20. Control flow and fork intersection

No new syntax. Four mechanisms cover everything.

1. **Fork arms** for branching: `{ > A; > B; }`.
2. **Bare `$X` in chain position** as semijoin — drops cursors lacking `$X`. Relation ops (`eq`, `gt`, `lt`, `in`) emit zero or one cursor and compose the same way. No separate `filter(cond)` primitive.
3. **Recursive rules** for looping (with `@recursive(max_depth=N)` opt-in).
4. **Higher-order control ops** as Rust ops taking Pipeline args.

### 20.1 Fork capture semantics — intersection, not union

When a Fork `{ A ; B }` emits, downstream stages see only captures
present in **all** arms. A cursor from arm 0 lacking arm 1's bindings
cannot safely be consumed by a downstream op that expects both. Static
checker computes `Γ_A ∩ Γ_B`; any downstream reference to a capture
not in the intersection produces a lower-phase diagnostic.

Rationale: bash `wait` returns the exit status of the last foreground
job; sprefa Fork is parallel composition with a meet-semilattice
merge. Union would permit unsound downstream references.

### 20.2 Higher-order control op table

| op | signature | meaning |
|---|---|---|
| `retry(body, n)` | Pipeline × Number → Pipeline | up to n retries |
| `until(body, cond)` | Pipeline × Pipeline → Pipeline | repeat until cond emits |
| `while_changes(body)` | Pipeline → Pipeline | fixpoint; repeat while output changes |
| `when(cond, body)` | Pipeline × Pipeline → Pipeline | run if cond emits, else empty |
| `if_else(cond, then, else)` | Pipeline × Pipeline × Pipeline → Pipeline | per-cursor branch |
| `debounce(body, ms)` | Pipeline × Number → Pipeline | rate-limit re-emissions |
| `distinct_by(body, term)` | Pipeline × TermRef → Pipeline | dedupe by term_path value |
| `merge_by_key(stream, term)` | Pipeline × TermRef → Pipeline | rxjs mergeByKey on term_path |

---

## 21. Lazy / subscribe policy

Pipelines are cold by default. A Pipeline value is a description;
execution starts at subscribe.

```rust
enum SubscribePolicy {
    Cold,                        // default, re-run per subscribe
    Shared { store_key: Key },   // first subscriber materializes to Store; later subscribers join
    Memo { ttl: Duration },      // shared + expiry
}
```

Defaults:
- Top-level zero-param rules: `Shared` (auto-materialize to their sqlite table).
- Parametric rules: `Shared` per (rule, args-tuple) key.
- Anonymous pipelines: `Cold` unless `@memo` attribute.
- Higher-order ops: policy is op-local (retry wants Cold; cache wants Memo).

---

## 22. Runtime model — mergeByKey

Every cursor emission is a keyed event. Key = `(rule_path, term_path)`.
Store row is the merged state per key. Downstream observers see deltas.

| rxjs | sprf |
|---|---|
| Cold Observable | Pipeline (default) |
| Hot Observable | Pipeline with Shared subscribe policy |
| Subject | Relations tier |
| BehaviorSubject | Capture tier row per term_path |
| ReplaySubject | Evidence tier rows per stage |
| mergeByKey | `merge_by_key` op on term_path |
| combineLatest | merge_by_key across multiple source rules |
| debounce | `debounce(body, ms)` op |
| distinct | `distinct_by(body, term)` op |
| subscribe | runner subscribes top-level rules; wrappers subscribe arg-pipelines |
| unsubscribe | CancellationToken fires, TaskGuard drops |
| backpressure | bounded mpsc channels; term-level parking |

Differential dataflow semantics under the hood. Reparse invalidates by
term_path; retract + reinsert per delta.

---

## 23. Dagging — StageDeps

Resolver builds per-rule stage dependency graph:

```rust
struct StageDeps {
    reads:  Vec<TermPath>,
    writes: Vec<TermPath>,
    path:   SprfPath,
}
```

Used by:
- runner for scheduling (stages with satisfied reads run parallel)
- LSP for block-point diagnostics ("waiting on `classes.NAME`")
- persistence for sprfpath completion (all required terms bound → row insert)
- fork-arm parallelization when DAGs don't cross

A cursor's sprfpath is incomplete until all parameter bindings are
present. Persistence writes only complete sprfpaths. Partial flows
live as parked cursors.

---

# Part V — Persistence + effects

## 24. Relations tier

| tier | table | written by | when |
|---|---|---|---|
| capture | `rule_<path>` | rule's Pipeline | per emitted cursor |
| evidence | `rule_<path>_evidence_<stage>` | framework | auto-tap before filter |
| relations | `relations` | tag-ops, link-ops, scan-pointer ops | cursor-pass-through side effect |
| violations | `violations_<check>` | check ops | per SQL row returned |

### 24.1 Relations schema

```sql
CREATE TABLE relations (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL,
    src_rule TEXT NOT NULL, src_row INTEGER NOT NULL, src_field TEXT,
    dst_rule TEXT, dst_row INTEGER, dst_field TEXT,
    via_rule TEXT NOT NULL,
    repo TEXT NOT NULL, rev TEXT NOT NULL,
    payload BLOB
);
```

### 24.2 Tag-op family contract

| aspect | contract |
|---|---|
| input | cursor with referenced captures bound |
| reads | `captures[$NAME]` per arg |
| writes | `relations` row with kind + src/dst |
| cursor | passes through unchanged |

Tag-ops: `is_repo($R)`, `is_rev($T)`, `is_fs($F)`, `is_repo_norm($R)`,
`is_rev_norm($T)`. Each owns kind, diagnostic, writer logic.

### 24.3 Link-op family contract

| aspect | contract |
|---|---|
| input | cursor with two or more captures bound |
| reads | captures[src], captures[dst] per arg |
| writes | `relations` row carrying both sides |
| cursor | passes through unchanged |

Link-ops: `link(:kind, $A, :other_kind, $B)`, `depends_on($A, $B)`,
`generated_from($DST, $SRC)`.

Scan-pointer (§12) is a relation of known `kind` (repo, rev, fs,
repo_norm, rev_norm). No syntactic class. Set by scan-pointer ops at
chain positions; reads `cursor.last_bound` when arg is elided.

---

## 25. Mutation effects — four optional slots

```rust
trait MutationEffect: Send + Sync {
    // required
    fn kind(&self) -> &'static str;
    fn apply(&self, ctx: &ApplyCtx) -> ApplyResult;

    // optional
    fn preview(&self) -> Option<Preview> { None }
    fn reversible(&self) -> bool { false }
    fn reverse(&self, ctx: &ApplyCtx) -> ApplyResult { unreachable!() }
    fn source_range(&self) -> Option<SourceRange> { None }
}

enum Preview {
    Diff { before: String, after: String },
    Textual { summary: String },
    Custom(Box<dyn Fn() -> Markdown>),
}
```

### 25.1 Approval flow

1. Op queues `Arc<dyn MutationEffect>` on ctx mpsc.
2. Runner drains; handler dispatches by `ApprovalPolicy`.
3. `LspPromptBridge` emits `RunEvent::MutationPrompt` + publishes LSP diagnostic at `source_range` with code actions `[Approve, Preview, Skip]`.
4. Approve → `apply()`. If `reversible`, framework records handle for undo; LSP shows `Undo` action post-apply.
5. Skip → drop, no apply.
6. CLI `InteractiveCli` prints `preview()` if Some; shows y/n/d prompt.
7. `AutoApprove` skips gate; `preview()` not called; `source_range` unused.

---

# Part VI — Tooling

## 26. Diagnostics surface

26.1 Core trait:

```rust
trait Diagnostic: Send + Sync {
    fn code(&self)     -> &'static str;
    fn severity(&self) -> Severity;
    fn primary(&self)  -> &ParseSite;
    fn render(&self, out: &mut dyn Renderer);
}

trait Renderer {
    fn primary(&mut self, site: &ParseSite);
    fn related(&mut self, site: &ParseSite, message: &str);
    fn note(&mut self, message: &str);
}
```

26.2 Parse-layer errors carry byte_range + structured kind. Offset-only
messages are a v2 regression and are forbidden:

```rust
struct ParseError {
    kind:       ParseErrorKind,
    byte_range: Range<usize>,
    message:    Arc<str>,
}

enum ParseErrorKind {
    SyntaxError,
    Missing { expected: Arc<str> },
}
```

26.3 Diagnostic codes introduced by this spec:

| code                          | phase  | where                              |
|-------------------------------|--------|------------------------------------|
| `bare-dollar-without-target`  | parse  | §7.2                               |
| `xref/empty-join`             | run    | §9                                 |
| `unresolved-term-ref`         | lower  | §19, no binding source             |
| `capture-write-bad-position`  | parse  | §10, `$X` in non-chain-step spot   |
| `fork-capture-not-in-intersection` | lower | §20.1                          |
| `pattern/str-forbids-hole`    | parse  | §14.8, term_ref inside `str(…)`    |
| `pattern/<op>-syntax`         | parse  | §14.8, ERROR in injected sub-tree  |
| `pattern/<op>-missing`        | parse  | §14.8, MISSING in injected sub-tree |

---

## 27. Op authoring surface — min-viable

Four trait slots minimum:

| row | what |
|---|---|
| A1 | `NAME: &str` + `inventory::submit!` |
| A2 | `parse(&OpInvocation, &mut ProgramCtx) -> Result<Pipeline, Vec<Box<dyn Diagnostic>>>` |
| C1 | `pipe(OpCtx, BoxStream<Arc<[Cursor]>>) -> BoxStream<Arc<[Cursor]>>` |
| C6 | one `CaptureKind` impl (if op emits captures) |

Everything else defaults.

### 27.1 Framework affordances

- `ProgramCtx::register_invocation(kind, range, path)` — op-invoked at parse time to populate its own registry.
- `ArgValue::TermRef { term_path, slot_key }` — new ArgValue variant.
- `OpCtx::resolve_term_modes(cursor, &[ArgValue]) -> Vec<TermMode>` — helper for arg-mode dispatch.
- `Operator::signature() -> Vec<ArgSpec>` — per-arg mode declaration.
- `MutationEffect::{preview, reversible, reverse, source_range}` — four optional slots.
- `SubscribePolicy` enum + `Pipeline::subscribe(policy)` wrapper.
- `StageDeps` computed per rule at lower; consumed by runner + LSP.
- `BindingGraph` computed per rule at lower; used for diag + completion.
- `Rule { params, schema with arg_<param> columns }` — parametric rule type.

---

## 28. First-pass implementation scope

### 28.1 Land together

1. §7.2 lex rule for unified `$`, including `Carveout` token emission.
2. §7.3 balanced-brace pre-pass at lex time with carveout-kind classification.
3. §7.4 `${{...}}` shell escape.
4. §7.5 eager parser re-entry producing `Arc<Pipeline>`.
5. §8 `CarveoutOp` with narrow + inner + rebase runtime.
6. §9 `rule.$V` as ordinary dotted-access host_expr.
7. §10 `CaptureWriteOp` for bare-`$IDENT` chain step (annotate-only).
8. §11 `VoidOp` registration.
9. §12 scan-pointer op skeleton reading `cursor.last_bound`.
10. §14 ops-own-grammar infra: `pipeline/build.rs`, `op_languages.rs` codegen, `tree-sitter-sprefa/shared/tokens.js`, `#[sprf_pattern_op]` macro, `ops/{str,glob,re}/` folders, host `injections.scm` regeneration, injected-tree support in `sprefa_parse::host_parse`.

### 28.2 Explicit non-goals for first pass

- parametric rule params (`rule(:name, $P1, $P2)`)
- `assert(:name) { ... }` decl
- `@recursive` attribute
- casing enforcement (§5.3)
- `&{computed}` address-grammar carveout
- higher-order control ops (retry, until, when, if_else, debounce, distinct_by)
- term annotations (§15)

### 28.3 Tests to reactivate

- `rule_repo_rev` rewritten against `> $VAR + is_repo($VAR)`.
- `kitchen_sink.sprf` migrated to `rule(:name)` atoms + new carveout shape.
- Targeted new tests per §7, §8, §10, §11, §14.

---

# Part VII — Meta

## 29. What was discarded

| discarded | replaced by |
|---|---|
| `$$repo($R)` / `$$rev($T)` scan sigil class | tag-op family `is_repo($R)` / `is_rev($T)` (§12) |
| `$$` Ans-slot sigil | scan-pointer op reads `cursor.last_bound` directly (§12) |
| `$$sigil` migration diag | dead; v1→v3 source must be rewritten |
| `&&.rules` registry sigil | op-mediated registry via parametric call `rule($R)` |
| Separate `op` keyword for reusable defs | `rule(name, $TERM*)` subsumes |
| `${rule.$V > $NEW}` rename via `>` inside carveout | capture-write station `> $NEW` as ordinary chain op (§10) |
| Scan-pointer as sigil class | relation rows with known `kind` (§24) |
| `$NAME` vs `${NAME}` duality | `$NAME` canonical; `${...}` is the carveout expr-hole |
| Separate `Rule` vs `Op` runtime | Rule is a named Pipeline; collapse (§17) |
| Norm as dedicated syntax | `is_repo_norm($R)` tag-op |
| Bash-style `>&N` content redirect | capture-write op + prolog mode |
| Full unification / SLD resolution as framework concern | shallow prolog only (mode dispatch); embed deep logic in `prolog(...)` op if ever needed |
| `re:pattern` prefix strings (v1) | `re("…")` with `$NAME` holes (§14) |
| `glob("…/$$$PATHS/…")` triple-sigil (v1) | `fs("…/$PATH/…")` single hole (§14) |

---

## 30. Open items

1. **Term annotation grammar** — §15. Pick shape before grammar.js extensions.
2. **Anonymous pipeline AST normalization** — LOCKED 2026-04-20: blake3 over canonicalized AST (whitespace strip, var rename, comment strip). Scheme details TBD at implementation time.
3. **Rule recursion annotation spelling** — `@recursive(max_depth=N)` vs `rule(..., :recursive(5))` vs attribute form.
4. **Shadowing policy across scopes** — current lock is no-shadowing same-scope. Cross-scope is undecided.
5. **Parametric rule call site subpath** — does `foo(:a).arm_0` differ from `foo(:b).arm_0` in path? Probably yes, args baked in.
6. **Fork arm path naming when unnamed** — positional `arm_N` locked; could use content hash. Minor.
7. **Evidence tables for anonymous pipelines** — default on. Opt-out via `@ephemeral`.
8. **`merge_by_key` as first-class op vs SQL-derived** — locked as both coexist; op for hot path, SQL for cold queries.
9. **Subscribe-policy per-call vs per-definition** — currently per-definition; call-site override not designed.
10. **Undo scope** — stack-of-effects vs targeted by id. Stack is simpler; targeted more powerful.
11. **`last_bound` storage type** — `Option<Arc<str>>` (current) vs `Option<SlotKey<()>>` (typed). First pass: `Arc<str>`.
12. **`CarveoutOp` narrowing** — always lexical `source_range`, or can inner ops replace target? First pass: always lexical.
13. **Rule body `last_bound` reset** — probably reset at parametric-rule body entry; confirm when calls land.
14. **Scan-pointer table schema** — today's `relations` table is universal; split if volume warrants.

---

## 31. Invariant count summary

- 6 concepts: Op, Rule, Pipeline, Cursor, Capture, Scalar
- 4 Pipeline cases: Op, Chain, Group, Fork
- 5 EntityRef cases: Scalar, Op, Pipeline, Rule, Capture
- 7 Cursor fields: path, content, byte_range, slots, captures, parent, last_bound
- 3 sigils: `$`, `&`, carveouts `${...}` / `&{...}`
- 6 invariants: ops-own-everything, cursor-is-flow, content-contract, reads-pipe/writes-deferred, reparse-cancel, cursor-has-path
- 4 persistence tiers: capture, evidence, relations, violations
- 3 diagnostic phases: parse, lower, run
- 5 binding sources: param, walker-body, chain-stage, fork-arm, parametric-call
- 4 Pipeline run policies: Cold, Shared, Memo (+ op-local)
- 4 min-viable op trait slots: name, parse, pipe, capture-kind

---

## 32. Future: type transclusion

32.1 Every Rust snippet in this spec is a hand-pasted copy of code in
the crate. Drift between this doc and the types is a near-certainty
with manual maintenance.

32.2 Target shape: this file declares anchor-bracketed transclusion
regions whose contents are rendered from the corresponding Rust entity
by a sprf rule. The daemon keeps them in sync; changes flush through
the mutation effect pipeline (§25).

32.3 Anchor protocol (proposed):

```markdown
<!-- sprf:type Cursor path=v3/crates/pipeline/src/_0_cursor.rs -->
\`\`\`rust
pub struct Cursor { ... }
\`\`\`
<!-- /sprf:type -->
```

32.4 Rule that drives it (sketch):

```sprf
rule(doc_type_sync) {
  > fs("**/parse.md")
  > marker(sprf:type, $ENTITY_NAME, $ATTRS)
  > &{ path_of($ATTRS) }
  > ast[rust] { struct $ENTITY_NAME { $$$BODY } }
  > render_into_marker
}
```

32.5 Mechanics that land this: `marker` op (planned), `render_into_marker`
as a `MutationEffect` implementer, approval via the existing
MutationHandler family, bidirectional edits gated by the same approval
surface.

32.6 Acceptance: this file is the zero-th test case. When transclusion
lands, every Rust snippet here becomes generated; drift diagnostics
fire when the source diverges.

---

## 33. Reading order

1. This file (language spec).
2. `v3/docs/v3-semantic-model.md` — semantic essay + binding calculus.
3. `v3/docs/v3-plugin-author-surface.md` — op-author A/B/C/D table.
4. `v3/docs/v3-min-author-ops.md` — the metric.
5. `v3/docs/v3-vs-v2-reading-preview.md` — cross-version reader notes.
6. `v2/docs/_b_v3-unified-language.md` — teaching doc (historical; stale in places).

---

*End of language spec. Update alongside the code; drift is worse than redundancy.*
