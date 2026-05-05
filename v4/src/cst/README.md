# `cst` — pattern-DSL v2 runtime for sprf v4

A sprf-blind library for plugging pattern dialects (regex, glob, JSON shape, ast-grep, future) into a host language. The lib answers exactly one question per DSL: *given pattern bytes and target bytes, what captures fire?* Everything else — composition, runtime, cursor flow, template-hole evaluation, cross-op dataflow — lives above this boundary.

## What it is

```
   compile  ::  Bytes ──► (Bytes ──► [CaptureRow])
                ^pattern^   ^target^

   Dsl       — factory for Compiled, plus optional LSP attachments.
   Compiled  — closure over a single pattern; runs against many targets.
   CaptureRow— { name: Arc<str>, kind: Span | Literal } emitted at match time.
```

Two-stage staged computation. Pattern compile cost is paid once per `Dsl::compile`; per-target match cost is paid per `Compiled::match_into`. Capture names are existential at the row level — never pre-declared.

## Trait surface

```rust
pub trait Dsl: Send + Sync + 'static {
    fn id(&self) -> &'static str;

    fn compile(
        &self,
        body:  &[u8],
        diags: &dyn DiagSink,
    ) -> Result<Box<dyn Compiled>, Diag>;

    fn injection_grammar(&self) -> Option<tree_sitter::Language> { None }
    fn lsp(&self) -> Option<&dyn DslBodyLsp> { None }
}

pub trait Compiled: Send + Sync {
    fn match_into(
        &self,
        target:     &[u8],
        target_off: usize,
        sink:       &mut dyn CaptureSink,
    );

    fn emit_path_items(&self, _body_byte: usize, _builder: &mut PathBuilder) {}
}
```

`target_off` is the absolute byte offset of `target` inside whatever larger document the consumer is walking. Spans emitted by `match_into` are pre-shifted into that absolute space, so consumers can mix rows from many calls without bookkeeping.

`DslBodyLsp` is a separate trait for body-pure LSP features (hover, semantic-tokens, folding, document symbols, diagnostics, …). It walks raw body bytes; the lib does not require a parsed body to live anywhere visible.

## Shipped DSLs

| id     | strategy                | body grammar           | capture syntax            |
|--------|-------------------------|------------------------|---------------------------|
| `re`   | tree-sitter grammar.js  | regex with extensions  | `(?<NAME>...)`            |
| `glob` | tree-sitter grammar.js  | glob with extensions   | `$NAME` per segment       |
| `json` | hand-rolled parser      | brace-pattern DSL      | `{X}` braces              |
| `ast`  | borrowed engine         | ast-grep syntactic     | `$NAME`, `$$$NAME`        |

All four implement the same trait. `re` and `glob` declare `injection_grammar`; the host parser injects the body grammar at sprf body ranges so syntax highlight, folding, and selection ranges fall out of tree-sitter for free. `json` and `ast` parse internally and return `None`.

## Three pattern-compilation strategies

```
   tree-sitter backed (re, glob)
   ─────────────────────────────
       body bytes ──ts parse──► tree ──lower──► automaton/regex
       compile error: parse error or lower error
       lsp:           injection_grammar + DslBodyLsp impl

   hand-rolled (json)
   ──────────────────
       body bytes ──brace_parse──► program ──compile──► walker plan
       compile error: brace_parse rejects malformed body
       lsp:           DslBodyLsp impl walks bytes itself

   borrowed engine (ast)
   ─────────────────────
       body bytes + lang ──ast_grep::Pattern::try_new──► Pattern + metavars
       compile error: ast-grep rejects (rare; permissive parser)
       lsp:           DslBodyLsp impl scans metavars by regex
```

The trait surface is identical; only the closure contents differ.

## Quick start

