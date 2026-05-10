---
name: sprf-lsp-lower-traits
description: [v4 planning] Trait shape for multi-DSL lowering — DslGrammar / Lower / Surface triad, biome Queryable+Rule+Services pattern, ast-grep prefilter trick. Load when designing or refactoring v3 op trait surface.
---

# Lowering traits across many DSLs

## The triad (proposal for v3)

```rust
// (A) Sub-language registration
pub trait DslGrammar {
    const NAME: &'static str;
    fn language() -> tree_sitter::Language;
    fn highlights() -> Option<&'static str> { None }
    fn injections() -> Option<&'static str> { None }
}

// (B) CST → typed State + diagnostics. Pure, Salsa-cacheable.
pub trait Lower {
    type Grammar: DslGrammar;
    type State;
    type Services;                                  // typed DI bag
    fn potential_kinds() -> &'static [u16];          // bitset prefilter
    fn required_substrings() -> &'static [&'static str] { &[] }   // AC prefilter
    fn lower(node: tree_sitter::Node<'_>, src: &[u8],
             ctx: LowerCtx<Self>)
        -> Result<Self::State, Vec<Box<dyn Diagnostic>>>;
}

// (C) State → IDE facts. Lazy, on-demand, throwaway.
pub trait Surface {
    type State;
    fn hover    (&self, _: &Self::State, _at: usize) -> Option<HoverFact>     { None }
    fn diags    (&self, _: &Self::State)             -> Vec<Box<dyn Diagnostic>> { vec![] }
    fn tokens   (&self, _: &Self::State)             -> Vec<SemanticTokenFact> { vec![] }
    fn symbols  (&self, _: &Self::State)             -> Vec<SymbolFact>        { vec![] }
    fn complete (&self, _: &Self::State, _at: usize) -> Vec<CompletionFact>    { vec![] }
    fn actions  (&self, _: &Self::State, _r: Range<usize>) -> Vec<ActionFact> { vec![] }
}
```

Why three not one: a single trait that returned every fact type would force every DSL to know every IDE feature at lower time. Splitting `Lower` (CST → State, diagnostics) from `Surface` (State → IDE facts, lazy) means lowering runs once per parse and surface methods run on demand per LSP request.

## Why the split exists in the wild

| Pattern | Owner | Notes |
|---|---|---|
| `Queryable` + `Rule` | biome | Queryable is the selector (CST kind), Rule is the consumer (`run(ctx) -> Vec<State>`). `diagnostic` and `action` are projections off `State`. |
| `Matcher` + `MatcherExt` | ast-grep | `do_match(node, env) -> Option<MatchEnv>` + algebra (and/or/not/inside/has). `potential_kinds() -> BitSet` for dispatch fan-out. |
| `Visitor` + `Rule` + bitset | oxc | Single mut visitor walks once; `AstTypesBitset` per rule gates dispatch. ~3x faster than biome on real codebases. |
| `Semantics<'db>` facade | rust-analyzer | God-object exposing all queries; rules pull what they need. Less testable. |
| `LateLintPass` | clippy | Per-node hooks, no kind bitset. Slowest model. |

The triad above takes biome's split + ast-grep's prefilter + ra's lazy projection.

## Services (typed DI, biome-style)

Each `Lower` declares what context it needs. Framework injects only that subset. Compiler enforces.

```
                    ┌─────────────────────────────────┐
                    │     Analyzer (one per file)     │
                    │  ServiceBag {                    │
                    │    Registry, RelationStore,     │
                    │    BindingGraph, EffectRuntime, │
                    │    RepoResolver, …              │
                    │  }                              │
                    └────────────────┬────────────────┘
                                     │ inject only declared subset
            ┌────────────────────────┼────────────────────────┐
            ▼                        ▼                        ▼
   regex Lower               tag Lower                   rule Lower
   Services = (Registry,)    Services = (Registry,       Services = (Registry,
                                         RelationStore)             BindingGraph,
                                                                    RuleResolver)
```

vs alternatives:

```
   No DI (clippy):                each rule pulls from a global; no per-rule contract
   God object (ra Semantics):     every rule sees every API; can't slim the facade
   Services (biome):              rule declares subset; framework parallelizes rules
                                  that share no services
```

## ast-grep prefilter (steal wholesale)

Two stages, both cheap:

```
   1. required_substrings() -> &[&str]
      Aho-Corasick scans source bytes once per parse.
      Pattern `console.log($X)` extracts `["console", "log"]`.
      AC produces candidate offsets in O(N).

   2. potential_kinds() -> &[u16]
      Tree-sitter dispatch only at offsets where the kind is in the bitset.
      O(1) per node.
```

Combined: O(N + matches × dispatch). Without it, dispatch over N DSLs is O(N) per CST node.

Free 5-10x speedup at ~100 LoC. Add now while op count is small; expensive to retrofit later.

## What this collapses in v3 today

| Today | Becomes |
|---|---|
| `pipeline::_1_op::Op::language()` / `highlights()` | `DslGrammar` |
| `pipeline::op_ctor::PatternCtor::compile_from_tree` | `Lower::lower` (one path) |
| `pipeline::op_ctor::OpLowering::lower` (escape hatch) | `Lower::lower` (same path) |
| Legacy `sprefa::op::{hover_self, hover_capture, hover_match}` | `Surface::hover` (one method) |
| Concrete `DocSession::{hover_at, completions_at}` | demux of `Surface::*` facts |

What stays: `pipeline::Op` (runtime trait) is unchanged. `lower::Pipeline / OpCall / Term / Rule` is the *output* of `Lower::lower` for the host DSL specifically. Each sub-DSL gets its own `State` type.

## Open questions

1. Is `Lower::State` a node in the existing `lower::Pipeline` IR, or a parallel per-DSL type the host IR references by `OpCall.callee`? Decides whether sub-DSL state is uniform or open.
2. Salsa now or after the trait collapse? See sprf-incr-salsa-cost.
