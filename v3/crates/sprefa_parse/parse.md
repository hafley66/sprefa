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

## Project-wide conventions (binding for all contributors)

### Test style

- **Consolidate common setup.** When two or more tests share a compile
  / parse / fixture prefix, merge them into one test function with
  multiple `assert!` / `assert_eq!` per scenario. Put a short `//`
  comment above each assert block describing what it checks. One
  `#[test]` per *fixture*, not per *assertion*.
- **No one-assert tests** that only vary their expected output over a
  shared setup. They fragment the signal and inflate the file.
- Inline snapshots (`insta::assert_snapshot!` / `toMatchInlineSnapshot`)
  are preferred when the output is structured.
- `assert!(x.is_some())` / `toBeDefined` style is forbidden. Assert the
  exact value.

### Communication style (for agents and contributors writing prose)

- No em dashes.
- No LLM-common fluff: "you're absolutely right", "not X, it's Y" in
  any form including across sentences, "this isn't X; it's Y".
- No personhood pronouns; act like a textbook that is awake.
- No rhetorical closes, no drop-the-mic, no framing as inevitable or
  novel, no lineage positioning.
- No negative parallelism (`not X. Y.` pattern).
- No banned-word lists; write positive posture instead.

These apply to every text file under `v3/` including chat_log entries,
inline comments, and generated docs.

---

## Pattern-arg interchangeability (worked example)

`re(...)`, `glob(...)`, `ast[lang](...)`, `json(...)` all compile to
the same `Value::Op(Arc<dyn Op>)` (§4.3, §14.5a; post-Pass-A of §14.5m).
Any op slot declared `ArgSpec::Op` accepts any of them — the outer op
inspects via the capability surface (`try_raw_regex`, `bound_captures`,
`materialize_with`) without pattern-matching on variants or downcasting
to concrete types. A `$NAME` inside a pattern body is a hole (term_ref);
holes bound upstream fall into *read mode*, unbound holes into
*write mode* (§14.5b, §18.3).

### Example A — glob at the fs slot, re at the json key slot

```
fs(glob($A/$B/*/$FILE.json))
  > json({
      ${re((devD|optionalD|peerD|d)ependencies) > $DEP_KIND}:
        { $DEP : $VER }
    })
```

Slot-by-slot:

- `fs(glob(...))` — fs's arg slot is `ArgSpec::Op`. The glob op
  compiles to a `GlobOp` with holes `$A`, `$B`, `$FILE` and unbound_re
  `[^/]+`; fs reads `op.try_raw_regex()` and drives the ignore-crate
  walker with the anchored path regex.
- `$A/$B/*/$FILE.json` — three holes + one unbound wildcard segment.
  Each match binds `$A`, `$B`, `$FILE` on the emitted cursor.
- `> json({...})` — json pattern at pipe-step position; its body is
  a structural json template.
- `${re(...) > $DEP_KIND}` — carveout slot holding another pattern
  op chained into a term bind. re's hole-bodied group captures the
  matched key into `$DEP_KIND`; `$DEP` and `$VER` are bound by the
  surrounding json template at the leaf level.

### Example B — re at the fs slot, glob at the json key slot

```
fs(re($A/$B/[^/]+/$FILE\.json))
  > json({
      ${glob($DEP_KIND) > $DEP_KIND}:
        { $DEP : $VER }
    })
```

The swap is mechanical: each pattern slot accepts whichever pattern
op's grammar is natural for the shape. Outer ops never downcast to
glob-specific or re-specific types; they pattern-match the `Pattern`
enum and, when they need raw bytes to hand to a bulk backend, call
`Pattern::as_raw_regex()`. Bodies that use operators exclusive to
one grammar (glob `**`, re `|`) stop being literal-swappable, but
the slot stays the same.

### Example C — cross-repo dep graph with scan pointers

```
fs(re($A/$B/[^/]+/$FILE\.json))
  > json({
      ${glob($DEP_KIND) > $DEP_KIND}: {
        ${$DEP > is_repo_norm($DEP_NORM)} : ${$VER > is_repo_rev_norm($DEP_NORM)}
      }
    })
```

Motivation. Semver ranges (`^1.2.3`, `~3.x`) cannot address a
concrete git tag or SHA. Cross-repo joins that need literal rev
values read `package-lock.json`, since it records the resolved ref
per dependency rather than a range expression.

Cross-repo walk. `fs(re(...))` fans over every repo in the corpus
(§14.5c, repo dimension). `$A` and `$B` bind repo-owner and
repo-name segments; the `[^/]+` segment catches intermediate
directories; `$FILE` binds the lockfile stem. `cursor.repo` and
`cursor.rev` come along as Synthesized captures (§14.5c bind mode).

Scan pointer.

- `${$DEP > is_repo_norm($DEP_NORM)}`. `$DEP` is bound by the
  surrounding json template (the dependency name). `is_repo_norm`
  receives `$DEP` in read mode, normalizes it (lowercase, strip
  punctuation), and writes the result into `$DEP_NORM`. `$DEP_NORM`
  is a scan pointer: a canonical handle usable as a lookup key in
  sibling slots and downstream steps.
- `${$VER > is_repo_rev_norm($DEP_NORM)}`. `$DEP_NORM` is already
  bound by the sibling slot. `is_repo_rev_norm` receives it in read
  mode, addresses the cross-repo rev index with it, and emits the
  matched rev literal alongside a scan pointer back to the
  originating lockfile row.

Arg-mode dispatch (§18.1).

- `is_repo_norm(X)`: X absent on input ⇒ write mode. Rule body
  binds X before the row leaves the rule.
- `is_repo_rev_norm(X)`: X present on input ⇒ read mode. Rule body
  reads X, never rebinds.
- Passing one term through both roles in one row is how sprf
  expresses a cross-repo join without a temporary variable table.

Exercised surface. fs cross-repo walk, injected json pattern,
json-template dotted term binding, arg-mode dispatch with a shared
term, rev-index read under a synthesized scan pointer.

---

## Table of Contents