```rust
use v4::cst::{Dsl, Compiled, SilentSink, VecCaptureSink};
use v4::cst::dsls::re::ReDsl;

let dsl = ReDsl::new();
let compiled = dsl.compile(br"TODO\($WHO\)", &SilentSink).unwrap();

let mut sink = VecCaptureSink::new();
compiled.match_into(b"TODO(alice)", 0, &mut sink);

assert_eq!(&*sink.rows[0].name, "WHO");
```

For dynamic dispatch over a heterogeneous mix:

```rust
let dsls: Vec<Box<dyn Dsl>> = vec![
    Box::new(ReDsl::new()),
    Box::new(GlobDsl::new()),
    Box::new(JsonDsl::new()),
    Box::new(AstDsl::new(SupportLang::Rust)),
];
```

See `tests/cst_dogfood.rs` for the canonical cross-DSL test pattern.

## Where it sits in sprf v4

```
   sprf host source            host CST            lower
   "re(/TODO\\($X?\\)/)" ──► tree-sitter-sprefa ──► op tree
                                     │
                                     ▼
                       sprf-side dispatch (op_name → Dsl factory)
                                     │
                                     ▼
                            ┌────────────────────┐
                            │  cst lib boundary  │
                            └────────────────────┘
                                     │
                                     ▼
                       Dsl::compile(body) → Compiled
                                     │
                                     ▼
              wrap as sprf::SOp { compiled, body_off, ... }
                                     │
                                     ▼
                       impl effect_runtime::v2::Component for SOp
                            render(cursor) {
                              compiled.match_into(target, off, &mut sink);
                              Emit(sink.rows.into_iter().map(...))
                            }
```

The lib never sees:

- the host CST or other ops in the pipe
- the source file path, repo, rev, fs
- `${...}` template holes — sprf parses those at the host level and pre-substitutes bytes before `dsl.compile`
- the runtime cursor shape

Sprf never re-implements per op:

- pattern compilation + diagnostics
- regex / glob body grammar (re-use the injection grammar)
- body-level semantic tokens, hover, folding, document symbols
- capture-name extraction (rows carry names)

## Capture rows

```rust
pub struct CaptureRow {
    pub name: Arc<str>,
    pub kind: CaptureKind,
}

pub enum CaptureKind {
    Span    { byte_range: Range<usize> },
    Literal { value: Arc<[u8]> },
}
```

`Span` is the common case: the captured slice lives in the target. `Literal` covers DSL-synthesized values (ast multi-metavar `$$$XS` joins token text; future DSLs may compute values). Consumers join by name; the kind dictates whether to project a sub-cursor or carry the literal forward.

`CaptureSink` is a streaming interface; rows arrive one at a time. `VecCaptureSink` collects into a `Vec` for tests and LSP. Production runtime sinks can stop iteration via `ControlFlow::Break`.

## LSP path

The LSP path is orthogonal to the runtime path. Both walk the same `Compiled` instance (sharable via `Arc`), but:

- **runtime path**: `match_into(target, ...)` driven by per-document events.
- **lsp path**: `DslBodyLsp::*` driven by per-keystroke client requests.

`DslBodyLsp` methods take raw body bytes plus an optional position. The DSL caches its parsed body internally if it wants. Cross-DSL features (completion menu assembly, definition resolution across pipe stages, code actions touching multiple ops) live in sprf, where the host CST and the runtime cursor graph are visible.

```rust
pub trait DslBodyLsp: Send + Sync {
    fn hover            (&self, body: &[u8], byte: usize) -> Option<Hover>             { None }
    fn diagnostics      (&self, body: &[u8], diags: &dyn DiagSink)                     {}
    fn semantic_tokens  (&self, body: &[u8]) -> Vec<SemanticToken>                     { vec![] }
    fn folding_ranges   (&self, body: &[u8]) -> Vec<FoldingRange>                      { vec![] }
    fn document_symbols (&self, body: &[u8]) -> Vec<DocumentSymbol>                    { vec![] }
    // ...
}
```