### Part I — Foundations
1. [Scope and layering](#1-scope-and-layering) — incl. §1.5 crate layout, §1.7 op partitioning, §1.9 injection-query ownership
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
14. [Pattern DSLs — ops own grammar, queries, and parse](#14-pattern-dsls--ops-own-grammar-queries-and-parse)
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

### 1.5 Crate layout

Workspace partition. Each crate has one responsibility; boundaries
enforce the invariants.

```
v3/crates/
├── spine/              # foundational shared types (Atom, RowId, etc.)
├── effect_runtime/     # RtCtx, effect dispatch, BoxFuture plumbing
├── tree-sitter-sprefa/ # HOST grammar + shared/tokens.js + generated injections.scm
├── sprefa_parse/       # host_parse, AST, ParseError, injected-tree support (this spec)
├── pipeline/           # Op trait, Cursor, Pipeline enum, ops/<name>/ folders, build.rs
├── server/             # stdio LSP (tower-lsp); parse diagnostics landed, hover/completion stubbed
└── sprefa/ (future)    # runtime binary, HTTP server, watcher, Store
    └── sprefa-cli/     # proxy to sprefa HTTP
```

### 1.6 Dependency arrows

Strict DAG. Every arrow is a Cargo dep.

| crate | depends on | does NOT depend on |
|---|---|---|
| `spine` | (none internal) | anything |
| `effect_runtime` | (none internal) | pipeline, sprefa_parse |
| `tree-sitter-sprefa` | (none internal) | any other v3 crate |
| `sprefa_parse` | `tree-sitter-sprefa` | pipeline, effect_runtime, spine |
| `sprefa_macros` | (none internal) | anything |
| `pipeline` | `effect_runtime`, `sprefa_macros`; builds `ops/*/parser.c` via `cc` | sprefa_parse, sprefa, server, spine, tree-sitter-sprefa |
| `server` | `pipeline`, `sprefa_parse`, `effect_runtime` | sprefa (future) |
| `sprefa` | `pipeline`, `sprefa_parse`, `effect_runtime`, `spine` | sprefa-cli, server |
| `sprefa-cli` | `sprefa` (via HTTP, not Cargo) | server |

Notes on the actual graph as of Stage A:

- `pipeline` has no Cargo dep on `sprefa_parse`. The op layer is parse-
  agnostic; it works against `OpInvocation` only when a caller (server,
  tests) feeds it one. `sprefa_parse` is a dev-dep so integration tests
  in `pipeline/tests/` can exercise the full host-parse → lower path.
- `pipeline` does not Cargo-depend on `tree-sitter-sprefa`. Each op's
  sub-grammar `parser.c` is compiled in-tree by `pipeline/build.rs`
  using `cc`. The host grammar's parser is reached transitively through
  `sprefa_parse` only from consumers that parse `.sprf` sources.
- `spine` is currently a parked foundational crate. No v3 build-path
  consumer imports it. The row in this table is reserved for when the
  row/schema types promote out of in-crate shapes.

`sprefa_parse` knows nothing about ops. It produces `OpInvocation`
nodes with opaque `name: Arc<str>`. Op resolution happens in
`pipeline`. This keeps the parse layer reusable by any consumer.

### 1.7 Op partitioning — one crate, one folder per op

Every op lives under `pipeline/src/ops/<name>/`. Pattern ops own their
sub-grammar in the same folder (§14.1). Non-pattern ops stay as single
files `ops/<name>.rs`.

Rules:

1. **One Cargo crate** for all built-in ops. Each op is a folder or
   file, not a crate. Ten ops = ten folders, not ten `Cargo.toml`s.
2. **`pipeline/build.rs`** only runs `cc::Build` across each op's
   committed `parser.c` (+ optional `scanner.c`). It does **not** run
   `tree-sitter generate`. Regeneration is manual (see §14.4).
3. **Per-op tests** run via `cargo test -p pipeline ops::<name>::`.
   Grammar fixtures live in `ops/<name>/tests/`.
4. **Sub-grammar artifacts are committed.** Each op's `parser.c` (and
   `grammar.json` / `node-types.json` as tree-sitter emits them) lives
   in `ops/<name>/src/` and is checked in alongside `grammar.js`. Same
   convention as `tree-sitter-sprefa` already uses for the host grammar.
   `tree-sitter-sprefa/queries/injections.scm` is likewise hand-maintained.
   YOLO posture — automation lands via dogfood later (§14.9 Phase 2).

### 1.8 When to promote an op to its own crate

| signal | action |
|---|---|
| op's grammar + impl > 5kloc | extract to `sprefa-op-<name>` crate, depend on `pipeline` for `Op` + `PatternOp` traits |
| op has third-party dependencies with version conflicts with pipeline's other ops | extract for isolation |
| op is a plugin (external author, loaded dynamically) | always its own crate, registered via `inventory::submit!` at runtime |
| op is one of the built-in common set | stays in `pipeline/src/ops/` |

The extracted-op pattern mirrors the v2 ast-grep carve-out: the op
trait lives in the core crate; the op impl + grammar lives in a
sibling crate. No framework changes required; the registry picks up
whatever is linked.

### 1.9 Injection query — hand-maintained

`tree-sitter-sprefa/queries/injections.scm` lives in the host grammar
crate (ships with the parser distribution) and is hand-edited. Adding
or removing a pattern op = one-line edit to the `#match?` alternation.

No codegen, no drift-grep, no CI assert. One file, author-maintained.
When Phase 2 dogfood lands (§14.9), a sprf rule will regenerate it
from the op registry via the same `MutationEffect` machinery as §32
type transclusion.

Rationale for living with the host: external tree-sitter consumers
expect queries alongside the parser distribution. Rationale for
manual: the file is ~5 lines; build-time automation is not worth the
cost until the op count justifies it.

### 1.10 Why not split sub-grammars into a separate crate

Considered and rejected:

| option | rejected because |
|---|---|
| each op its own tree-sitter crate (`tree-sitter-sprefa-glob`, …) | 10+ crate explosion for built-in ops; build-time fan-out without corresponding modularity |
| single `tree-sitter-sprefa-patterns` crate containing all sub-grammars | grammars move with op impls; co-locating in `pipeline/ops/<name>/` keeps edit scope local |
| host grammar absorbs pattern grammars via `grammar.extends` | tree-sitter's extension mechanism is primitive; shared-token approach (§14.3) is simpler and scales better |

Ops move as units: Rust impl + grammar + queries + tests ship in one
folder, reviewed as one diff, deleted in one `rm -rf`. That's the
invariant the crate layout optimizes for.

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

4.3 **Value tier.** What an op receives in an argument slot. Closed
algebra. Added 2026-04-21: `Pattern` and `Pipeline` joined the Value
tier so that op arguments carry a uniform typed face, no runtime
downcast to extract op-internal state.

```rust
enum Value {
    Atom(Arc<str>),
    Str(Arc<str>),
    Int(i64),
    Float(f64),
    Term(Arc<str>),          // unresolved $NAME reference
    Pattern(Pattern),        // compiled matcher from glob/re/ast/json at arg position
    Pipeline(Arc<Pipeline>), // any other op chain passed as arg
}
```

4.4 Op membership in Value is what makes "ops pass ops into ops"
first-class without a value/operator split at the surface. A pattern
op at pipe-step position lowers to `PipelineStep`; at arg position it
lowers to `Value::Op(Arc<dyn Op>)`. The outer op's `ArgSpec` per slot
(§18) decides which; the outer op introspects via the capability
surface at arg position.

4.5 Values flow as fields inside cursors. Names reference streams of
cursors. Ops transform streams. One uniform rule per tier.

4.6 Collapse landed Pass A (spike sprefa-x5b §14.5m, 2026-04-22).
`Value::Pattern` and `Value::Pipeline` merged into `Value::Op(Arc<dyn Op>)`.
`Term` survives as a sugar-distinct variant for resolver ergonomics.
Scalars stay as cheap leaves. Consumers read capability methods on the
inbound `Op` (`try_raw_regex`, `materialize_with`, `bound_captures`)
instead of pattern-matching on variants or downcasting to concrete
matcher types. `PatternValue` trait and concrete matcher structs
(`GlobPattern` / `RegexPattern` / `JsonPattern` / `AstPattern`) deleted;
state folded into `GlobOp` / `ReOp` directly. See §14.5m for the full
migration log.

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

12.6 Three shapes (x5b framing, §14.5m.3):

| shape | source when unbound | examples |
|---|---|---|
| produce-or-filter | op has a source to draw from | `fact(:key, $V)`, `fs($P)`, `rev($R)`, `repo($R)` |
| filter-only | no source; asserts on existing cursor state | `is_rev($R)`, `is_repo($R)`, `is_fs($F)` |
| pattern-with-embedded-term | the pattern's compile/apply cycle (§14.5b) | `re($NAME = ...)`, `glob(.../$F)` |

Per-op arg-mode dispatch. Each op reads `cursor.captures` for names
declared in its `bound_captures()` and chooses its branch. Pipe order
is goal order. Shallow prolog, forward-only — no search, no backtrack.

---

# Part III — Sub-grammars

## 13. Sub-grammar lowering (two flavors)

| sub-grammar | lowers to | why |
|---|---|---|
| json, yaml, toml, md | sprf op chain (fork over field-extract ops) | structural, composable |
| ast-grep walker | walker-native rule tree + capture slot decls | opaque, engine-owned |
| regex (as op body) | single `re_match(pattern)` op | opaque, leaf; see §14 for arg-position regex |
| shell | `sh` op with body as literal + carveout substitution | opaque, effect |

`${...}` and `&{...}` carveouts inside any pattern sub-grammar body
are emitted as **`carveout_expr` CST nodes** in the injected tree (via
the shared rule fragment, §14.3). There is no balanced-brace pre-pass
at the byte level. LSP hover dispatches by node kind (§14.6); lower-time
recursion walks `carveout_expr` children back into host-grammar
sub-pipelines via `parse_injected` with the host language.

`sh` double-brace escape `${{var}}` passes literal `${var}` to shell;
this is a shared-rule exception owned by the `sh` op grammar.

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
├── src/
│   ├── parser.c                 # COMMITTED; regenerated via `just` (§14.4)
│   ├── grammar.json             # COMMITTED (tree-sitter emits)
│   └── node-types.json          # COMMITTED (tree-sitter emits)
├── scanner.c                    # OPTIONAL; only if op needs external tokens
└── queries/
    ├── highlights.scm           # editor colorization
    └── injections.scm           # OPTIONAL; for nested DSLs
```

Non-pattern ops (`capture_write`, `void`) remain single-file
`ops/<name>.rs`.

`str` is a pattern op whose sub-grammar is the identity: the paren
body is stored as `Arc<str>` and wrapped as `Pattern::Str` (§14.2).
Its folder is `ops/str/mod.rs` plus the shared PatternOp trait impl.

### 14.2 Surface

Pattern ops live in two positions (§14.2b). At **pipe-step** position
they drive cursors through `Op::pipe`. At **arg-slot** position they
appear inside another op's paren body and lower to `Value::Op(Arc<dyn Op>)`
so the outer op consumes the compiled op via the capability surface.

```sprf
str(literal bytes)                       # identity sub-grammar; paren body = raw bytes
glob(**/$DIR/file.txt)                   # $DIR is term_ref; bare or braced both accepted
re(TODO\($WHO\))                         # $WHO is term_ref; bare or braced both accepted
ast[rust](fn ${NAME}(${{ARGS}}))         # braces required inside ast; see §14.2a
cst[rust]((identifier) @${NAME})         # braces required inside cst; `@` prefix per tree-sitter
json({ pkg: $PKG, version: $V })         # json walker owns its brace grammar
```

Composition (arg-slot position, locked 2026-04-22):

```sprf
fs(glob(**/*.rs))                        # nested op_invocation lowers to Value::Op
repo(glob(myorg/*))
rev(:main)                               # :ident atom sugar — scalar filter literal
rev(str(next/10.1.1))                    # str wrapper required for non-[A-Za-z0-9_$!?] bytes
rev($V)                                  # ERROR: unbounded; terms in filter position require a body
rev(re($ALL_LOL))                        # legal opt-in: explicit regex body with term hole
comment(re(TODO\($WHO\)))
line(glob(**/*.rs))                      # line-level filter
line(re(TODO))
line(str(literal))
```

No raw string literals in arg slots. No bare identifiers in arg slots
(use `:atom`). No raw glob body — glob/re/str must appear as explicit
op calls so each arg is a self-describing Pattern value.

`str` is the **identity sub-grammar**: its `compile` stores the paren
body bytes as an `Arc<str>` on a `StrOp` instance. `$NAME` inside
`str(...)` is literal bytes, not a term_ref. Term binding requires one
of the other pattern ops (glob / re / ast / cst / json).

The `:ident` atom is a lowerer-level shortcut that emits `Value::Atom`.
Ops that expect a scalar filter accept `Atom` and `Str` interchangeably
with a literal-bytes `StrOp`. See §14.5c for rev/repo/fs consumer shapes.

### 14.2a Capture-sigil form per sub-grammar (locked 2026-04-22)

Every sub-grammar parses `term_ref` from `shared/tokens.js` (§14.3),
but not every sub-grammar accepts both the bare `$NAME` and braced
`${NAME}` forms. The decision is per-op and follows a single rule:
if the sub-grammar's surrounding token alphabet can abut identifier
characters, braces are required; otherwise both forms are accepted.

| op    | authored form         | bare `$NAME` | rationale                                                    |
|-------|-----------------------|--------------|--------------------------------------------------------------|
| re    | `$NAME` or `${NAME}`  | legal        | non-dollar literal bytes terminate the sigil naturally       |
| glob  | `$NAME` or `${NAME}`  | legal        | `*`, `?`, `/` terminate; dollar cannot appear in a segment   |
| host  | `$NAME` or `${NAME}`  | legal        | whitespace, `(`, `.`, `>` terminate                          |
| ast   | `${NAME}` only        | rejected     | pattern-by-example abuts idents (`fn$Ax(`); unbraced ambig. |
| cst   | `@${NAME}` only       | rejected     | S-expression parens+idents abut sigils; mirror of ast        |
| json  | `$NAME` or `${NAME}`  | legal        | `,`, `:`, `{`, `}`, `"`, `[`, `]` terminate                  |
| str   | literal bytes, no sigil | n/a        | identity sub-grammar; no term_refs, byte-equality Pattern    |

The `@` prefix in `cst` is tree-sitter's capture marker; compile
strips `@` + `${`/`}` when rendering the actual tree-sitter Query
source, so the engine sees `@NAME`. v2's ast-grep extension
precedent: `${VAR}` sugar was optional in v2 and is now mandatory
in v3 inside ast/cst bodies.

Multi-node subseq capture (ast-grep `$$$NAME`) follows the same rule
with triple-dollar: inside ast, write `$$${NAME}` to capture a
subsequence. Bare `$$$NAME` is rejected for the same ambiguity
reason as bare `${NAME}`.

### 14.2b Pattern-op position duality — pipe-step vs arg

Every pattern op has two legal positions with two distinct lowerings.
Author syntax is identical; the lowerer picks the form by the host-CST
parent of the invocation.

| position                    | lowering                                   | mechanic                                                                                        |
|-----------------------------|--------------------------------------------|-------------------------------------------------------------------------------------------------|
| pipe-step `> ast[rust]{…}`  | `Box<dyn Op>` inside `Pipeline::Op`        | per cursor, calls `Op::pipe`; emits 0..N narrowed cursors                                       |
| arg slot `fs(glob(…))`      | `Value::Op(Arc<dyn Op>)`                   | outer op introspects via capability surface; bulk backends call `op.try_raw_regex()` when valid |

The same inherent `<Op>::compile_from_tree(tree, bytes) -> Self` runs
in both cases. Choice of lowering lives entirely in the host lowerer,
driven by the outer op's `arg_spec` (§18). Slots declaring `ArgSpec::Op`
or `ArgSpec::Any` receive `Value::Op(arc)`.

Invariant: no pattern op branches on "am I a pipe step or an arg?".
`compile` is pure. Position-specific behavior is the outer lowerer's
concern.

### 14.2c Nestable ast/cst — sprf chaining for structural narrowing

ast and cst narrow the cursor to the matched CST subtree. Chaining a
second structural op after the first re-parses that narrowed subtree.
Two authoring forms, one desugaring:

**Linear chain.** Plain pipe:

```sprf
ast[rust] { struct ${NAME} { $$${BODY} } }
  > ast[rust] { fn ${METHOD_NAME}(${{ARGS}}) }
```

The first ast matches structs and narrows `cursor.byte_range` to the
struct body. The second ast runs against the narrowed content. Per
§14.5b content contract, the inner op parses `cursor.content[byte_range]`
first; no re-read from the filesystem. Nothing ast-specific about
this — every pipe-step op composes the same way.

**Brace-block form** — sugar for the nested chain:

```sprf
ast[rust] { struct ${NAME} { $$${BODY} } }{
  > ast[rust] { fn ${METHOD_NAME}(${{ARGS}}) }
  > sql_write{ INSERT INTO methods(...) VALUES (...) }
}
```

The `{ … }` after the pattern body's closing brace holds a nested
pipeline spliced in after the outer match emits cursors. Desugars to
the linear chain; exists for readability when the inner pipe is
visually bound to the outer pattern ("for each struct, extract
methods and write").

**Lowering.** The brace block lowers as a `Pipeline` value the outer
ast's `Op::pipe` runs once per emitted cursor. This is the inverse of
§14.2b duality: at pipe-step position the outer op is driven as an
`Op`, and its brace block is a nested `Pipeline` driven as a value.
Position-duality applies at every level.

**Chaining across ops.** A pattern op at pipe-step position is just
an op. Chaining into a non-pattern op is a plain pipe:

```sprf
ast[rust] { impl ${TRAIT} for ${TY} { $$${BODY} } }
  > sql_write{ INSERT INTO impls(trait, ty) VALUES (${TRAIT}, ${TY}) }
```

`sql_write` is a mutation effect (§25), not a pattern op. It reads
the upstream-bound `${TRAIT}` and `${TY}` captures and writes one row
per cursor. Pattern ops carry no special status in the chain — they
emit cursors, downstream ops consume cursors, and the shape of the
downstream is open.

### 14.3 Shared tokens — one source of truth

Pattern sub-grammars (glob / re / ast / json — NOT str) must emit
`term_ref` and `carveout_expr` identically. Host ships a shared rules
fragment:

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

Drift enforcement is **by convention**, not build-script. Author runs
`just regen-grammars` (§14.4) after any grammar edit; if shared tokens
diverge, parser.c diffs show it. Phase 2 dogfood (§14.9) adds a sprf
rule to grep for missing spreads.

### 14.4 Build pipeline — manual regen, minimal cargo hooks

Regeneration is author-driven via `justfile` recipes. Cargo compiles
what's already committed.

**`justfile`** at repo root (new, ~20 LoC):

```make
# regenerate a single op's parser.c from its grammar.js
regen-op OP:
    cd v3/crates/pipeline/src/ops/{{OP}} && tree-sitter generate

# regenerate all pattern op parsers
regen-grammars:
    for op in v3/crates/pipeline/src/ops/*/grammar.js; do \
      dir=$(dirname $op); \
      echo "regenerating $dir"; \
      (cd $dir && tree-sitter generate); \
    done

# regenerate host grammar (existing convention)
regen-host:
    cd v3/crates/tree-sitter-sprefa && tree-sitter generate
```

**`pipeline/build.rs`** (~40 LoC): one job only — compile each op's
committed `parser.c` + optional `scanner.c` via `cc::Build`. It does
not invoke `tree-sitter`. If `parser.c` is missing the build fails
with a pointer to `just regen-op <name>`.

**`op_languages.rs`**: hand-written or emitted by `build.rs` as a
trivial `match` over op names → `extern "C"` language fns. Either
works; hand-written is fine while the op count is small.

**`tree-sitter-sprefa/queries/injections.scm`**: hand-edited. Shape:

```scheme
((op_invocation
  name: (identifier) @_n
  paren: (paren_slot) @injection.content)
 (#match? @_n "^(glob|re|json|ast|sh)$")
 (#set! injection.language "sprefa_\\1"))
```

Adding a new pattern op = append to the alternation, commit.

### 14.5 Rust trait surface

Two slots added to `Op` (both default to "not a pattern op"), plus a
companion `PatternOp` sub-trait for pattern-specific hooks.

(Pass A of spike sprefa-x5b landed 2026-04-22: `bound_captures`,
`try_raw_regex`, `materialize_with` now live on `Op` as optional
capability methods with defaults; see §14.5m.2. `PatternValue` deleted;
`GlobPattern` / `RegexPattern` / `JsonPattern` / `AstPattern` deleted;
state folded into `GlobOp` / `ReOp`. The trait snippet below reflects
the landed shape.)

```rust
pub trait Op: Send + Sync + std::fmt::Debug + 'static {
    fn name(&self) -> &'static str;
    fn pipe<'a>(&'a self, ctx: &'a RtCtx, c: Cursor) -> BoxFuture<'a, Vec<Cursor>>;

    /// Per-slot expectation. Lowerer consults this positionally to
    /// validate args and reject obvious misuses (atom into an op slot,
    /// etc.). Default empty — ops with no args or opaque bodies (`str`,
    /// `void`) skip. One entry per slot in lex order. Variadic-tail is
    /// a follow-up.
    fn arg_spec(&self) -> &[ArgSpec] { &[] }

    /// Sub-grammar for this op's paren-slot body. None = non-pattern op.
    fn language(&self) -> Option<tree_sitter::Language> { None }

    /// Highlight queries for the sub-grammar.
    fn highlights(&self) -> Option<&'static str> { None }

    // --- capability surface (Pass A of §14.5m) -----------------------
    /// `$NAME` holes this op declares as writable/readable captures.
    fn bound_captures(&self) -> &[Arc<str>] { &[] }
    /// Bulk escape hatch. `Some(&regex)` iff this op's work is a regex
    /// over raw bytes with no structural post-filter.
    fn try_raw_regex(&self) -> Option<&Regex> { None }
    /// Partial eager collapse: rebuild the matcher with upstream-bound
    /// terms substituted as escaped literals.
    fn materialize_with(&self, _: &HashMap<Arc<str>, Vec<u8>>) -> Option<Regex> { None }
}

pub trait PatternOp: Op {
    /// Capture names declared in the parsed sub-tree. Walks the tree;
    /// callable before the op is compiled (resolver/LSP use this).
    fn binds_captures(&self, tree: &Tree, bytes: &[u8]) -> Vec<Arc<str>>;

    /// Hover body for a pattern-local node kind (e.g. re's char_class).
    /// `term_ref` hover is framework-owned; this is for match-kind nodes.
    fn hover_match(&self, node: Node, cursors: &[Cursor]) -> Option<String> { None }

    /// Per-cursor work at pipe-step position (macro-generated
    /// `Op::pipe` delegates here for macro-wired pattern ops).
    /// Default: passthrough. Pattern ops override with their own
    /// identity-apply. Glob/Re no longer use the macro; they impl
    /// `Op::pipe` directly from their fused state.
    fn pattern_pipe<'a>(&'a self, ctx: &'a RtCtx, c: Cursor)
        -> BoxFuture<'a, Vec<Cursor>> { /* default passthrough */ }
}

// Construction lives off-trait because it returns Self (Sized):
impl GlobOp {
    pub fn compile_from_tree(tree: &Tree, bytes: &[u8])
        -> Result<Self, Vec<PatternDiagnostic>> { ... }
}
impl ReOp {
    pub fn compile_from_tree(tree: &Tree, bytes: &[u8])
        -> Result<Self, Vec<PatternDiagnostic>> { ... }
}
```

### 14.5a Pattern value shape (locked 2026-04-21, collapsed 2026-04-22 §14.5m)

**Historical form (locked 2026-04-21, superseded 2026-04-22):** four
concrete `*Pattern` structs behind a `PatternValue` object-safe trait,
with `Pattern = Arc<dyn PatternValue>` as a `Value` variant. Three
accessors — `apply`, `as_raw_regex`, `materialize_for` — drove consumers.

**Landed form (Pass A of §14.5m, 2026-04-22):** the wrapper structs and
the trait are gone. Their state fuses directly into `GlobOp` / `ReOp`:

```rust
pub struct GlobOp {
    template:       Vec<Seg>,
    regex:          regex::bytes::Regex,  // cached all-unbound form
    bound_captures: Vec<Arc<str>>,
    anchors:        (Arc<str>, Arc<str>),
}
pub struct ReOp { /* same four fields */ }

pub enum Seg {
    Fragment(Arc<str>),                            // pre-escaped regex bytes
    Term { name: Arc<str>, unbound_re: Arc<str> }, // $NAME hole, read-or-write at apply time
}
```

`Json` / `Ast` placeholder structs deleted outright; those ops haven't
landed yet and will carry their own fused state when they do.

**Projection model stays the same.** Every pattern op is
`(projection, regex, rehydrate)`. Identity-projection ops (Regex, Glob)
answer `try_raw_regex() -> Some(&Regex)` and unlock the bulk fast path.
Non-identity ops answer `None` and force consumers down the per-cursor
`Op::pipe` path. One code path across variants.

**Accessors moved to the `Op` capability surface (§14.5m.2):**

```rust
trait Op {
    /// Bulk escape hatch. Some(&regex) iff projection is identity and
    /// no structural post-filter. fs/rev/repo/ripgrep wrappers hand
    /// the regex straight to the bulk backend.
    fn try_raw_regex(&self) -> Option<&Regex> { None }

    /// Rebuild regex with upstream-bound terms substituted as escaped
    /// literals. Bulk consumers call this when any term is
    /// upstream-bound. `None` = op does not carry a rebuildable template.
    fn materialize_with(&self, bindings: &HashMap<Arc<str>, Vec<u8>>)
        -> Option<Regex> { None }

    /// Uniform per-cursor interface. Dispatches write vs read per hole
    /// at apply time via the shared `apply_identity_pattern` helper.
    fn pipe<'a>(&'a self, ctx: &'a RtCtx, c: Cursor)
        -> BoxFuture<'a, Vec<Cursor>>;
}
```

Shared `apply_identity_pattern(regex, bound_captures, template, anchors, &c)`
helper lives in `pipeline::value` and is reused by both Glob and Re
`Op::pipe` impls.

### 14.5b Term-binding dispatch (§18 arg-mode for pattern bodies)

`$NAME` inside a pattern body is dual. At `apply` time each hole is
classified by looking it up in `cursor.captures`:

| upstream state of `$NAME` | mode   | effect |
|---------------------------|--------|--------|
| absent from `c.captures`  | write  | hole becomes `(?P<NAME>unbound_re)`; match span written as a new Capture on the output cursor |
| present in `c.captures`   | read   | hole substituted with `regex::escape(bound_bytes)`; no new Capture written (upstream owns it) |

Fast path: every hole unbound → cached `regex` reused. Slow path: at
least one hole bound → `materialize_for` rebuilds a fresh regex for
this `apply`. Future amortization: LRU cache keyed by bindings set.

Unbound-hole substitutions are op-specific constants:

| op    | unbound_re    | rationale                             |
|-------|---------------|---------------------------------------|
| glob  | `[^/]+`       | never cross `/` unless `**` says so   |
| re    | `.*?`         | non-greedy so chained holes don't eat |
| json  | (op-specific) | leaf-scalar default                   |
| ast   | n/a           | structural post-filter, not regex     |

### 14.5c fs reads repo/rev/fs as Cursor fields, not captures

**Status (2026-04-21).** Landed in `pipeline` crate:

- `Cursor { repo: Arc<str>, rev: Arc<str>, fs: Option<Arc<Path>>, … }`
  (`_0_cursor.rs`). `narrow()` carries by clone; `rebase()` preserves
  repo/rev/fs while resetting slots + content.
- `Capture { name, byte_range, kind }` where
  `CaptureKind::{SpanBacked, Synthesized { value: Arc<str> }}`. Bind-
  mode ops (repo / rev / fs) emit Synthesized captures; pattern ops
  emit SpanBacked. `Capture::bytes(cursor.content)` resolves either
  source uniformly; `Pattern::apply`'s binding collector uses it.
- `ops/repo/mod.rs`, `ops/rev/mod.rs`, `ops/fs/mod.rs`: filter-or-bind
  classified from the raw paren source via a local `parse_capture`
  helper plus `glob::compile_str`. `rev` additionally rejects
  `*`/`**`/`**/*`/`*/*` (unbounded-wildcard) and bare `$V`
  (unbounded-capture).
- `fs` pulls its file listing through `RtCtx.put(FsListFilesEffect
  { repo, rev })` (`pipeline::effects`). The effect is a
  `PureEffect` in the `"fs"` domain — listings are cached per
  `(repo, rev)` so repeat `fs(...)` call-sites across pipes, rules,
  and fork arms share one walk. Callers register an
  `FsListFilesBatcher` on the `RtCtxBuilder` at ctx construction;
  `FsOp` itself is stateless beyond its mode and carries no
  `FileSource` handle.

Original-spec cursor sketch follows:

Parroting v2 (`v2/src/_0_types.rs:267`, `v2/src/ops/_1_repo.rs`).
Cursor carries repo/rev/fs as first-class fields (Phase-1.5 landing):

```rust
pub struct Cursor {
    pub content:    Arc<[u8]>,
    pub byte_range: Range<usize>,
    pub repo:       Arc<str>,           // added for v3
    pub rev:        Arc<str>,           // added for v3
    pub fs:         Option<Arc<Path>>,  // added for v3
    pub captures:   Vec<Capture>,
    pub path:       SprfPath,
    pub last_bound: Option<Arc<str>>,
    pub slots:      HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}
```

**fs contract.** `fs(pattern)` takes exactly one slot: a pattern. It
reads `cursor.repo`, `cursor.rev`, `cursor.fs` from the incoming
cursor to know what to walk. No kwargs, no brackets, no `=` sugar.
Per-cursor file stream flows through the pattern; matches fan out as
child cursors.

**repo/rev/fs as ops.** Dedicated filter-or-bind pattern ops mirror
v2. Paren body is a single `Value`; the outer op classifies by Value
kind (locked 2026-04-22):

```
repo(glob(myorg/*))  # filter: cursor.repo must match glob
repo(:home)          # filter: equals-atom shortcut
repo(str(some-org))  # filter: equality on non-ident bytes
repo($R)             # bind:   write cursor.repo into capture $R

rev(:main)           # filter: cursor.rev equals
rev(glob(v1.*))      # filter: glob
rev(str(next/10.x))  # filter: non-ident literal
rev($V)              # ERROR: unbounded-capture; see rationale below
rev(re($V))          # legal opt-in: every rev matches, bind into V (expensive)

fs(glob(**/*.rs))    # filter/enumerate
fs(:CHANGELOG)       # equality on filename
fs($P)               # bind: read cursor.fs into capture $P (rare)
```

Consumer dispatch pattern: each factory takes `Vec<Value>` and matches
on `values[0]`:

- `Value::Term(name)`: bind mode. `rev` rejects (unbounded). `repo`
  and `fs` accept, emit Synthesized capture.
- `Value::Atom(s)` or `Value::Str(s)`: filter mode with byte equality
  (compiled to an anchored glob internally).
- `Value::Op(op) if op.try_raw_regex().is_some()`: filter mode driven
  by the op's raw regex. Covers both `glob(...)` and `re(...)` without
  the consumer caring which.
- Other `Value::Op(op)`: rejected — the consumer needs a regex surface
  and the op doesn't expose one (e.g. a json/ast op in an fs slot).

No local `parse_capture` / `classify_token` helper in the ops. The
lowerer resolves the Value shape before the factory sees it.

**Projection operators** `&.repo` / `&.rev` / `&.fs` (grammar already
accepts these; §8.5) turn a Cursor field into a cursor whose `content`
is that field's value.

Landing order: Cursor fields + preserve across rebase/narrow first;
RepoOp / RevOp / FsOp next. **All four landed 2026-04-21.**

Pattern-value refactor (sprefa-4m7.14, 2026-04-22): factories migrated
from raw-string paren bodies to structured `Value` inputs. The
`parse_capture` helper and `glob::compile_str` inline calls are
retired from rev/repo/fs; lowering happens at parse time via the
nested-op-in-slot grammar path.

### 14.5c.1 `rev($V)` rejection rationale

A bare term in a filter position has nothing to match against and
would bind every rev unconditionally — each rev potentially
materializes its own worktree on first query. Rejected at factory
time with diagnostic `rev/unbounded-capture`. Authors who genuinely
want per-rev fan-out write `rev(re($V))` (or `rev(glob(*))` if the
self-harm is preferred shape), which makes the "all revs" intent
explicit.

This sits in the narrow class of ops where bind-only-without-body is
unsound. `repo($R)` and `fs($P)` stay legal because repo is bounded
by the config's repo set and fs's enumerator body (the pattern) is
the discipline; rev lacks an enumerator here.

### 14.5d Server crate landing

`v3/crates/server` hosts the stdio LSP for smoke-testing `.sprf` sources
in VS Code. Shape:

- `DocSession` owns one source buffer plus the latest `ParsedSource` +
  `Vec<ParseError>`. `on_source_change` reparses via
  `host_parse_with_injections`, feeding `pipeline::op_languages::language_of`
  as the injection resolver.
- `Backend` is a `tower_lsp::LanguageServer`; per-URI `DocSession` map.
  did_open / did_change / did_save → publish parse diagnostics as LSP
  `Diagnostic` ranges.
- Hover and completion return `None` until the DocSession grows past
  the parse layer (§14.6).
- Transport: stdio only. HTTP / WebSocket (v2 `_5_transport_http.rs`)
  deferred.
- Binary: `sprefa-lsp` under `server/src/bin/`.

### 14.5e `rule(name)` — linear + brace forms (LANDED)

`rule` is recognized at pipe-head position by the CLI driver
(`server/src/bin/sprefa-run.rs`). Two shapes, both host-grammar native:

- Linear: `rule(name) > op > op ...` — the rest of the pipe runs
  under the given name; header printed as `rule <name> — N rows`.
- Brace:  `rule(name) { pipe; pipe; }` — the brace body is re-parsed
  as a `.sprf` sub-program via `host_parse_with_injections`, each
  sub-pipe runs independently, header printed as
  `rule <name> pipe <j> — N rows`.

The brace body reparse uses the outer language resolver, so sub-pipes
get the same pattern-op grammar injections as top-level pipes. Rule
itself is not a registered pipeline `Op`; it is a control-flow sugar
handled by the driver above the op layer. A future registry pass
generalizes this dispatch but the user surface is locked.

Smoke: `tests/smoke/_2_rule.sh` + `fixtures/rule_smoke.sprf`.

### 14.5f Glob `**` semantics (ripgrep-style, LANDED)

`GlobOp::compile` walks paired tokens before single-node lowering:

- `/**/` → `(?:/[^/]+)*/` — zero or more interior segments.
- `**/` → `(?:[^/]+/)*` — zero or more leading segments.
- `/**` → `(?:/[^/]+)*` — zero or more trailing segments.
- bare `**` → `.*` (unchanged; fallback for non-path globs).

This fixes the former segment-bracketed behavior where `src/**/*.rs`
missed `src/foo.rs`. Covered by tests in `ops/glob/mod.rs`:
`double_star_slash_matches_zero_or_more_segments`,
`leading_double_star_slash_matches_zero_prefix`,
`trailing_slash_double_star_matches_zero_suffix`.

### 14.5g `.sprefa.toml` workspace config (LANDED)

`server/src/config.rs` loads a TOML workspace file into
`Config { seeds: Vec<Seed> }`. Each seed names one `(slug, root, rev)`
triple. The CLI driver builds one `RtCtx` per seed, registers an
`FsListFilesBatcher` pointed at that seed's `DiskFileSource`, and
iterates every pipe in the `.sprf` source under that seed.

Format:

```toml
[[seed]]
slug = "sprefa"
root = "."        # relative roots resolved against the .toml's parent
rev  = "HEAD"     # default when omitted

[[seed]]
slug = "other"
root = "../other"
rev  = "main"
```

Resolution precedence (sprefa-run):

1. `--config <path>` explicit.
2. CLI single-seed: `--root <dir>` + `--rev <rev>`; slug = basename(root).

Ancestor walk and `.sprefa.toml`-in-CWD auto-pickup are follow-ups.
Output headers prefix each line with `[slug] ` when more than one seed
is in play; single-seed runs keep the legacy `pipe I — N rows` shape
so the existing smoke scripts stay intact.

### 14.5h `FsListFilesEffect` batcher (LANDED)

`pipeline/src/effects.rs` defines:

- `FsListFilesEffect { repo: Arc<str>, rev: Arc<str> }` —
  `PureEffect` in the `"fs"` domain; response
  `Vec<Arc<Path>>`; cache key `(repo, rev)`.
- `FsListFilesBatcher` — wraps `Arc<dyn FileSource>`; implements
  `Batcher<FsListFilesEffect>`.

`FsOp` holds only its `FsMode` (filter glob or bind name + `**`-glob).
At `pipe()` time it calls `ctx.put(FsListFilesEffect { repo, rev })`
and filters the returned paths through the compiled glob. Multiple
`fs(...)` call-sites in the same pipe, rule body, or seed share one
fs walk courtesy of the effect's `CacheLayer`. Test coverage lives in
`effects::tests` plus the pre-existing `fs::tests` rewired to build
an `RtCtx` with the batcher registered.

### 14.5i `comment(open_re [, close_re])` — v2 `marker` port (LANDED)

`ops/comment.rs` narrows `cursor.byte_range` to comment-bounded
regions. Two shapes from one body:

- Single arg `comment("SECTION:")` — sequential. Every comment line
  whose text matches `open_re` opens a region; the next match (or
  EOF) closes it.
- Two args `comment("BEGIN:", "END:")` — paired. Matches nest LIFO;
  an unpaired open collapses to the next comment line (or EOF). Close
  matches with no open on the stack are ignored.

A named group in the open regex binds the matched label as a
SpanBacked capture on the output cursor. Fallback: when no named
group exists, the op synthesizes the label from the trimmed post-match
tail.

Comment detection is line-prefix only: `//`, `#`, `--`, `/*`, `*`,
`<!--`. Per-language tree-sitter detection is parked; the line-prefix
set covers the common surfaces without pulling a language parser into
`pipeline`.

The paren body is classified at construction time via a top-level
comma split that respects `[...]` classes and backslash escapes. No
tree-sitter injection grammar yet; a `comment` sub-grammar lands when
the op grows beyond regex-pair semantics.

Diagnostic codes: `comment/missing-arg`, `comment/too-many-args`,
`comment/open-syntax`, `comment/close-syntax`.

### 14.5j Operator registry (LANDED)

`pipeline/src/registry.rs` maps op name → factory closure:

```rust
pub type OpFactory =
    dyn Fn(&str, &str) -> Result<Box<dyn Op>, Vec<PatternDiagnostic>>
    + Send + Sync;
```

The factory takes `(op_name, paren_body)` as raw strings so `pipeline`
stays free of any Cargo dep on `sprefa_parse` (§1.6 invariant).
Callers holding an `OpInvocation` extract its paren body and dispatch
through `Registry::build(name, body)`.

`Registry::with_stdlib()` pre-registers every built-in: `repo`, `rev`,
`fs`, `void`, `str`, `comment`, `print`. `CaptureWriteOp` is built out
of band via `Registry::capture_write(target)` because the grammar
lowers `> $NAME` to a dedicated `PipeStepKind::CaptureWrite` node, not
an op invocation.

`Registry::doc(name)` returns the op's markdown hover text. Every
registered name has a doc; a unit test enforces that invariant.

`sprefa-run` and the LSP `Backend` share one registry; construction is
one-time at driver startup.

### 14.5k `PrintEffect` + `print([prefix])` (LANDED)

First write-side (non-pure) effect in the v3 pipeline. Registered via
`RtCtxBuilder::register` (not `register_pure`) — print is not
cacheable.

- `PrintEffect { line: Arc<str> }` — `EffectKind` with
  `payload_bytes = Some(line.len())` for telemetry.
- `PrintSink { Stdout | Buffer(Arc<Mutex<Vec<String>>>) }` — sink
  selected at batcher construction. `Buffer` gives tests captured
  output without redirecting stdout.
- `PrintBatcher::new(sink)`, `PrintBatcher::stdout()`,
  `PrintBatcher::buffer()` — three constructors for the three usage
  shapes.
- `PrintOp { prefix: Option<Arc<str>> }` at `pipe()` time puts one
  `PrintEffect { line }` per input cursor. Optional prefix prepends
  `prefix: ` so several `print` sites stay distinguishable. The cursor
  flows through unchanged.

Diagnostic code: `print/too-many-args`.

### 14.5l LSP hover dispatcher (LANDED)

`DocSession::hover_at(offset)` walks the host CST to the innermost
descendant at `offset`, climbs to the enclosing `op_invocation`, reads
its `name` field, and returns `Registry::doc(name)`. The LSP
`Backend::hover` converts the LSP `Position` to a byte offset via
`position_to_offset` and wraps the markdown body in a
`HoverContents::Markup` response.

Hover inside an injected pattern body (per §14.6) is the next step.
The registry and `PatternOp::hover_match` surface are in place; the
dispatcher today terminates at the outer op name.

---

### 14.5m x5b — IO unification + lowerer trait fu (spike locked 2026-04-22; Pass A + Pass B both landed 2026-04-22)

Thesis: one IO contract end-to-end, `cursor[] -> cursor[]`. Every
composable thing impls `Op`. Pattern, Pipeline, Term fold into a single
`Value::Op(Arc<dyn Op>)` variant; scalars stay as cheap leaves. Args are
tuples. Pipes are tuples. Patterns are tuples of sub-grammar-emitted
Ops. Same substance at every tier.

#### 14.5m.1 Value collapse (target)

```rust
// post-x5b
enum Value {
    Atom(Arc<str>),
    Str(Arc<str>),
    Int(i64),
    Float(f64),
    Op(Arc<dyn Op>),   // absorbs Pattern + Pipeline + Term from §4.3
}
```

`$NAME` is sugar for term-op; `${body}` is term-op with a sprf-body
slot. The grammar-level fused token stays for ergonomics; the lowerer
desugars to `Value::Op(Arc<term_op>)`.

Value::Term may survive as sugar-distinct through migration for
resolver ergonomics; re-evaluate after step 5 of §14.5m.6.

#### 14.5m.2 Op capability surface

`Op` stays object-safe. Optional capability methods with defaults carry
the bulk/lazy/eager collapse hooks that `PatternValue` used to own:

```rust
pub trait Op: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn pipe<'a>(&'a self, ctx: &'a RtCtx, c: Cursor) -> BoxFuture<'a, Vec<Cursor>>;
    fn arg_spec(&self) -> &[ArgSpec] { &[] }
    fn language(&self) -> Option<tree_sitter::Language> { None }
    fn highlights(&self) -> Option<&'static str> { None }

    // capability surface (absorbed from PatternValue):
    fn bound_captures(&self) -> &[Arc<str>] { &[] }
    fn try_raw_regex(&self) -> Option<&Regex> { None }
    fn materialize_with(&self, _: &HashMap<Arc<str>, Vec<u8>>) -> Option<Regex> { None }
}
```

Consumers (`fs`, `rev`, `repo`, `comment`) stop pattern-matching on
`Value` variants and stop downcasting to `GlobPattern` / `RegexPattern`.
They call `op.try_raw_regex()` for the bulk path and fall through to
`op.pipe(ctx, c)` for the generic path. Concrete matcher structs
(`GlobPattern`, `RegexPattern`) fold into the compiled state of their
ops (`GlobOp`, `ReOp`).

`PatternOp` extension trait stays — it owns the compile-time
sub-grammar surface (`compile`, `language`, `highlights`, `hover_match`)
that non-pattern ops don't need. Runtime dispatch surface is Op only.

#### 14.5m.3 Scan-pointer as op category (not a framework primitive)

§12 already says scan-pointers are ops. x5b names the three shapes
explicitly, distinguished by what happens when an arg is observed
unbound at drive time:

| shape | source when unbound | example |
|---|---|---|
| produce-or-filter | op has a source to draw from | `fact(:key, $V)`, `fs($P)`, `rev($R)`, `repo($R)` |
| filter-only | no source; asserts on existing cursor state | `is_rev($R)`, `is_repo($R)`, `is_fs($F)` |
| pattern-with-embedded-term | pattern compiles its hole into write-mode named group, or read-mode literal substitution | `re($NAME = ...)`, `glob(.../$F)` |

Per-op arg-mode dispatch: each op inspects incoming `cursor.captures`
for names declared in `bound_captures()`. Per-slot state (absent /
present-empty / present-full) selects the branch. Pipe order is goal
order. No search, no re-ordering, no backtrack. Shallow prolog,
forward-only.

Two equivalent compositions express the same semantics:

```sprf
json({ ${KEY > is_rev()}: _ })   # sub-pipe drives is_rev on cursor w/ KEY slot
json({ ${is_rev($KEY)}: _ })     # is_rev takes $KEY arg, inspects slot
```

Both end with `KEY` bound iff `is_rev` approves. Difference is
composition shape (pipe vs arg); semantics identical.

#### 14.5m.4 Term-as-cursor-of-1-hashmap

A term IS a cursor with one capture slot entry. Bound = slot has
bytes. Unbound = slot present but empty. Three-state (unbound / bound
/ free) collapses to two. "Free" doesn't exist at runtime — it's just
slot-absent, which is a resolver error at lower time (§19).

Scan-pointer side-effect = op writing to `cursor.captures` from its
pipe body. Framework never auto-writes. Every capture mutation is an
op executing its own semantics. `bound_captures()` is the
declared-intent contract that resolver and LSP read; the op itself is
the authority on how bindings flow.

#### 14.5m.5 Lowerer trait fu — default behavior, per-op escape hatch

Object-safe `Op` can't hold constructors (`Self: Sized`). Split into
three traits:

```rust
// Object-safe; dyn-dispatched. Above, §14.5m.2.
pub trait Op { ... }

// Companion; compile-time. Most ops live here.
pub trait OpCtor: Op + Sized {
    fn from_values(values: Vec<Value>) -> Result<Self, Vec<PatternDiagnostic>>;
    fn compile_from_tree(_: &Tree, _: &[u8])
        -> Result<Self, Vec<PatternDiagnostic>> { Err(unsupported()) }
    fn language_static() -> Option<tree_sitter::Language> { None }
}

// Registry-facing factory; one impl per op.
pub trait OpLowering: Send + Sync + 'static {
    fn lower(
        &self,
        node: Node<'_>,
        src: &[u8],
        registry: &Registry,
        diags: &mut Vec<PatternDiagnostic>,
    ) -> Option<Box<dyn Op>>;
}

pub struct DefaultLowering<O: OpCtor>(PhantomData<O>);

impl<O: OpCtor + 'static> OpLowering for DefaultLowering<O> {
    fn lower(&self, node, src, registry, diags) -> Option<Box<dyn Op>> {
        // 1. If O::language_static() is Some AND node has an injected
        //    tree (sub-grammar body): parse + O::compile_from_tree.
        // 2. Else: walk paren children via registry.lower_paren_slot
        //    into Vec<Value>; call O::from_values.
        // 3. Validate arg count/variants against O::arg_spec() before
        //    the from_values call; push diagnostics for mismatches.
    }
}
```

Registration:

```rust
registry.register::<FsOp>("fs");               // default lowering (90% of ops)
registry.register::<ReOp>("re");               // default lowering (has language_static)
registry.register::<GlobOp>("glob");
registry.register_custom("fact", FactLowering);  // custom: per-slot arg-mode dispatch
```

Ops that need raw CST inspection, partial-construction on error, or
per-slot arg-mode dispatch that can't be expressed via `arg_spec`
register a custom `OpLowering` impl. `fact` is the first expected
consumer (produce-or-filter shape with both arg modes coming through
the same syntactic slot).

#### 14.5m.6 Migration steps

**Pass A — landed 2026-04-22 (Value collapse + consumer migration):**

1. [x] `_1_op.rs` — `bound_captures` / `try_raw_regex` /
   `materialize_with` added with defaults. `Op: Debug` bound added.
2. [x] `value.rs` — `Value::Pattern` + `Value::Pipeline` collapsed
   into `Value::Op(Arc<dyn Op>)`. `ArgSpec::Pattern` + `::Pipeline`
   collapsed into `ArgSpec::Op`. `Pattern` type alias removed outright
   (no migration window needed — consumers flipped in the same pass).
7. [x] Deleted: `PatternValue` trait, `GlobPattern`, `RegexPattern`,
   `JsonPattern`, `AstPattern`, `AstStructuralConfirm`. Json/Ast
   placeholders removed since those ops haven't landed.
8. [x] `GlobOp` / `ReOp` fused with their former `*Pattern`
   structs: hold `template` / `regex` / `bound_captures` / `anchors`
   directly. Static inherent `compile_from_tree(&Tree, &[u8]) -> Self`.
   `Op::pipe` runs `apply_identity_pattern` directly. `Op::try_raw_regex`
   / `materialize_with` / `bound_captures` override the capability
   defaults from their own state. Dropped `#[sprf_pattern_op]` macro
   for these two — hand-rolled `impl Op` is cleaner than extending the
   macro to delegate capability methods.
9. [x] `PatternOp` trait slimmed: kept `binds_captures` (tree-walk
   probe, used by resolver/LSP) and `hover_match`. Dropped `compile`,
   `compiled_pattern`, `pattern_pipe` default. Construction now lives
   on the op as an inherent `compile_from_tree` (requires `Self: Sized`,
   so outside the dyn-safe trait).
10. [x] Consumers (`fs` / `rev` / `repo` / `comment`): downcast-to-
    concrete replaced with `Value::Op(op) if op.try_raw_regex().is_some()`
    + `op.try_raw_regex().unwrap().clone()` where a regex is needed.
    RevOp / RepoOp / FsOp store `Arc<dyn Op>` in their Filter variants.
11. [x] `registry.rs` — `compile_pattern` returns `Arc<dyn Op>`.
    Nested `op_invocation` lowering emits `Value::Op(op)` directly.
    Two-door registry shape preserved (`OpFactory` + `PatternOpFactory`).
12. [x] 170/170 workspace tests green.

**Pass B — landed 2026-04-22 (trait-fu registry ergonomics):**

1. [x] `op_ctor.rs` NEW — three sibling traits: `OpCtor` (value-arg
   construction), `PatternCtor` (compile-from-injected-tree), `OpLowering`
   (raw-CST escape hatch). Each carries `const NAME` / `const DOC`.
   `PatternCtor` also carries `language()` / `highlights()` so the
   sub-grammar handle moves out of the by-name `op_languages` lookup.
2. [x] `registry.rs` — `Builder::register::<O>()` /
   `register_pattern::<O>()` / `register_custom::<L>()` replace the
   per-op closure hand-rolling. Languages + highlights now stored in
   the registry (`Registry::pattern_language(name)` /
   `pattern_highlights(name)`); the by-name `crate::op_languages::
   language_of` lookup in `lower_nested_op_invocation` becomes a
   typed registry method. The two factory tables remain underneath the
   trait facade — the trait surface is the seam, the storage shape is
   unchanged.
3. [x] Per-op `impl OpCtor for <Op>`: every stdlib op now declares
   `NAME` + `DOC` on its own type. `from_values` either hoisted into
   the trait or delegated to the inherent (kept inherent for the
   five ops with non-trivial bodies — repo/rev/fs/comment/print —
   so the inherent ascii-table-of-arms remains the source of truth).
4. [x] `with_stdlib()` is now ten one-liners
   (`register::<RepoOp>()` ... `register_pattern::<ReOp>()`).
   `register_custom::<L>()` exists with no consumer; lands when the
   first `fact`-style op arrives.

Pass B is organizational — unlocks nothing at runtime. The motivation
for landing it now (rather than waiting for `fact`) was twofold:
ergonomic registration for the next 4-5 ops about to land (json/ast/
sh/render), and the `register_custom` door pre-built so `fact` slots in
without re-shaping the registry.

LoC: registry.rs net −22 (factory closures + `*_DOC` consts removed,
trait-driven Builder methods + language storage added). Per-op DOC
strings now live next to each op (~80 lines moved, not added).

#### 14.5m.7 Open tensions (deferred)

- `Value::Term` survival: kept as sugar-distinct variant through
  Pass A. Resolver ergonomics seem to want the syntactic distinction;
  re-evaluate if Pass B or a later slice surfaces a reason to fold
  into `Value::Op(term_op)`.
- `ArgSpec` post-collapse is now `Atom | Str | Int | Float | Term |
  Op | Any`. Per-slot "must-be-bulk-collapsible" refinement (e.g.
  `ArgSpec::Op { bulk: required }`) left as op-side diagnostic for now
  — current consumers check `op.try_raw_regex().is_some()` inline.
- Scalar literals as trivial Ops: no. Leaves stay leaves; lift on
  demand at the outer op's seam.

---

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

`str` does NOT use this macro. It is a plain `Op` impl; `language()`
and `highlights()` return `None`; its `pipe()` reads `paren_slot`
bytes into `Arc<str>` and stores them in a cursor slot (no compile,
no sub-tree, no captures).

### 14.5a Parsed source — host tree plus injected trees

`sprefa_parse::host_parse` returns a `ParsedSource` that bundles the
host tree with one injected tree per pattern-op call-site. Parsing is
a sum:

```
parse_host(bytes)                       : bytes → (HostTree, Errors)
parse_L_i(bytes[slot_i.byte_range])     : bytes → (InjectedTree_i, Errors)
                                          for each pattern-op invocation i
                                          with language L_i = op(i).language()

ParsedSource(bytes) = parse_host(bytes)  +  { parse_L_i(slot_i) : i }
```

Same big-O as a single host parse: each byte is parsed at most twice
(host sees `paren_slot` as opaque; injected grammar consumes slot
bytes only, via tree-sitter `set_included_ranges`).

```rust
pub struct ParsedSource {
    pub host:     Arc<Tree>,           // full host parse
    pub pipes:    Vec<Pipe>,           // lowered pipeline surface
    pub injected: Vec<InjectedTree>,   // one per pattern-op call-site
}

pub struct InjectedTree {
    pub host_node:     Arc<ParseSite>, // pointer back to the paren_slot
    pub language_name: Arc<str>,       // "sprefa_glob", "sprefa_re", ...
    pub tree:          Arc<Tree>,      // parsed body
}
```

Why it exists:

- **Capture extraction at lower time**: `PatternOp::binds_captures`
  walks `term_ref` nodes in the injected tree (§14.5). No string-scan
  phase over the host source.
- **LSP hover by node kind**: the dispatcher (§14.6) reaches the
  injected tree by host_node byte-range lookup, then dispatches on
  `node.kind()`.
- **Parse-phase pattern diagnostics**: ERROR / MISSING nodes inside
  the injected tree feed the same `collect_errors` pass as host errors
  (§14.8).

Dependency direction: `sprefa_parse` takes a `&dyn Fn(&str) ->
Option<Language>` closure for the op→language lookup; it does not
depend on `pipeline`. The pipeline crate supplies the closure at
construction time, keyed off `op_languages` (§14.4).

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
| `str` | N/A — body is unparsed bytes; `$NAME` inside `str(...)` is literal |

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
| `pattern/<op>-syntax` | parse | tree-sitter ERROR inside op's injected tree |
| `pattern/<op>-missing` | parse | tree-sitter MISSING inside op's injected tree |

`str` has no parse-phase pattern diagnostics; its body is unparsed.

### 14.9 Dogfood path

Phase 1 — manual YOLO (near-term landing):
- `justfile` with `regen-op`, `regen-grammars`, `regen-host` recipes
- `pipeline/build.rs` does `cc::Build` only (no tree-sitter invocation)
- `tree-sitter-sprefa/shared/tokens.js` (author-maintained, no drift-grep)
- `tree-sitter-sprefa/queries/injections.scm` hand-edited
- `ops/str/` (const, plain `Op`), `ops/glob/`, `ops/re/` with committed
  `parser.c` next to each `grammar.js`
- `#[sprf_pattern_op]` proc-macro in `sprefa_macros`
- injected-tree support in `sprefa_parse::host_parse`

Phase 2 — dogfood (later):
- a sprf rule walks `pipeline/src/ops/*/`, finds `#[sprf_pattern_op]` sites
- drift-checks shared tokens, regenerates `parser.c`, regenerates
  `injections.scm`
- LSP code-action "register new pattern op" drops a skeleton folder +
  reruns `just regen-op` via `MutationEffect` (§32)
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
rule(classes) > ast[rust] { class ${NAME} }              # zero params; runs on subscribe
rule(used_by, $CLASS) > ast[rust] { new ${CLASS}() }     # one param; lazy until call

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

Example: rule `r` whose body calls `fact($PARAM, :kind)` — fact requires
arg 0 bound; therefore `r`'s `$PARAM` is derived as `BoundOnly` at
call site.

### 18.2 Rule mode: explicit annotation

Reserved for §15 term-annotations lane. Not locked.

### 18.3 Pattern ops as arg-mode consumers

Pattern ops (§14) declare their paren body as a single positional arg
with `body = Body::Paren { injected: true }`. Bracket-tags still route
through the normal `ArgSpec` list (e.g. `ast[rust](...)` has one
bracket-tag arg `:lang` plus one paren-body arg). `$NAME` references
inside a pattern body are **not** arg-mode inputs — they are
capture-writes owned by the op (§14.7). Resolver reads them via
`PatternOp::binds_captures(injected_tree)` at lower time (§19 phase
lower) and treats them identically to captures produced by a
downstream `> $NAME`.

`str` has `body = Body::Paren { injected: false }`; its paren body is
raw bytes, no injected tree, no captures.

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
| relations | `relations` | fact-ops, link-ops, scan-pointer ops | cursor-pass-through side effect |
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
| `$$repo($R)` / `$$rev($T)` scan sigil class | fact-op family `is_repo($R)` / `is_rev($T)` (§12) |
| `$$` Ans-slot sigil | scan-pointer op reads `cursor.last_bound` directly (§12) |
| `$$sigil` migration diag | dead; v1→v3 source must be rewritten |
| `&&.rules` registry sigil | op-mediated registry via parametric call `rule($R)` |
| Separate `op` keyword for reusable defs | `rule(name, $TERM*)` subsumes |
| `${rule.$V > $NEW}` rename via `>` inside carveout | capture-write station `> $NEW` as ordinary chain op (§10) |
| Scan-pointer as sigil class | relation rows with known `kind` (§24) |
| `$NAME` vs `${NAME}` duality | `$NAME` canonical; `${...}` is the carveout expr-hole |
| Separate `Rule` vs `Op` runtime | Rule is a named Pipeline; collapse (§17) |
| Norm as dedicated syntax | `is_repo_norm($R)` fact-op |
| Bash-style `>&N` content redirect | capture-write op + prolog mode |
| Full unification / SLD resolution as framework concern | shallow prolog only (mode dispatch); embed deep logic in `prolog(...)` op if ever needed |
| `re:pattern` prefix strings (v1) | `re("…")` with `$NAME` holes (§14) |
| `glob("…/$$$PATHS/…")` triple-sigil (v1) | `fs("…/$PATH/…")` single hole (§14) |
| `tag(...)` / `tag?(...)` (alias period) | `fact(...)` / `fact?(...)` — emits diag `tag/renamed-to-fact` during alias period |

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
  > ast[rust] { struct ${ENTITY_NAME} { $$${BODY} } }
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