Returned ranges are body-relative. `crate::cst::lsp::shift` lifts them into host-document positions.

## Adding a DSL

1. Pick a strategy (TS-backed, hand-rolled, borrowed engine).
2. For TS-backed DSLs: drop `grammar.js`, `parser.c`, headers, and `highlights.scm` under `dsls/<id>/`. `build.rs` picks them up. After adding a new TS-backed DSL, `touch build.rs` once to force the linker to pick up the new static lib.
3. Implement `Dsl` and `Compiled` in `dsls/<id>/mod.rs`.
4. (Optional) Implement `DslBodyLsp` for body-level LSP features.
5. Register in `dsls/mod.rs`.

The lib does not maintain a central registry by op name — that mapping is sprf's job, since `ast.rs` vs `ast.py` vs `ast(rust, ...)` is a host-grammar surface choice.

## What this design buys

- **Adding a DSL** touches one directory and one `pub use`. No edits to runtime, registry, LSP backend, op enum.
- **LSP body features** are free across DSLs that opt in. Sprf's LSP backend dispatches by walking the host CST and asking each DSL.
- **Pattern reuse**: the same `Compiled` can serve runtime matches and LSP queries via `Arc`.
- **Test isolation**: each DSL's unit tests cover its own grammar; the cross-DSL test (`tests/cst_dogfood.rs`) covers only the trait-object shape.

## What it deliberately does not do

- No template-hole (`${...}`) handling. That is host-level, evaluated by sprf before `compile` sees the body.
- No declared captures. Names emerge from rows. Pre-declaration would leak DSL internals.
- No central op enum. Sprf composes DSLs into ops; the lib does not name them.
- No structured target type. Target is `&[u8]`. DSLs that benefit from parsed-target sharing (json, ast) get caching via the sprf-side effect runtime, not via a wider trait.
- No streaming budget on `match_into` (yet). Likely the first additive bend if/when whole-repo bodies start saturating a tick.
- No bindings-aware compile (yet). The `compile(body, bindings)` extension lands when ast-grep MetaVarMatcher constraints become necessary; until then, sprf splices bound holes byte-wise before calling `compile`.

## Layout

```
v4/src/cst/
├── dsl.rs           Dsl, Compiled, CaptureRow, CaptureSink
├── diag.rs          Diag, Severity, DiagSink, BufferSink
├── doc.rs           Doc, DocId
├── injected.rs      Injected sub-tree
├── locate.rs        locate(byte) ↔ resolve(path)
├── path.rs          Path, PathItem, PathBuilder
├── store.rs         Store trait + MemStore
├── build/           build.rs grammar compile helpers
├── lsp/
│   ├── providers.rs DslBodyLsp trait + SemanticToken
│   ├── position.rs  byte ↔ line/col helpers
│   ├── highlights.rs shared highlights → semantic tokens
│   └── shift.rs     body-relative → host-document range lifting
└── dsls/
    ├── re/          tree-sitter backed
    ├── glob/        tree-sitter backed
    ├── json/        hand-rolled (walker + data parsers preserved from v3)
    └── ast/         borrowed engine (ast_grep_core)
```

## Tests

- `tests/cst_dogfood.rs` — cross-DSL trait-surface test. Holds all four DSLs behind `Vec<Box<dyn Dsl>>`, exercises `compile` + `match_into` with non-zero `target_off`. Catches object-safety drift, `Send + Sync + 'static` regressions, span arithmetic bugs.
- Per-DSL unit tests live alongside each `dsls/<id>/mod.rs`.
- `cargo test -p v4 --lib` for fast feedback; full integration tests via `cargo test -p v4`.

## Versioning policy for the trait

`Dsl::compile`, `Compiled::match_into`, `CaptureRow`, `CaptureKind` are the load-bearing surface. Changes to those are breaking and are versioned with the v4 crate. Everything else (`DslBodyLsp` methods, `emit_path_items`, ancillary helpers) can grow additively.
